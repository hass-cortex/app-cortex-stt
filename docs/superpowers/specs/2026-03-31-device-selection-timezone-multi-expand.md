# Per-Model Device Selection, History Timezone, Multi-Expand

## Overview

Three enhancements to cortex-stt-server:
1. Per-model CPU/GPU compute device configuration with API override
2. History page timezone-aware display
3. History page multi-expand support

---

## Feature 1: Per-Model Compute Device Selection

### Problem

All ONNX models share the same global execution provider (CPU or CUDA). INT8 models run slower on GPU due to missing CUDA kernels (`DynamicQuantizeLinear`, `MatMulInteger`), causing CPU↔GPU memory copies that are slower than pure CPU inference. Users cannot control which device a model uses.

### Design

#### Data Model

Add `device_overrides` map to `AppSettings` (persisted in DB):

```rust
// api/settings.rs
pub struct AppSettings {
    // ... existing fields ...
    /// Per-model compute device override. Key = model_id, Value = "auto" | "cpu" | "gpu"
    #[serde(default)]
    pub device_overrides: HashMap<String, ComputeDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ComputeDevice {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
```

`Auto` resolves at load time: INT8 → CPU, FP32 → GPU, fallback CPU if no GPU.

#### Execution Provider Selection

The `onnx_bridge::onnx_factory` closure receives a `ComputeDevice` parameter. Before creating the ORT session, it sets the global ORT accelerator accordingly:

```
ComputeDevice::Cpu  → set_ort_accelerator(CpuOnly)
ComputeDevice::Gpu  → set_ort_accelerator(Cuda) [or whatever is available]
ComputeDevice::Auto → INT8 quantization? CpuOnly : best available
```

Model loading is serialized (behind pool mutex), so temporarily setting the global accelerator is safe. After session creation, restore the previous value.

The `SpeechEngine` trait gains a `device()` method returning the actual device used (`"cpu"` or `"cuda"`), determined at session creation time by checking which execution providers were registered.

#### Register Flow

In `register.rs`, `create_factory` receives the resolved `ComputeDevice` from settings (looked up by model_id) and passes it to the factory:

```
register_downloaded_models()
  → for each model:
    → look up device_overrides[model.id] (default: Auto)
    → resolve Auto → CPU or GPU based on quantization
    → create_factory(engine_type, model_path, quantization, device)
```

#### API Override

The transcription API (`POST /api/transcribe`) accepts an optional `device` query parameter:

```
POST /api/transcribe?model=sense-voice-int8&device=cpu
```

If provided, it overrides the model's configured device for that request. This requires acquiring a model instance with the specified device — if the pooled instance was loaded with a different device, it must be reloaded (or a separate pool entry created).

Simplification: for v1, the API `device` parameter is accepted but only validated against the model's current loaded device. If mismatched, return an error suggesting the user change the model's device setting. Full per-request device switching can be added later.

#### History Record

Add `device` field to the transcription record:

```rust
// db/records.rs - TranscriptionRecord
pub device: String,  // "cpu" or "cuda"
```

DB migration: `ALTER TABLE records ADD COLUMN device TEXT NOT NULL DEFAULT 'cpu'`.

#### Web UI: Models Page

Each model card shows a device selector dropdown (`Auto / CPU / GPU`). Changes are saved via `PUT /api/settings` updating `device_overrides`. The current actual device is shown as a badge (e.g., "CPU" or "CUDA").

#### Web UI: Engine Page

The "Load Model" section shows the resolved device next to pool size.

#### Web UI: History Page

Each history record shows a small badge (`CPU` / `CUDA`) next to the inference time.

---

## Feature 2: History Timezone Display

### Problem

History timestamps are displayed in UTC. Users expect local time.

### Design

#### Approach: Client-Side Conversion

Timestamps remain stored as UTC in the database and returned as UTC in API responses. The frontend converts to local time at display time.

#### Settings

Add `timezone` to `AppSettings`:

```rust
pub struct AppSettings {
    // ... existing fields ...
    /// Timezone for display. "auto" = browser detection, or IANA timezone (e.g., "Asia/Taipei")
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "auto".into()
}
```

#### Frontend Implementation

1. On app load, detect browser timezone: `Intl.DateTimeFormat().resolvedOptions().timeZone`
2. If settings `timezone` is `"auto"`, use detected timezone
3. If settings `timezone` is a specific IANA zone (e.g., `"Asia/Taipei"`), use that
4. All timestamp formatting uses a shared `formatTimestamp(utcString, timezone)` utility
5. The utility uses `Date.toLocaleString('default', { timeZone })` for conversion

#### Settings Page

Add a "Timezone" setting in the Settings page:
- Dropdown with "Auto (detect from browser)" as default
- Common timezone options (Asia/Taipei, America/New_York, Europe/London, UTC, etc.)
- Shows the currently resolved timezone (e.g., "Auto → Asia/Taipei")

#### No Backend Changes Required

The API continues to return UTC timestamps. No DB migration needed. All conversion is frontend-only.

---

## Feature 3: History Multi-Expand

### Problem

Only one history record can be expanded at a time. Users want to compare multiple records side-by-side.

### Design

#### State Change

In the history list component, change state from single-expand to multi-expand:

```typescript
// Before
const [expandedId, setExpandedId] = useState<string | null>(null);

// After
const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
```

#### Toggle Logic

```typescript
const toggleExpand = (id: string) => {
  setExpandedIds(prev => {
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

#### Collapse All Button

When 2+ records are expanded, show a "Collapse All" button in the filter bar area:

```typescript
{expandedIds.size >= 2 && (
  <Button size="sm" variant="ghost" onClick={() => setExpandedIds(new Set())}>
    Collapse All
  </Button>
)}
```

#### No Persistence

Expanded state is component-local. Refreshing the page resets all expansions. No localStorage needed.

---

## Implementation Order

1. **Feature 3 (Multi-expand)** — frontend-only, no backend changes, quick win
2. **Feature 2 (Timezone)** — mostly frontend, small settings addition
3. **Feature 1 (Device selection)** — backend + frontend, most complex

## Files to Modify

### Feature 1
- `src/api/settings.rs` — add `ComputeDevice` enum, `device_overrides` field
- `src/engine/register.rs` — pass device to factory
- `src/engine/onnx_bridge.rs` — accept device, set accelerator before session creation
- `src/engine/traits.rs` — add `device()` to `SpeechEngine` trait
- `src/api/transcribe.rs` — accept `device` query param, record device in history
- `src/db/records.rs` — add `device` column
- `src/db/database.rs` — migration for `device` column
- `web/src/api/types.ts` — add `device_overrides`, `ComputeDevice`, history `device` field
- `web/src/components/engine/model-lifecycle.tsx` or new model settings component
- `web/src/pages/history.tsx` — show device badge

### Feature 2
- `src/api/settings.rs` — add `timezone` field
- `web/src/api/types.ts` — add `timezone` field
- `web/src/utils/time.ts` — new: `formatTimestamp()` utility
- `web/src/pages/history.tsx` — use `formatTimestamp()`
- `web/src/pages/settings.tsx` — add timezone selector

### Feature 3
- `web/src/pages/history.tsx` — change `expandedId` to `expandedIds: Set<string>`, add "Collapse All"
