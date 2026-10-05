# Model catalog is a vendored, converted snapshot of Handy's catalog.json

The set of downloadable models (the **Catalog**) is no longer a hand-curated
list in `registry.rs`. It is a full sync of Handy's `catalog.json`
(handy-computer GGUF releases), vendored into this repo as a converted snapshot
and refreshed by a maintainer-run `sync-catalog` script that pins each model
to a Hugging Face commit and takes every file's URL, sha256 and size from it.
(Model documentation lives upstream — [transcribe.cpp `docs/models/`](https://github.com/handy-computer/transcribe.cpp/tree/main/docs/models);
the generated `MODELS.md` was removed in favor of linking there.)

## Why full sync, vendored

- Full sync over curation: the upstream catalog already carries slugs, quant
  matrices, capabilities, language lists, and recommendation ranks; curating a
  subset re-creates maintenance work every upstream release for little gain.
- Vendored snapshot over runtime fetch: builds stay deterministic and offline,
  the addon gains no network dependency or third-party schema drift at runtime,
  and every catalog change is reviewable in a diff.
- Converted to our own schema (not Handy's verbatim) so upstream schema changes
  break the sync script, not the server.
- Pinned to a commit, not a branch: upstream re-uploads files in place (a
  metadata fix re-hashes every quant), and a `resolve/main` URL would then serve
  bytes the shipped sha256 rejects — every released version breaks at once. A
  commit URL keeps serving the pinned bytes, so a release installs the same
  files for as long as Hugging Face keeps the history. Handy does the same.
  The script keeps a model's commit while main still serves identical files,
  so a model-card edit does not churn the snapshot.

## Consequences

- Model identity is the upstream `slug`; exactly one quant of a model exists on
  disk (chosen at download, default `default_quant`), so ids never carry a
  quant dimension.
- Catalog freshness depends on a script run (manual, or the weekly drift PR) —
  acceptable, since new models require validation before being offered anyway.
  A stale snapshot only misses newer files; it never stops installing.
- We inherit upstream's model set wholesale, including families we may never
  load; the UI leans on `recommended` flags to keep the list navigable.
