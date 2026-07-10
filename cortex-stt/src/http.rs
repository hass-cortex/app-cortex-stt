//! Shared pooled HTTP client. Built once: a fresh `Client` per call
//! would re-pay DNS/TLS setup and never reuse a keep-alive connection.
//! Used by model downloads and every Supervisor call. The timeouts
//! bound a stalled connection — generous enough not to trip a
//! slow-but-live model download; per-request `.timeout(..)` overrides
//! apply where a call must give up sooner (e.g. fire-and-forget events).

use reqwest::Client;

pub(crate) fn client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build shared HTTP client")
    })
}
