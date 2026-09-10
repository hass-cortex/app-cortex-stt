//! Resident-memory probe for evaluation runs.
//!
//! A model's real cost is what it adds to the process's resident set,
//! which the GGUF file size only approximates: transcribe.cpp does not
//! mmap weights, and each backend allocates its own scratch buffers.
//!
//! Lives in `eval` because it exists for one question — "does this
//! model fit?" — and nothing else in the service asks it.

/// Current process resident set size in bytes, or `None` where
/// `/proc/self/status` is unavailable (non-Linux dev hosts).
///
/// `VmRSS` is reported in kB by the kernel regardless of page size.
pub fn resident_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

/// Bytes the kernel reports as available to allocate without swapping.
pub fn available_bytes() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

/// Resident growth from `before` to now, floored at zero.
///
/// A model load can coincide with an unrelated free elsewhere in the
/// process; a negative delta means the measurement was swamped by that
/// noise, not that the model costs nothing, so it reads as unknown-but-
/// small rather than as a negative size.
pub fn growth_since(before: u64) -> u64 {
    resident_bytes().unwrap_or(before).saturating_sub(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore)]
    fn the_process_reports_a_plausible_resident_size() {
        let rss = resident_bytes().expect("/proc/self/status readable on linux");
        // A running test binary is never smaller than a megabyte, and a
        // parse that picked up the wrong field would land far outside.
        assert!(rss > 1 << 20, "implausibly small RSS: {rss}");
        assert!(rss < 1 << 40, "implausibly large RSS: {rss}");
    }

    #[test]
    fn growth_never_goes_negative() {
        let huge = u64::MAX / 2;
        assert_eq!(growth_since(huge), 0);
    }
}
