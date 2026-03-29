use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;

use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::engine::traits::*;
use wyoming_asr::error::AsrError;
use wyoming_asr::wyoming::event::{WyomingEvent, read_event, write_event};
use wyoming_asr::wyoming::handler::ConnectionHandler;
use wyoming_asr::wyoming::types::Transcript;

/// Mock engine that reports sample count in the transcription text.
struct MockEngine;

impl SpeechEngine for MockEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "mock".into(),
            languages: vec!["en".into()],
            supports_translation: false,
        }
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: format!("transcribed {} samples", samples.len()),
            segments: vec![],
        })
    }
}

fn mock_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
    Arc::new(|| Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>))
}

fn test_manager() -> Arc<EngineManager> {
    EngineManager::new(EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(10),
    })
}

/// Send a describe event and verify the response is an info event with an asr array.
#[tokio::test]
async fn test_handler_describe_returns_info() {
    let manager = test_manager();
    manager.register("test-model", mock_factory()).await;

    let handler = ConnectionHandler::new(
        Arc::clone(&manager),
        "test-model".to_string(),
        Duration::from_secs(30),
    );

    // Client→Handler channel: client writes events, handler reads them.
    let (client_tx, server_rx) = tokio::io::duplex(4096);
    // Handler→Client channel: handler writes responses, client reads them.
    let (server_tx, client_rx) = tokio::io::duplex(4096);

    let handler_task = tokio::spawn(async move {
        let mut reader = BufReader::new(server_rx);
        let mut writer = server_tx;
        handler.handle(&mut reader, &mut writer).await
    });

    // Client side: write a describe event then close.
    let mut client_writer = client_tx;
    let describe = WyomingEvent {
        event_type: "describe".to_string(),
        data: None,
        payload: None,
    };
    write_event(&mut client_writer, &describe).await.unwrap();
    // Drop the writer to signal EOF so the handler loop exits cleanly.
    drop(client_writer);

    // Read the info response from the handler.
    let mut client_reader = BufReader::new(client_rx);
    let response = read_event(&mut client_reader).await.unwrap().unwrap();

    assert_eq!(response.event_type, "info");
    let data = response.data.unwrap();
    let asr = data.get("asr").expect("info should have asr field");
    assert!(asr.is_array(), "asr should be an array");
    let programs = asr.as_array().unwrap();
    assert!(!programs.is_empty(), "asr array should not be empty");

    // Handler should exit cleanly on EOF.
    handler_task.await.unwrap().unwrap();
}

/// Send a full transcribe flow: transcribe → audio-start → audio-chunk → audio-stop.
/// Verify the transcript response contains the correct sample count.
#[tokio::test]
async fn test_handler_transcribe_flow() {
    let manager = test_manager();
    manager.register("test-model", mock_factory()).await;

    let handler = ConnectionHandler::new(
        Arc::clone(&manager),
        "test-model".to_string(),
        Duration::from_secs(30),
    );

    let (client_tx, server_rx) = tokio::io::duplex(65536);
    let (server_tx, client_rx) = tokio::io::duplex(65536);

    let handler_task = tokio::spawn(async move {
        let mut reader = BufReader::new(server_rx);
        let mut writer = server_tx;
        handler.handle(&mut reader, &mut writer).await
    });

    let mut client_writer = client_tx;

    // 1. Send transcribe event.
    let transcribe = WyomingEvent {
        event_type: "transcribe".to_string(),
        data: Some(serde_json::json!({"language": "en"})),
        payload: None,
    };
    write_event(&mut client_writer, &transcribe).await.unwrap();

    // 2. Send audio-start event.
    let audio_start = WyomingEvent {
        event_type: "audio-start".to_string(),
        data: Some(serde_json::json!({"rate": 16000, "width": 2, "channels": 1})),
        payload: None,
    };
    write_event(&mut client_writer, &audio_start).await.unwrap();

    // 3. Send audio-chunk with 32000 bytes of zeros (= 16000 i16 samples).
    let audio_chunk = WyomingEvent {
        event_type: "audio-chunk".to_string(),
        data: None,
        payload: Some(vec![0u8; 32000]),
    };
    write_event(&mut client_writer, &audio_chunk).await.unwrap();

    // 4. Send audio-stop to trigger transcription.
    let audio_stop = WyomingEvent {
        event_type: "audio-stop".to_string(),
        data: None,
        payload: None,
    };
    write_event(&mut client_writer, &audio_stop).await.unwrap();

    // Close the write side so the handler will eventually see EOF.
    drop(client_writer);

    // Read the transcript response.
    let mut client_reader = BufReader::new(client_rx);
    let response = read_event(&mut client_reader).await.unwrap().unwrap();

    assert_eq!(response.event_type, "transcript");
    let data = response.data.unwrap();
    let transcript: Transcript = serde_json::from_value(data).unwrap();

    // 32000 bytes / 2 bytes per sample = 16000 samples.
    assert!(
        transcript.text.contains("16000"),
        "expected transcript to contain '16000', got: {}",
        transcript.text
    );
    assert_eq!(transcript.text, "transcribed 16000 samples");

    // Handler should exit cleanly on EOF.
    handler_task.await.unwrap().unwrap();
}
