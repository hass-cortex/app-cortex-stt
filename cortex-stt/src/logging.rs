//! Process-wide log setup: one JSON line per event, with the native
//! inference library's diagnostics routed through the same subscriber.

use tracing_subscriber::EnvFilter;

/// transcribe.cpp / ggml report buffer sizes and weight copies at INFO on
/// every model load and inference; keep them out of the default level.
const NATIVE_TARGET: &str = "transcribe_cpp";
const NATIVE_DEFAULT: &str = "transcribe_cpp=warn";

/// The filter for `spec` (a `RUST_LOG` directive string). The native
/// library's target stays at WARN unless `spec` names it.
pub fn filter_directives(spec: &str) -> String {
    let spec = spec.trim();
    if spec.contains(NATIVE_TARGET) {
        spec.to_string()
    } else if spec.is_empty() {
        format!("info,{NATIVE_DEFAULT}")
    } else {
        format!("{spec},{NATIVE_DEFAULT}")
    }
}

/// Install the global subscriber. Call once, before any model loads.
pub fn init(spec: &str) {
    let filter = EnvFilter::try_new(filter_directives(spec))
        .unwrap_or_else(|_| EnvFilter::new(filter_directives("info")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .flatten_event(true)
        // The innermost span (request_id, job_id, …) is enough; the full
        // list repeats it on every line.
        .with_span_list(false)
        .init();
    // Without this the native library writes unstructured lines to stderr.
    #[cfg(feature = "engine")]
    transcribe_cpp::init_logging();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_noise_is_kept_below_info_by_default() {
        let f = filter_directives("info");
        assert!(f.contains("transcribe_cpp=warn"), "{f}");
    }

    #[test]
    fn an_explicit_native_directive_wins() {
        let f = filter_directives("info,transcribe_cpp=debug");
        assert!(!f.contains("transcribe_cpp=warn"), "{f}");
    }
}
