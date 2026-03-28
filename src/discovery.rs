use tracing::info;

/// Log that the Wyoming service is ready for discovery.
///
/// This is a placeholder for future HA discovery integration (e.g., Zeroconf/mDNS
/// announcement). Currently it simply logs the port so operators know the
/// service is accepting connections.
pub async fn announce_discovery(wyoming_port: u16) {
    info!(wyoming_port, "Wyoming service ready for discovery");
}

/// Poll until the Wyoming TCP server is accepting connections, or time out.
///
/// Returns `true` if a connection was successfully established before the
/// deadline, `false` if the timeout elapsed.
pub async fn wait_for_ready(host: &str, port: u16, timeout_secs: u64) -> bool {
    use tokio::net::TcpStream;

    let addr = format!("{host}:{port}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(&addr).await.is_ok() {
            info!("Wyoming server is ready at {addr}");
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    false
}
