# A Variant, not a model, is the unit of installation

A model can now have several **Quants** installed and in use at once: one
Home Assistant pipeline runs Whisper Small at Q4_K_M for speed while
another runs it at Q8_0 for accuracy, and an **Evaluation run** compares
the two side by side. So the unit that is installed, loaded, evaluated,
recorded in history and offered to Home Assistant is a **Variant** (one
**Catalog model** at one Quant), not the model. This replaces ADR 0003's
"exactly one quant of a model exists on disk".

## What follows from it

- **Install adds, never replaces.** Downloading a Quant installs its
  Variant and leaves the model's other Variants alone; Uninstall removes
  one Variant. A download is unique per Variant, so two Quants of one
  model can download at once under the same global slot limit.
- **A request names a model, and optionally a Quant.** `model=` keeps
  meaning the Catalog model; `quant=` picks the Variant. Without it the
  model's **Preferred variant** serves: the user's choice, else the first
  installed, and on its Uninstall the earliest-installed remaining one.
  The **Default model** stays a model and resolves the same way, so "which
  Quant when none is named" has exactly one rule. The rule is not "the
  catalog's `default_quant`": a catalog sync that moved 22 models from
  Q5_K_M to Q8_0 would then have switched what running pipelines use.
- **Each installed file describes itself.** Its JSON sidecar records the
  model, the Quant, the verified sha256, the install time and a snapshot
  of the catalog entry's display data. Identity therefore no longer comes
  from looking a filename up in the current catalog, which is what lets
  a file outlive its catalog entry.
- **Retired, not reassigned.** When the catalog stops offering a Quant or
  a whole model, the installed Variant becomes a **Retired variant**: same
  identity, still serving, no more updates, shown from its sidecar
  snapshot. A model renamed upstream is a new Catalog model; the old
  Variants stay Retired under the old name until the user removes them.
- **Each model gets its own directory.** Catalog files live at
  `{model_id}/{filename}`. Upstream reuses filenames across a rename
  (`moss-transcribe-diarize` became `MOSS-Transcribe-Diarize` with the
  same filenames and new bytes), and in a flat directory the new model's
  download would overwrite the Retired one. **Custom models** stay at the
  top level, where people already drop them.
- **Home Assistant sees one device per Variant.** Each carries the STT
  entity, the loaded sensor and the usage sensors, named with the Quant
  ("Whisper Small (Q8_0)") even when it is the only one, so installing a
  second Quant never renames the first. Identifiers become
  `{entry}_{model}:{quant}`; a Custom model keeps `{entry}_{model}`.

## Upgrading

On first start the app moves each flat catalog file into its model's
directory and writes its sidecar from the catalog entry that owns the
filename, using the file's mtime as the install time; each model's single
installed Variant becomes its Preferred variant. The integration rewrites
each existing device identifier and entity unique id to the Variant
installed at that moment, so entity ids, pipelines and stt-corrector
references survive and only display names change.

The app keeps a model-level `status` (Downloaded when any Variant is
installed), so an older integration keeps working against a newer app:
one entity per model, served by its Preferred variant. A newer
integration requires the app's `variants`; against an older app it fails
setup with a repair issue asking to update the app. The app is released
first.

## Considered options

- **Keep one Quant per model and compare by switching.** Rejected: two
  pipelines cannot use two Quants at once, and every switch deletes the
  file the other Quant needs.
- **Show Variants only to the admin UI and evaluation, one Home Assistant
  entity per model.** Rejected for the same reason: a pipeline picks an
  entity, so a Variant a pipeline cannot pick is not in use.
- **A `variants` table in SQLite instead of sidecars.** Rejected: a file
  and the record that describes it could then disagree, and losing the
  database would lose which model a file belongs to.
- **Treat a filename match as the same model across a rename.** This is
  what a flat layout does today: the Home Assistant entity is recreated
  and pipelines must be re-pointed. Rejected: an upstream rename should
  break nothing.
