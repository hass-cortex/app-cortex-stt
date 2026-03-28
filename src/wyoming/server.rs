use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::engine::manager::EngineManager;
use crate::wyoming::handler::ConnectionHandler;

/// Start the Wyoming TCP server and accept connections forever.
///
/// Each incoming connection is handled in its own spawned task with an
/// independent [`ConnectionHandler`]. The function runs until the listener
/// encounters an unrecoverable error.
pub async fn run_wyoming_server(
    host: &str,
    port: u16,
    default_model: String,
    transcription_timeout: Duration,
    engine_manager: Arc<EngineManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await?;
    info!("Wyoming server listening on {addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        let engine_manager = engine_manager.clone();
        let default_model = default_model.clone();

        tokio::spawn(async move {
            info!(%peer, "Wyoming client connected");
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut writer = write_half;

            let handler = ConnectionHandler::new(default_model, transcription_timeout);
            if let Err(e) = handler
                .handle(&mut reader, &mut writer, &engine_manager)
                .await
            {
                error!(%peer, error = %e, "Handler error");
            }
            info!(%peer, "Wyoming client disconnected");
        });
    }
}
