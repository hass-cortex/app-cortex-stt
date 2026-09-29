mod test_helpers;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use tower::ServiceExt;

use cortex_stt::api::metrics::metrics_routes;
use cortex_stt::history::{CreateRecord, TranscriptionSource};
use cortex_stt::state::AppState;
use test_helpers::test_state;

fn record() -> CreateRecord {
    CreateRecord {
        source: TranscriptionSource::HttpApi,
        language: Some("zh".into()),
        model_id: "whisper-small".into(),
        audio_duration_ms: 1000,
        inference_ms: 100,
        model_load_ms: 0,
        pool_wait_ms: 0,
        cold_load_ms: 0,
        text: "text".into(),
        raw_text: None,
        segments: Vec::new(),
        has_error: false,
        error_message: None,
        api_key_id: None,
        device: "cpu".into(),
        capture_device: None,
        rms_db: None,
        peak_db: None,
        clip_ratio: None,
    }
}

/// Insert one record stamped at local `hour`:00 in `tz`, `days_ago` local days back.
async fn insert_at_local(state: &Arc<AppState>, tz: Tz, days_ago: i64, hour: u32) {
    let day = Utc::now().with_timezone(&tz).date_naive() - Duration::days(days_ago);
    let local = day.and_time(NaiveTime::from_hms_opt(hour, 0, 0).unwrap());
    let stamp = tz
        .from_local_datetime(&local)
        .unwrap()
        .with_timezone(&Utc)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let id = state.history.create(record(), None).await.unwrap();
    state
        .db
        .connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE records SET timestamp = ?1 WHERE id = ?2",
                rusqlite::params![stamp, id],
            )
        })
        .await
        .unwrap();
}

async fn get_metrics(state: Arc<AppState>, uri: &str) -> serde_json::Value {
    let app = Router::new().merge(metrics_routes()).with_state(state);
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Either record straddles UTC midnight's view of "today" whatever the
/// wall clock says: 02:00 Taipei is 18:00 UTC the day before, 23:00
/// Taipei yesterday is 15:00 UTC.
async fn seed_taipei_day_edges(state: &Arc<AppState>) {
    let tz: Tz = "Asia/Taipei".parse().unwrap();
    insert_at_local(state, tz, 0, 2).await;
    insert_at_local(state, tz, 1, 23).await;
}

#[tokio::test]
async fn today_is_the_calendar_day_in_the_configured_timezone() {
    let (state, _tmp) = test_state().await;
    let mut settings = state.db.load_settings().await.unwrap();
    settings.timezone = "Asia/Taipei".into();
    state.db.save_settings(&settings).await.unwrap();
    seed_taipei_day_edges(&state).await;

    // The configured zone wins over whatever browser zone the viewer sends.
    let m = get_metrics(state, "/api/metrics?tz=America/New_York").await;
    assert_eq!(m["today_transcriptions"], 1);
    assert_eq!(m["today_audio_duration_ms"], 1000);
    assert_eq!(m["total_transcriptions"], 2);
}

#[tokio::test]
async fn auto_timezone_uses_the_browser_zone() {
    let (state, _tmp) = test_state().await;
    seed_taipei_day_edges(&state).await;

    let m = get_metrics(state, "/api/metrics?tz=Asia/Taipei").await;
    assert_eq!(m["today_transcriptions"], 1);
}
