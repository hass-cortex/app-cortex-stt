use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A single Wyoming protocol event with optional JSON data and binary payload.
#[derive(Debug, Clone)]
pub struct WyomingEvent {
    pub event_type: String,
    pub data: Option<serde_json::Value>,
    pub payload: Option<Vec<u8>>,
}

/// Wire-format header serialized as a single JSON line.
#[derive(Debug, Serialize, Deserialize)]
struct EventHeader {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    version: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    data_length: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    payload_length: usize,
}

fn is_zero(v: &usize) -> bool {
    *v == 0
}

/// Read a single Wyoming event from an async buffered reader.
///
/// Returns `Ok(None)` on EOF (zero bytes read for the header line).
pub async fn read_event<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<WyomingEvent>, crate::error::AsrError> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(None);
    }

    let header: EventHeader =
        serde_json::from_str(line.trim()).map_err(|e| crate::error::AsrError::ProtocolError {
            detail: format!("invalid event header: {e}"),
        })?;

    let data = if header.data_length > 0 {
        let mut data_buf = vec![0u8; header.data_length];
        reader.read_exact(&mut data_buf).await?;
        let value: serde_json::Value = serde_json::from_slice(&data_buf).map_err(|e| {
            crate::error::AsrError::ProtocolError {
                detail: format!("invalid event data: {e}"),
            }
        })?;
        Some(value)
    } else {
        None
    };

    let payload = if header.payload_length > 0 {
        let mut payload_buf = vec![0u8; header.payload_length];
        reader.read_exact(&mut payload_buf).await?;
        Some(payload_buf)
    } else {
        None
    };

    Ok(Some(WyomingEvent {
        event_type: header.event_type,
        data,
        payload,
    }))
}

/// Write a single Wyoming event to an async writer.
pub async fn write_event<W: AsyncWrite + Unpin>(
    writer: &mut W,
    event: &WyomingEvent,
) -> Result<(), crate::error::AsrError> {
    let data_bytes = match &event.data {
        Some(value) => serde_json::to_vec(value)?,
        None => Vec::new(),
    };

    let payload_bytes = event.payload.as_deref().unwrap_or(&[]);

    let header = EventHeader {
        event_type: event.event_type.clone(),
        version: 1,
        data_length: data_bytes.len(),
        payload_length: payload_bytes.len(),
    };

    let header_json = serde_json::to_string(&header)?;
    writer.write_all(header_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    if !data_bytes.is_empty() {
        writer.write_all(&data_bytes).await?;
    }

    if !payload_bytes.is_empty() {
        writer.write_all(payload_bytes).await?;
    }

    writer.flush().await?;
    Ok(())
}
