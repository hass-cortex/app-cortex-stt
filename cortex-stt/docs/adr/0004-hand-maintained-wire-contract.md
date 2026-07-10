# The web/backend wire contract stays hand-maintained; codegen deferred until a trigger fires

The admin UI's TypeScript types (`web/src/api/types.ts`, ~230 lines) are a
hand-maintained mirror of the Rust `Serialize` DTOs spread across ten
files (`src/api/*.rs`, `src/model/types.rs`, `src/settings.rs`,
`src/history/store.rs`). There is no compile-time link: a Rust field
rename is a silent runtime break on the frontend.

We evaluated generating the TypeScript side from Rust (ts-rs; specta as
the alternative) and decided **not to adopt it now**.

## Why deferred

- **Annotation noise outweighs today's drift risk.** The DTOs carry ~23
  `u64`/`i64` fields, which ts-rs maps to `bigint` by default — each
  needs a per-field override to match what `serde_json` actually emits
  (`number`). Plus a new dev-dependency (cargo-deny audit), a build-time
  export step, and a CI freshness check.
- **One consumer, one team.** The admin UI is the only TS consumer and
  changes land in the same PR as the Rust side; drift has not produced a
  real bug so far. Codegen pays off when the contract has independent
  consumers or independent release cadence — neither holds today.
- **The worst offender was structural, not tooling.** The
  `segments_json` triple contract (Rust struct → string-encoded JSON in
  the wire response → hand `JSON.parse` in the component) was removed
  outright: the string encoding is now a private storage detail of
  `history::store` (`RecordSegment`), and the wire carries structured
  `segments`. No codegen needed for that fix.

## Re-open when any trigger fires

1. A second independent consumer of the HTTP API's typed shapes appears
   (beyond the HA integration's narrow `/api/models` + `/api/transcribe`
   use).
2. A real production bug traced to Rust↔TS shape drift.
3. The DTO surface grows past roughly double its current size (new
   resource families, not new fields).

When re-opened, prefer deriving on the Rust side as the single owning
artifact, generated output committed to the repo, and a CI check that
regenerating produces no diff.

## Consequences

- `web/src/api/types.ts` remains the place to update when a DTO
  changes; keep its section comments aligned with the owning Rust files.
- New endpoints must update both sides in the same PR — reviewers should
  treat a Rust DTO diff without a `types.ts` diff as a red flag.
