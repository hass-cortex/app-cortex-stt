# Device Selection, Timezone, Multi-Expand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-model CPU/GPU device selection, timezone-aware history display, and multi-expand in history list.

**Architecture:** Three independent features implemented in order of complexity: multi-expand (frontend-only), timezone (frontend + small backend), device selection (backend + frontend). Each feature is committed independently.

**Tech Stack:** Rust (axum, rusqlite, ort), TypeScript/React (TanStack Query, Tailwind CSS)

---

### Task 1: History Multi-Expand

**Files:**
- Modify: `web/src/components/history/history-list.tsx:49,60-62,114-122,183`

- [ ] **Step 1: Change state from single to multi-expand**

In `web/src/components/history/history-list.tsx`, replace the single `expandedId` state and toggle function:

```typescript
// Replace line 49:
// const [expandedId, setExpandedId] = useState<string | null>(null);
const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

// Replace lines 60-62 (toggleExpand function):
const toggleExpand = (id: string) => {
  setExpandedIds((prev) => {
    const next = new Set(prev);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    return next;
  });
};
```

- [ ] **Step 2: Update isExpanded check in HistoryRow**

Find where `expandedId === record.id` is used (around line 116) and replace with:

```typescript
const isExpanded = expandedIds.has(record.id);
```

- [ ] **Step 3: Add Collapse All button**

In the filter bar area (after the filter controls, before the records list), add:

```tsx
{expandedIds.size >= 2 && (
  <Button
    size="sm"
    variant="ghost"
    onClick={() => setExpandedIds(new Set())}
  >
    Collapse All
  </Button>
)}
```

Import `Button` if not already imported.

- [ ] **Step 4: Verify in browser**

Run: `cd web && bun run build`

Open the History page, expand multiple records, verify:
- Multiple records stay expanded simultaneously
- "Collapse All" button appears when ≥2 are expanded
- Clicking "Collapse All" closes all
- Clicking an expanded record's chevron collapses only that one

- [ ] **Step 5: Commit**

```bash
git add web/src/components/history/history-list.tsx
git commit -m "feat(web): support multi-expand in history list"
```

---

### Task 2: History Timezone Display — Backend Settings

**Files:**
- Modify: `src/api/settings.rs:33-49`

- [ ] **Step 1: Add timezone field to Settings struct**

In `src/api/settings.rs`, add the `timezone` field to the `Settings` struct (after `log_level`):

```rust
pub struct Settings {
    pub default_model: String,
    pub pool_size: usize,
    pub max_loaded_models: usize,
    pub idle_timeout_secs: Option<u64>,
    pub transcription_timeout_secs: u64,
    pub save_audio: bool,
    pub audio_retention: RetentionPolicy,
    pub record_retention: RetentionPolicy,
    #[serde(default)]
    pub preload_default_model: bool,
    pub cors_allowed_origins: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Timezone for display. "auto" = browser detection, or IANA timezone (e.g., "Asia/Taipei")
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "auto".into()
}
```

Add `timezone: default_timezone()` to the `Default` impl for `Settings`.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all existing tests pass (settings serialization tests still work due to `#[serde(default)]`)

- [ ] **Step 4: Commit**

```bash
git add src/api/settings.rs
git commit -m "feat(api): add timezone setting"
```

---

### Task 3: History Timezone Display — Frontend

**Files:**
- Create: `web/src/utils/time.ts`
- Modify: `web/src/api/types.ts:154-166`
- Modify: `web/src/components/history/history-list.tsx` (timestamp display)
- Modify: `web/src/components/settings/` (add timezone selector)

- [ ] **Step 1: Add timezone to AppSettings type**

In `web/src/api/types.ts`, add to the `AppSettings` interface:

```typescript
export interface AppSettings {
  // ... existing fields ...
  log_level: string;
  timezone: string;  // "auto" or IANA timezone
}
```

- [ ] **Step 2: Create formatTimestamp utility**

Create `web/src/utils/time.ts`:

```typescript
/**
 * Format a UTC timestamp string for display in the given timezone.
 * @param utcTimestamp - ISO 8601 or UTC date string from the API
 * @param timezone - IANA timezone (e.g., "Asia/Taipei") or "auto" for browser default
 */
export function formatTimestamp(utcTimestamp: string, timezone: string): string {
  const date = new Date(utcTimestamp.endsWith("Z") ? utcTimestamp : `${utcTimestamp}Z`);
  const tz = timezone === "auto" ? getBrowserTimezone() : timezone;
  return date.toLocaleString("default", {
    timeZone: tz,
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/**
 * Detect browser timezone via Intl API.
 */
export function getBrowserTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return "UTC";
  }
}

/** Common timezones for the settings dropdown. */
export const COMMON_TIMEZONES = [
  { value: "auto", label: "Auto (detect from browser)" },
  { value: "Asia/Taipei", label: "Asia/Taipei (UTC+8)" },
  { value: "Asia/Tokyo", label: "Asia/Tokyo (UTC+9)" },
  { value: "Asia/Shanghai", label: "Asia/Shanghai (UTC+8)" },
  { value: "America/New_York", label: "America/New York (UTC-5/-4)" },
  { value: "America/Los_Angeles", label: "America/Los Angeles (UTC-8/-7)" },
  { value: "Europe/London", label: "Europe/London (UTC+0/+1)" },
  { value: "Europe/Berlin", label: "Europe/Berlin (UTC+1/+2)" },
  { value: "UTC", label: "UTC" },
];
```

- [ ] **Step 3: Update history list to use timezone formatting**

In `web/src/components/history/history-list.tsx`:

1. Import the utility and settings hook:
```typescript
import { formatTimestamp } from "@/utils/time";
import { useSettings } from "@/hooks/use-settings";
```

2. Inside the component, get the timezone:
```typescript
const { data: settings } = useSettings();
const timezone = settings?.timezone ?? "auto";
```

3. Replace all `formatRelativeTime(record.timestamp)` calls in the summary row with the new format. Keep relative time but add tooltip with absolute time, or replace timestamp display in ExpandedDetail (around line 208-210) with:
```typescript
<span>{formatTimestamp(record.timestamp, timezone)}</span>
```

Also update the summary row timestamp display.

- [ ] **Step 4: Add timezone selector to Settings page**

Create `web/src/components/settings/timezone-settings.tsx`:

```tsx
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { getBrowserTimezone, COMMON_TIMEZONES } from "@/utils/time";
import { Save } from "lucide-react";
import { useState } from "react";

export function TimezoneSettings() {
  const { data: settings } = useSettings();
  const updateMutation = useUpdateSettings();
  const { toast } = useToast();
  const [timezone, setTimezone] = useState(settings?.timezone ?? "auto");

  const resolvedTz = timezone === "auto" ? getBrowserTimezone() : timezone;

  return (
    <Card>
      <CardHeader title="Timezone" description="Configure how timestamps are displayed" />
      <div className="space-y-3">
        <Select
          options={COMMON_TIMEZONES.map((tz) => ({ value: tz.value, label: tz.label }))}
          value={timezone}
          onChange={(e) => setTimezone(e.target.value)}
        />
        {timezone === "auto" && (
          <p className="text-xs text-text-muted">Detected: {resolvedTz}</p>
        )}
        <Button
          size="sm"
          icon={<Save size={14} />}
          onClick={() =>
            updateMutation.mutate(
              { timezone },
              {
                onSuccess: () => toast("Timezone saved", "success"),
                onError: (err) => toast(`Failed: ${err.message}`, "error"),
              },
            )
          }
          loading={updateMutation.isPending}
        >
          Save
        </Button>
      </div>
    </Card>
  );
}
```

Import and add `<TimezoneSettings />` to `web/src/pages/settings.tsx` alongside other settings cards.

- [ ] **Step 5: Build and verify**

Run: `cd web && bun run build`

Verify:
- History page shows timestamps in local time
- Settings page shows timezone selector
- Changing timezone updates history display
- "Auto" shows detected timezone

- [ ] **Step 6: Commit**

```bash
git add web/src/utils/time.ts web/src/api/types.ts web/src/components/history/history-list.tsx web/src/components/settings/timezone-settings.tsx web/src/pages/settings.tsx
git commit -m "feat(web): timezone-aware history timestamps with configurable timezone"
```

---

### Task 4: Device Selection — Backend Data Model

**Files:**
- Modify: `src/api/settings.rs:33-49`
- Modify: `src/db/database.rs:53-67` (migration)
- Modify: `src/db/records.rs:36-48,51-66,81-122`

- [ ] **Step 1: Add ComputeDevice enum and device_overrides to Settings**

In `src/api/settings.rs`, add before the `Settings` struct:

```rust
use std::collections::HashMap;

/// Compute device preference for a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ComputeDevice {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
```

Add to the `Settings` struct:

```rust
    /// Per-model compute device override. Key = model_id.
    #[serde(default)]
    pub device_overrides: HashMap<String, ComputeDevice>,
```

Add `device_overrides: HashMap::new()` to the `Default` impl.

- [ ] **Step 2: Add device column to records table**

In `src/db/database.rs`, add a migration after the initial schema creation. Find the `open` function and add after the CREATE TABLE statements:

```rust
// Migration: add device column to records
conn.call(|conn| {
    // Check if column exists first
    let has_device: bool = conn
        .prepare("SELECT COUNT(*) FROM pragma_table_info('records') WHERE name='device'")?
        .query_row([], |row| row.get::<_, i64>(0))
        .map(|c| c > 0)?;
    if !has_device {
        conn.execute_batch(
            "ALTER TABLE records ADD COLUMN device TEXT NOT NULL DEFAULT 'cpu';"
        )?;
    }
    Ok(())
})
.await
.map_err(|e| AsrError::DatabaseError(format!("Migration failed: {e}")))?;
```

- [ ] **Step 3: Add device to CreateRecord and TranscriptionRecord**

In `src/db/records.rs`, add `device` field to both structs:

```rust
pub struct CreateRecord {
    // ... existing fields ...
    pub api_key_id: Option<String>,
    pub device: String,  // "cpu" or "cuda"
}

pub struct TranscriptionRecord {
    // ... existing fields ...
    pub api_key_id: Option<String>,
    pub device: String,
}
```

Update `insert_record` to include `device` in the INSERT statement (add parameter and column).

Update `list_records` and `get_record` to read the `device` column.

- [ ] **Step 4: Verify compilation and run tests**

Run: `cargo check && cargo test`
Expected: compilation succeeds, tests pass

- [ ] **Step 5: Commit**

```bash
git add src/api/settings.rs src/db/database.rs src/db/records.rs
git commit -m "feat: add ComputeDevice settings and device column to records"
```

---

### Task 5: Device Selection — Engine Integration

**Files:**
- Modify: `src/engine/traits.rs:37-47`
- Modify: `src/engine/onnx_bridge.rs:15-18,64-141`
- Modify: `src/engine/register.rs:21-55,60-102`
- Modify: `src/main.rs` (pass settings to register)

- [ ] **Step 1: Add device method to SpeechEngine trait**

In `src/engine/traits.rs`, add to the `SpeechEngine` trait:

```rust
pub trait SpeechEngine: Send {
    fn capabilities(&self) -> EngineCapabilities;
    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError>;
    /// Returns the compute device this engine instance is using ("cpu" or "cuda").
    fn device(&self) -> &str {
        "cpu"  // default implementation
    }
}
```

- [ ] **Step 2: Update OnnxBridge to track and report device**

In `src/engine/onnx_bridge.rs`, add a `device` field to `OnnxBridge` and implement the trait method:

```rust
pub struct OnnxBridge {
    engine: Box<dyn SpeechModel>,
    device: String,
}

impl SpeechEngine for OnnxBridge {
    // ... existing capabilities() and transcribe() unchanged ...

    fn device(&self) -> &str {
        &self.device
    }
}
```

Update `onnx_factory` to accept `ComputeDevice` and set the ORT accelerator before model loading:

```rust
pub fn onnx_factory(
    model_dir: PathBuf,
    engine_type: EngineType,
    quantization: Quantization,
    compute_device: crate::api::settings::ComputeDevice,
) -> crate::engine::manager::SharedEngineFactory {
    std::sync::Arc::new(move || {
        use crate::api::settings::ComputeDevice;

        // Set ORT accelerator based on compute device preference
        let prev = transcribe_rs::get_ort_accelerator();
        match compute_device {
            ComputeDevice::Cpu => transcribe_rs::set_ort_accelerator(transcribe_rs::OrtAccelerator::CpuOnly),
            ComputeDevice::Gpu => {
                // Keep current accelerator (CUDA if available)
            }
            ComputeDevice::Auto => {
                if matches!(quantization, Quantization::Int8) {
                    transcribe_rs::set_ort_accelerator(transcribe_rs::OrtAccelerator::CpuOnly);
                }
                // FP32: keep current (CUDA if available)
            }
        }

        let engine: Box<dyn SpeechModel> = match engine_type {
            // ... existing match arms unchanged ...
        };

        // Determine actual device used
        let actual_device = if transcribe_rs::get_ort_accelerator() == transcribe_rs::OrtAccelerator::CpuOnly {
            "cpu".to_string()
        } else {
            "cuda".to_string()
        };

        // Restore previous accelerator
        transcribe_rs::set_ort_accelerator(prev);

        Ok(Box::new(OnnxBridge { engine, device: actual_device }) as Box<dyn SpeechEngine>)
    })
}
```

- [ ] **Step 3: Update WhisperBridge to implement device()**

In `src/engine/whisper_bridge.rs`, implement `device()` for WhisperBridge (always returns "cpu" unless whisper-cuda is enabled):

```rust
fn device(&self) -> &str {
    "cpu"
}
```

- [ ] **Step 4: Update register.rs to pass device settings**

In `src/engine/register.rs`, update `register_downloaded_models` to accept settings and look up device overrides:

```rust
use crate::api::settings::{ComputeDevice, Settings};

pub async fn register_downloaded_models(
    engine_manager: &EngineManager,
    model_dir: &Path,
    device_overrides: &std::collections::HashMap<String, ComputeDevice>,
) -> u32 {
    // ... existing loop ...
    for def in builtin_models() {
        // ... existing checks ...
        let device = device_overrides
            .get(&def.id)
            .cloned()
            .unwrap_or_default(); // Auto

        let factory = create_factory(&def.engine_type, model_path.clone(), device);
        // ... rest unchanged ...
    }
}
```

Update `create_factory` to accept and pass `ComputeDevice`:

```rust
fn create_factory(
    engine_type: &EngineType,
    model_path: std::path::PathBuf,
    compute_device: ComputeDevice,
) -> Option<crate::engine::manager::SharedEngineFactory> {
    // ... infer quantization as before ...

    match engine_type {
        #[cfg(feature = "whisper")]
        EngineType::Whisper => Some(crate::engine::whisper_bridge::whisper_factory(model_path)),

        #[cfg(feature = "onnx")]
        EngineType::SenseVoice | ... => Some(crate::engine::onnx_bridge::onnx_factory(
            model_path, engine_type.clone(), quantization, compute_device,
        )),

        #[cfg(feature = "qwen3")]
        EngineType::Qwen3 => Some(crate::engine::onnx_bridge::onnx_factory(
            model_path, engine_type.clone(), quantization, compute_device,
        )),
        // ...
    }
}
```

- [ ] **Step 5: Update main.rs to pass device_overrides**

In `src/main.rs`, pass `device_overrides` from DB settings to the register function:

```rust
let device_overrides = db_settings
    .as_ref()
    .map(|s| s.device_overrides.clone())
    .unwrap_or_default();

cortex_stt_server::engine::register::register_downloaded_models(
    &engine_manager,
    &model_dir_path,
    &device_overrides,
)
.await;
```

- [ ] **Step 6: Verify compilation and run tests**

Run: `cargo check && cargo test`
Expected: compiles, all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/engine/traits.rs src/engine/onnx_bridge.rs src/engine/whisper_bridge.rs src/engine/register.rs src/main.rs
git commit -m "feat: per-model compute device selection in engine pipeline"
```

---

### Task 6: Device Selection — Transcription API & History Recording

**Files:**
- Modify: `src/api/transcribe.rs:24-37,85-120,129-181`

- [ ] **Step 1: Record device in transcription flow**

In `src/api/transcribe.rs`, after acquiring the engine guard and running inference, capture the device from the engine:

In `run_transcription` (around line 85-120), after getting the transcription result, also capture `guard.device().to_string()` and return it alongside the result.

Update the return type or the result struct to include `device: String`.

- [ ] **Step 2: Pass device to save_to_history**

In `save_to_history` (around line 129-181), add `device: String` parameter and include it in the `CreateRecord`:

```rust
let record = CreateRecord {
    // ... existing fields ...
    device,
};
```

- [ ] **Step 3: Update TranscribeResponse to include device**

Add `device` field to `TranscribeResponse` struct (around line 48-55):

```rust
pub struct TranscribeResponse {
    pub text: String,
    pub segments: Vec<TranscriptionSegment>,
    pub model: String,
    pub duration_ms: u64,
    pub inference_ms: u64,
    pub device: String,
}
```

- [ ] **Step 4: Update all callers (sync, SSE, async handlers)**

Update `transcribe_sync`, `transcribe_sse`, and `transcribe_async` to pass `device` through the flow.

- [ ] **Step 5: Verify compilation and run tests**

Run: `cargo check && cargo test`

- [ ] **Step 6: Commit**

```bash
git add src/api/transcribe.rs
git commit -m "feat: record compute device in transcription history"
```

---

### Task 7: Device Selection — Frontend

**Files:**
- Modify: `web/src/api/types.ts`
- Modify: `web/src/components/history/history-list.tsx`
- Modify: `web/src/components/engine/model-lifecycle.tsx`

- [ ] **Step 1: Update TypeScript types**

In `web/src/api/types.ts`:

```typescript
export type ComputeDevice = "auto" | "cpu" | "gpu";

export interface AppSettings {
  // ... existing fields ...
  timezone: string;
  device_overrides: Record<string, ComputeDevice>;
}

export interface TranscriptionRecord {
  // ... existing fields ...
  device: string;  // "cpu" or "cuda"
}
```

- [ ] **Step 2: Show device badge in history**

In `web/src/components/history/history-list.tsx`, add a device badge in the ExpandedDetail metadata grid (after inference time):

```tsx
<div>
  <span className="text-xs text-text-muted">Device</span>
  <div className="text-sm text-text-primary">
    <Badge variant={record.device === "cuda" ? "info" : "default"}>
      {record.device.toUpperCase()}
    </Badge>
  </div>
</div>
```

Also show a small badge in the summary row next to inference time.

- [ ] **Step 3: Add device selector in Model Lifecycle**

In `web/src/components/engine/model-lifecycle.tsx`, add a per-model device selector. For each loaded model in the list, show a dropdown:

```tsx
<Select
  options={[
    { value: "auto", label: "Auto" },
    { value: "cpu", label: "CPU" },
    { value: "gpu", label: "GPU" },
  ]}
  value={settings?.device_overrides?.[modelId] ?? "auto"}
  onChange={(e) => {
    const overrides = { ...settings?.device_overrides, [modelId]: e.target.value };
    updateSettingsMutation.mutate({ device_overrides: overrides });
  }}
  className="w-24"
/>
```

Note: changing device requires model reload to take effect. Show a note about this.

- [ ] **Step 4: Build and verify**

Run: `cd web && bun run build`

Verify:
- History records show CPU/CUDA badge
- Model Lifecycle shows device selector per model
- Changing device setting persists

- [ ] **Step 5: Commit**

```bash
git add web/src/api/types.ts web/src/components/history/history-list.tsx web/src/components/engine/model-lifecycle.tsx
git commit -m "feat(web): device badge in history, per-model device selector"
```

---

### Task 8: Build, Deploy, and Verify

**Files:** None (deployment task)

- [ ] **Step 1: Build release binary**

```bash
cd /home/brandon/workspaces/github/hass-cortex/cortex-stt-server
cargo build --release --features "all-engines,ort-cuda"
```

- [ ] **Step 2: Build web UI**

```bash
cd web && bun run build
```

- [ ] **Step 3: Deploy to ollama server**

```bash
# Stop service
ssh ollama "sudo systemctl stop cortex-stt"

# Upload binary
scp target/release/cortex-stt-server ollama:/opt/cortex-stt/cortex-stt-server

# Upload web UI
rsync -ah web/dist/ ollama:/opt/cortex-stt/web/dist/

# Start service
ssh ollama "chmod +x /opt/cortex-stt/cortex-stt-server && sudo systemctl start cortex-stt"
```

- [ ] **Step 4: Verify all features**

```bash
# Check service health
ssh ollama "curl -s http://localhost:10400/health | python3 -m json.tool"

# Check logs for device info
ssh ollama "journalctl -u cortex-stt.service --since '30 sec ago' --no-pager" | grep -iE "Engine config|CUDA|device"
```

Open Web UI and verify:
1. History: multiple records can expand simultaneously, "Collapse All" appears
2. History: timestamps show in local timezone
3. Settings: timezone selector works
4. Engine: per-model device selector (Auto/CPU/GPU) appears
5. History: new transcriptions show CPU/CUDA badge

- [ ] **Step 5: Commit any deployment fixes**

If any fixes were needed, commit them.
