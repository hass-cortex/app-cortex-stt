//! URL reachability tests — makes HEAD requests, no downloads.
//! Skips entirely if network is unavailable.

use cortex_stt::model::catalog_data::catalog_models;

#[tokio::test]
async fn all_catalog_default_quant_urls_are_reachable() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let mut failures = Vec::new();

    for m in catalog_models() {
        let q = m.default_quant_file();
        match client.head(&q.url).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {}
            Ok(resp) => {
                failures.push(format!("{} ({}): HTTP {}", m.id, q.quant, resp.status()));
            }
            Err(e) => {
                if e.is_connect() || e.is_timeout() {
                    eprintln!("SKIP: network unavailable — {e}");
                    return;
                }
                failures.push(format!("{} ({}): {e}", m.id, q.quant));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "URL verification failures:\n{}",
        failures.join("\n")
    );
}
