use serde::{Deserialize, Serialize};

use super::event::WyomingEvent;

pub fn describe_event() -> WyomingEvent {
    WyomingEvent {
        event_type: "describe".to_string(),
        data: None,
        payload: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrModel {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub installed: bool,
    pub attribution: Attribution,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrProgram {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub installed: bool,
    pub attribution: Attribution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub models: Vec<AsrModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub asr: Vec<AsrProgram>,
}

impl Info {
    pub fn to_event(&self) -> WyomingEvent {
        WyomingEvent {
            event_type: "info".to_string(),
            data: Some(serde_json::to_value(self).unwrap()),
            payload: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcribe {
    #[serde(default)]
    pub language: Option<String>,
}

impl Transcribe {
    pub fn from_event(event: &WyomingEvent) -> Self {
        event
            .data
            .as_ref()
            .and_then(|d| serde_json::from_value(d.clone()).ok())
            .unwrap_or(Transcribe { language: None })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStart {
    #[serde(default = "default_rate")]
    pub rate: u32,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_channels")]
    pub channels: u16,
    #[serde(default)]
    pub timestamp: Option<u64>,
}

fn default_rate() -> u32 {
    16000
}
fn default_width() -> u16 {
    2
}
fn default_channels() -> u16 {
    1
}

impl AudioStart {
    pub fn from_event(event: &WyomingEvent) -> Self {
        event
            .data
            .as_ref()
            .and_then(|d| serde_json::from_value(d.clone()).ok())
            .unwrap_or(AudioStart {
                rate: 16000,
                width: 2,
                channels: 1,
                timestamp: None,
            })
    }
}

pub fn audio_stop_event() -> WyomingEvent {
    WyomingEvent {
        event_type: "audio-stop".to_string(),
        data: None,
        payload: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
}

impl Transcript {
    pub fn to_event(&self) -> WyomingEvent {
        WyomingEvent {
            event_type: "transcript".to_string(),
            data: Some(serde_json::to_value(self).unwrap()),
            payload: None,
        }
    }
}
