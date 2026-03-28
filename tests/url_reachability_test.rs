//! URL reachability tests — makes HEAD requests, no downloads.
//! Skips entirely if network is unavailable.

use wyoming_asr::engine::registry::builtin_models;

#[tokio::test]
async fn all_registry_urls_are_reachable() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let models = builtin_models();
    let mut failures = Vec::new();

    for m in &models {
        match client.head(&m.url).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {}
            Ok(resp) => {
                failures.push(format!("{}: HTTP {}", m.id, resp.status()));
            }
            Err(e) => {
                if e.is_connect() || e.is_timeout() {
                    eprintln!("SKIP: network unavailable — {e}");
                    return;
                }
                failures.push(format!("{}: {e}", m.id));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "URL verification failures:\n{}",
        failures.join("\n")
    );
}
