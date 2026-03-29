use tracing::info;
#[cfg(feature = "zeroconf")]
use tracing::warn;

/// Announce this Wyoming ASR service via mDNS for auto-discovery.
///
/// Registers `_wyoming._tcp.local.` service so Home Assistant can find us.
/// Falls back to log-only when the `zeroconf` feature is not enabled.
pub async fn announce_discovery(wyoming_port: u16) {
    #[cfg(feature = "zeroconf")]
    {
        match register_mdns(wyoming_port) {
            Ok(()) => info!(wyoming_port, "Registered mDNS service _wyoming._tcp.local."),
            Err(e) => warn!(error = %e, "Failed to register mDNS service, discovery disabled"),
        }
    }

    #[cfg(not(feature = "zeroconf"))]
    {
        info!(
            wyoming_port,
            "Wyoming service ready (mDNS disabled, compile with --features zeroconf)"
        );
    }
}

#[cfg(feature = "zeroconf")]
fn register_mdns(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use mdns_sd::{ServiceDaemon, ServiceInfo};

    let mdns = ServiceDaemon::new()?;

    let hostname = gethostname();
    let service_type = "_wyoming._tcp.local.";
    let instance_name = format!("wyoming-asr-{hostname}");

    let service_info = ServiceInfo::new(
        service_type,
        &instance_name,
        &format!("{hostname}.local."),
        "", // auto-detect IP
        port,
        None, // no TXT properties needed
    )?;

    mdns.register(service_info)?;

    // Keep the daemon alive — it runs in background threads.
    // Leak it intentionally so the service stays registered for the process lifetime.
    std::mem::forget(mdns);

    Ok(())
}

#[cfg(feature = "zeroconf")]
fn gethostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "wyoming-asr".to_string())
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
