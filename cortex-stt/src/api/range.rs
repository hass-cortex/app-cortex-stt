//! Byte-range responses for the audio endpoints.
//!
//! A browser will not let a reader scrub a media element whose source
//! does not advertise range support: assigning `currentTime` is silently
//! reverted. Both audio routes already hold the whole file in memory, so
//! honouring `Range` is a matter of slicing what the domain handed back —
//! no second path resolution, which stays with the module that owns the
//! files.
//!
//! Only a single range is parsed. Multipart ranges exist in the spec and
//! in no media element's request.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The one satisfiable interval a request asked for, as inclusive offsets.
#[derive(Debug, PartialEq, Eq)]
enum Requested {
    /// No `Range` header, or one this server does not speak.
    Whole,
    Slice {
        start: u64,
        end: u64,
    },
    Unsatisfiable,
}

/// Parse `Range: bytes=…` for the three forms a media element sends:
/// `bytes=N-`, `bytes=N-M`, and the suffix form `bytes=-N`.
fn parse(headers: &HeaderMap, total: u64) -> Requested {
    let Some(raw) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return Requested::Whole;
    };
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return Requested::Whole;
    };
    // A comma means multiple ranges; serving the whole body is a legal
    // answer and keeps the response shape simple.
    if spec.contains(',') || total == 0 {
        return Requested::Whole;
    }
    let Some((from, to)) = spec.split_once('-') else {
        return Requested::Whole;
    };

    let (start, end) = match (from.trim(), to.trim()) {
        ("", "") => return Requested::Whole,
        // Suffix: the last N bytes.
        ("", n) => match n.parse::<u64>() {
            Ok(n) if n > 0 => (total.saturating_sub(n), total - 1),
            _ => return Requested::Unsatisfiable,
        },
        (s, "") => match s.parse::<u64>() {
            Ok(s) => (s, total - 1),
            Err(_) => return Requested::Unsatisfiable,
        },
        (s, e) => match (s.parse::<u64>(), e.parse::<u64>()) {
            (Ok(s), Ok(e)) => (s, e.min(total - 1)),
            _ => return Requested::Unsatisfiable,
        },
    };

    if start > end || start >= total {
        return Requested::Unsatisfiable;
    }
    Requested::Slice { start, end }
}

/// Answer a media request, honouring a single byte range when one is asked
/// for. Always advertises `Accept-Ranges`, which is what tells the browser
/// seeking is allowed at all.
pub fn audio_response(headers: &HeaderMap, bytes: Vec<u8>, mime: &'static str) -> Response {
    let total = bytes.len() as u64;

    match parse(headers, total) {
        Requested::Whole => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime.to_string()),
                (header::ACCEPT_RANGES, "bytes".to_string()),
            ],
            bytes,
        )
            .into_response(),

        Requested::Slice { start, end } => {
            let slice = bytes[start as usize..=end as usize].to_vec();
            (
                StatusCode::PARTIAL_CONTENT,
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (header::ACCEPT_RANGES, "bytes".to_string()),
                    (
                        header::CONTENT_RANGE,
                        format!("bytes {start}-{end}/{total}"),
                    ),
                ],
                slice,
            )
                .into_response()
        }

        Requested::Unsatisfiable => (
            StatusCode::RANGE_NOT_SATISFIABLE,
            [
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_RANGE, format!("bytes */{total}")),
            ],
            Body::empty(),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_range(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, value.parse().unwrap());
        headers
    }

    #[test]
    fn no_header_asks_for_the_whole_body() {
        assert_eq!(parse(&HeaderMap::new(), 100), Requested::Whole);
    }

    #[test]
    fn open_ended_range_runs_to_the_last_byte() {
        assert_eq!(
            parse(&with_range("bytes=10-"), 100),
            Requested::Slice { start: 10, end: 99 }
        );
    }

    #[test]
    fn closed_range_is_inclusive_and_clamped() {
        assert_eq!(
            parse(&with_range("bytes=10-20"), 100),
            Requested::Slice { start: 10, end: 20 }
        );
        assert_eq!(
            parse(&with_range("bytes=90-999"), 100),
            Requested::Slice { start: 90, end: 99 }
        );
    }

    #[test]
    fn suffix_range_counts_back_from_the_end() {
        assert_eq!(
            parse(&with_range("bytes=-10"), 100),
            Requested::Slice { start: 90, end: 99 }
        );
        // Longer than the file: the whole file satisfies it.
        assert_eq!(
            parse(&with_range("bytes=-500"), 100),
            Requested::Slice { start: 0, end: 99 }
        );
    }

    #[test]
    fn a_start_past_the_end_is_unsatisfiable() {
        assert_eq!(
            parse(&with_range("bytes=100-"), 100),
            Requested::Unsatisfiable
        );
        assert_eq!(
            parse(&with_range("bytes=50-10"), 100),
            Requested::Unsatisfiable
        );
    }

    #[test]
    fn multipart_and_unknown_units_fall_back_to_the_whole_body() {
        assert_eq!(
            parse(&with_range("bytes=0-10,20-30"), 100),
            Requested::Whole
        );
        assert_eq!(parse(&with_range("items=0-10"), 100), Requested::Whole);
    }

    #[test]
    fn an_empty_file_has_nothing_to_slice() {
        assert_eq!(parse(&with_range("bytes=0-"), 0), Requested::Whole);
    }

    #[test]
    fn a_slice_response_carries_the_content_range() {
        let response = audio_response(
            &with_range("bytes=2-4"),
            vec![0, 1, 2, 3, 4, 5],
            "audio/wav",
        );
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 2-4/6"
        );
    }

    #[test]
    fn a_whole_response_still_advertises_range_support() {
        let response = audio_response(&HeaderMap::new(), vec![0, 1, 2], "audio/wav");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
    }
}
