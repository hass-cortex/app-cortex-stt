# Model catalog is a vendored, converted snapshot of Handy's catalog.json

The set of downloadable models (the **Catalog**) is no longer a hand-curated
list in `registry.rs`. It is a full sync of Handy's `catalog.json`
(handy-computer GGUF releases), vendored into this repo as a converted snapshot
and refreshed by a maintainer-run `sync-catalog` script that also fetches
per-file sha256 from Hugging Face LFS metadata and regenerates `MODELS.md`.

## Why full sync, vendored

- Full sync over curation: the upstream catalog already carries slugs, quant
  matrices, capabilities, language lists, and recommendation ranks; curating a
  subset re-creates maintenance work every upstream release for little gain.
- Vendored snapshot over runtime fetch: builds stay deterministic and offline,
  the addon gains no network dependency or third-party schema drift at runtime,
  and every catalog change is reviewable in a diff.
- Converted to our own schema (not Handy's verbatim) so upstream schema changes
  break the sync script, not the server.

## Consequences

- Model identity is the upstream `slug`; exactly one quant of a model exists on
  disk (chosen at download, default `default_quant`), so ids never carry a
  quant dimension.
- Catalog freshness depends on a manual script run — acceptable, since new
  models require validation before being offered anyway.
- We inherit upstream's model set wholesale, including families we may never
  load; the UI leans on `recommended` flags to keep the list navigable.
