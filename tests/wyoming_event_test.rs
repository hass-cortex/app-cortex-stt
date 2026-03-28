use wyoming_asr::wyoming::event::{WyomingEvent, read_event, write_event};

#[tokio::test]
async fn test_write_and_read_simple_event() {
    let event = WyomingEvent {
        event_type: "describe".to_string(),
        data: None,
        payload: None,
    };

    let mut buf = Vec::new();
    write_event(&mut buf, &event).await.unwrap();

    let mut cursor = tokio::io::BufReader::new(std::io::Cursor::new(buf));
    let read_back = read_event(&mut cursor).await.unwrap().unwrap();

    assert_eq!(read_back.event_type, "describe");
    assert!(read_back.data.is_none());
    assert!(read_back.payload.is_none());
}

#[tokio::test]
async fn test_write_and_read_event_with_data() {
    let data = serde_json::json!({"text": "hello world"});
    let event = WyomingEvent {
        event_type: "transcript".to_string(),
        data: Some(data.clone()),
        payload: None,
    };

    let mut buf = Vec::new();
    write_event(&mut buf, &event).await.unwrap();

    let mut cursor = tokio::io::BufReader::new(std::io::Cursor::new(buf));
    let read_back = read_event(&mut cursor).await.unwrap().unwrap();

    assert_eq!(read_back.event_type, "transcript");
    assert_eq!(read_back.data.unwrap()["text"], "hello world");
}

#[tokio::test]
async fn test_write_and_read_event_with_payload() {
    let pcm_data = vec![0u8; 3200];
    let data = serde_json::json!({"rate": 16000, "width": 2, "channels": 1});
    let event = WyomingEvent {
        event_type: "audio-chunk".to_string(),
        data: Some(data),
        payload: Some(pcm_data.clone()),
    };

    let mut buf = Vec::new();
    write_event(&mut buf, &event).await.unwrap();

    let mut cursor = tokio::io::BufReader::new(std::io::Cursor::new(buf));
    let read_back = read_event(&mut cursor).await.unwrap().unwrap();

    assert_eq!(read_back.event_type, "audio-chunk");
    assert_eq!(read_back.payload.unwrap().len(), 3200);
}

#[tokio::test]
async fn test_read_event_eof_returns_none() {
    let mut cursor = tokio::io::BufReader::new(std::io::Cursor::new(Vec::new()));
    let result = read_event(&mut cursor).await.unwrap();
    assert!(result.is_none());
}
