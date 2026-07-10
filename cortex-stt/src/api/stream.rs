//! WebSocket streaming transcription — the sole streaming transport
//! (ADR 0001).
//!
//! Wire protocol (client → server):
//! - text `{"type":"start", "model", "language"?, "translate"?,
//!   "initial_prompt"?, "itn"?, "timestamps"?, "format"?, "sample_rate"?,
//!   "channels"?}` — must be the first message
//! - binary frames: PCM audio chunks (default `pcm_s16le` @ 16 kHz mono)
//! - text `{"type":"finalize"}` — flush and produce the final transcript
//! - closing (or dropping) the socket before finalize aborts the session
//!
//! Server → client (text frames):
//! - `{"type":"ready", "streaming": bool}` — after the engine slot is
//!   acquired; `streaming=false` means buffered fallback (no partials)
//! - `{"type":"partial", "text", "committed", "tentative", "revision"}`
//! - `{"type":"final", ...TranscribeResponse}` — then the socket closes
//! - `{"type":"error", "code", "message", "model_id"?}` — terminal

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::any;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::auth::AuthKeyId;
use crate::api::transcribe::normalize_language;
use crate::audio::canonical::SAMPLE_RATE;
use crate::audio::resample::raw_pcm_to_f32;
use crate::engine::traits::{Timestamps, TranscribeOptions};
use crate::error::AsrError;
use crate::history::TranscriptionSource;
use crate::state::AppState;
use crate::transcriber::{StreamMeta, TranscribeResponse};

/// A stream session holds an engine pool slot; a client that goes silent
/// (TCP half-open, wedged pipeline) must not hold it forever. Applied to
/// every socket receive — HA Assist feeds continuously, so a healthy
/// session never comes close.
const RECV_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Receive the next frame, bounding client silence.
async fn recv_frame(
    socket: &mut WebSocket,
) -> Result<Option<Result<Message, axum::Error>>, AsrError> {
    tokio::time::timeout(RECV_IDLE_TIMEOUT, socket.recv())
        .await
        .map_err(|_| AsrError::StreamProtocol {
            detail: format!(
                "no client frames for {}s; closing idle stream",
                RECV_IDLE_TIMEOUT.as_secs()
            ),
        })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Start(StartMessage),
    Finalize,
}

#[derive(Debug, Deserialize)]
struct StartMessage {
    model: String,
    language: Option<String>,
    #[serde(default)]
    translate: bool,
    initial_prompt: Option<String>,
    itn: Option<bool>,
    #[serde(default)]
    timestamps: Timestamps,
    /// Only `pcm_s16le` is supported.
    format: Option<String>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    /// Capture device (microphone / satellite) that recorded the audio.
    capture_device: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    Ready {
        streaming: bool,
    },
    Partial {
        text: String,
        committed: String,
        tentative: String,
        revision: i32,
    },
    Final {
        #[serde(flatten)]
        response: TranscribeResponse,
    },
    Error {
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
    },
}

impl ServerEvent {
    fn error(e: &AsrError) -> Self {
        ServerEvent::Error {
            code: e.code().to_string(),
            message: e.to_string(),
            model_id: e.related_id(),
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &ServerEvent) -> bool {
    let payload = serde_json::to_string(event).unwrap_or_default();
    socket.send(Message::Text(payload.into())).await.is_ok()
}

/// GET /api/transcribe/stream — upgrade to the streaming protocol.
/// Auth runs in the shared middleware (`?api_key=` or HA ingress).
async fn stream_upgrade(
    State(state): State<Arc<AppState>>,
    auth_key: Option<axum::Extension<AuthKeyId>>,
    ws: WebSocketUpgrade,
) -> Response {
    let api_key_id = auth_key.map(|ext| ext.0.0);
    ws.on_upgrade(move |socket| handle_socket(state, socket, api_key_id))
}

async fn handle_socket(state: Arc<AppState>, mut socket: WebSocket, api_key_id: Option<String>) {
    // ── 1. The first message must be `start`. ────────────────────────
    let start = loop {
        let frame = match recv_frame(&mut socket).await {
            Ok(f) => f,
            Err(e) => {
                send_event(&mut socket, &ServerEvent::error(&e)).await;
                return;
            }
        };
        match frame {
            Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                Ok(ClientMessage::Start(start)) => break start,
                Ok(ClientMessage::Finalize) | Err(_) => {
                    let e = AsrError::StreamProtocol {
                        detail: "first message must be a start message".to_string(),
                    };
                    send_event(&mut socket, &ServerEvent::error(&e)).await;
                    return;
                }
            },
            // Answered automatically by axum.
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return,
        }
    };

    if let Err(e) = validate_start(&start) {
        send_event(&mut socket, &ServerEvent::error(&e)).await;
        return;
    }

    let options = TranscribeOptions {
        language: normalize_language(start.language.clone()),
        translate: start.translate,
        initial_prompt: start.initial_prompt.clone(),
        itn: start.itn,
        timestamps: start.timestamps,
    };
    let meta = StreamMeta {
        model: start.model.clone(),
        language: start.language.clone(),
        source: TranscriptionSource::WsApi,
        api_key_id,
        capture_device: crate::api::transcribe::clamp_capture_device(start.capture_device.clone()),
    };

    // ── 2. Acquire the engine slot / open the stream. ────────────────
    let mut session = match state.transcriber.open_stream(meta, options).await {
        Ok(s) => s,
        Err(e) => {
            send_event(&mut socket, &ServerEvent::error(&e)).await;
            return;
        }
    };

    if !send_event(
        &mut socket,
        &ServerEvent::Ready {
            streaming: session.is_streaming(),
        },
    )
    .await
    {
        return; // client went away; session Drop writes the aborted row
    }

    // ── 3. Pump frames until finalize / disconnect. ───────────────────
    let mut last_revision = i32::MIN;
    loop {
        let frame = match recv_frame(&mut socket).await {
            Ok(f) => f,
            Err(e) => {
                // Idle client: report, then drop the session (aborted row).
                send_event(&mut socket, &ServerEvent::error(&e)).await;
                return;
            }
        };
        match frame {
            Some(Ok(Message::Binary(data))) => {
                let samples = match raw_pcm_to_f32(&data, SAMPLE_RATE, 1) {
                    Ok(s) => s,
                    Err(e) => {
                        send_event(&mut socket, &ServerEvent::error(&e)).await;
                        return;
                    }
                };
                match session.feed(samples).await {
                    Ok(Some(snapshot)) if snapshot.revision != last_revision => {
                        last_revision = snapshot.revision;
                        let event = ServerEvent::Partial {
                            text: snapshot.display,
                            committed: snapshot.committed,
                            tentative: snapshot.tentative,
                            revision: snapshot.revision,
                        };
                        if !send_event(&mut socket, &event).await {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        send_event(&mut socket, &ServerEvent::error(&e)).await;
                        return;
                    }
                }
            }
            Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                Ok(ClientMessage::Finalize) => {
                    match session.finalize().await {
                        Ok(response) => {
                            send_event(&mut socket, &ServerEvent::Final { response }).await;
                        }
                        Err(e) => {
                            send_event(&mut socket, &ServerEvent::error(&e)).await;
                        }
                    }
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
                Ok(ClientMessage::Start(_)) | Err(_) => {
                    let e = AsrError::StreamProtocol {
                        detail: "unexpected message during active stream".to_string(),
                    };
                    send_event(&mut socket, &ServerEvent::error(&e)).await;
                    return;
                }
            },
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                debug!("stream socket closed before finalize; aborting session");
                // Dropping the session persists the aborted history row.
                return;
            }
        }
    }
}

fn validate_start(start: &StartMessage) -> Result<(), AsrError> {
    if let Some(format) = start.format.as_deref() {
        if format != "pcm_s16le" {
            return Err(AsrError::StreamProtocol {
                detail: format!("unsupported format {format:?} (only pcm_s16le)"),
            });
        }
    }
    if start.sample_rate.is_some_and(|sr| sr != SAMPLE_RATE) {
        return Err(AsrError::StreamProtocol {
            detail: format!("unsupported sample_rate (only {SAMPLE_RATE})"),
        });
    }
    if start.channels.is_some_and(|ch| ch != 1) {
        return Err(AsrError::StreamProtocol {
            detail: "unsupported channels (only mono)".to_string(),
        });
    }
    if start.model.is_empty() {
        return Err(AsrError::StreamProtocol {
            detail: "model is required".to_string(),
        });
    }
    Ok(())
}

/// Routes for the WebSocket streaming API.
pub fn stream_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/transcribe/stream", any(stream_upgrade))
}
