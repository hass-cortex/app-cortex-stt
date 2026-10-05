# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
"""Sync the vendored model catalog from Handy's catalog.json.

Converts Handy's catalog schema (handy-computer GGUF releases) into our
own schema. Each model is pinned to a Hugging Face commit: the download
URL, sha256 and size all come from that one commit, so a release keeps
installing the bytes it pinned after upstream re-uploads a file. A model
moves to a newer commit only when one of its files changed.
Run manually; review the diff before committing.

Usage:
    uv run scripts/sync-catalog.py --source <path-or-url-to-catalog.json>
    uv run scripts/sync-catalog.py            # default: fetch from GitHub main

Models whose file metadata cannot be resolved without authentication
(e.g. gated models) are skipped with a warning — the vendored catalog
only offers models that download cleanly.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import UTC, datetime
from pathlib import Path

import requests

HANDY_CATALOG_URL = (
    "https://raw.githubusercontent.com/cjpais/Handy/main/src-tauri/src/catalog/catalog.json"
)
HF_BASE = "https://huggingface.co"

REPO_ROOT = Path(__file__).resolve().parent.parent
CATALOG_OUT = REPO_ROOT / "src" / "model" / "catalog.json"


def load_source(source: str) -> dict:
    if source.startswith(("http://", "https://")):
        resp = requests.get(source, timeout=30)
        resp.raise_for_status()
        return resp.json()
    return json.loads(Path(source).read_text())


def resolve_main(repo: str) -> str | None:
    """Commit sha of the repo's main branch; None when not anonymously accessible."""
    resp = requests.get(f"{HF_BASE}/api/models/{repo}/revision/main", timeout=30)
    if resp.status_code in (401, 403, 404):
        return None
    resp.raise_for_status()
    return resp.json()["sha"]


def fetch_lfs_files(repo: str, revision: str, filenames: list[str]) -> dict[str, dict]:
    """sha256 and size of each LFS file at `revision`, via the HF paths-info API."""
    url = f"{HF_BASE}/api/models/{repo}/paths-info/{revision}"
    resp = requests.post(url, json={"paths": filenames}, timeout=60)
    resp.raise_for_status()
    out: dict[str, dict] = {}
    for entry in resp.json():
        lfs = entry.get("lfs") or {}
        if lfs.get("oid"):
            out[entry["path"]] = {"sha256": lfs["oid"], "size_bytes": lfs["size"]}
    return out


def pinned_revision(quants: list[dict]) -> str | None:
    """The commit a previous snapshot's URLs point at, if they share one."""
    revs = {q["url"].split("/resolve/", 1)[1].split("/", 1)[0] for q in quants}
    return revs.pop() if len(revs) == 1 else None


def pin(repo: str, filenames: list[str], previous: dict | None) -> tuple[str, dict] | str:
    """Pick the commit to pin and its files, or return why the model is skipped.

    Keeps the previous commit while main still serves identical files, so a
    model-card edit upstream does not churn the snapshot.
    """
    head = resolve_main(repo)
    if head is None:
        return f"repo {repo} not anonymously accessible"
    files = fetch_lfs_files(repo, head, filenames)
    missing = [f for f in filenames if f not in files]
    if missing:
        return f"no LFS sha256 for {', '.join(missing)}"
    if previous:
        kept = pinned_revision(previous["quants"])
        same = {q["filename"]: q["sha256"] for q in previous["quants"]} == {
            f: files[f]["sha256"] for f in filenames
        }
        if kept and kept != "main" and same:
            return kept, {q["filename"]: q for q in previous["quants"]}
    return head, files


def pick_default_quant(model: dict) -> str:
    if model.get("default_quant"):
        return model["default_quant"]
    quants = [f["quant"] for f in model["files"]]
    return "Q8_0" if "Q8_0" in quants else quants[0]


def convert(handy: dict, previous: dict) -> tuple[dict, list[str]]:
    prev_models = {m["id"]: m for m in previous.get("models", [])}
    models = []
    skipped: list[str] = []
    for m in handy["models"]:
        repo = m["id"]  # e.g. handy-computer/whisper-small-gguf
        slug = m.get("slug") or repo.split("/")[-1].removesuffix("-gguf")
        filenames = [f["filename"] for f in m["files"]]
        pinned = pin(repo, filenames, prev_models.get(slug))
        if isinstance(pinned, str):
            skipped.append(f"{slug} ({pinned})")
            continue
        revision, files = pinned
        caps = m["capabilities"]
        models.append(
            {
                "id": slug,
                "name": m["name"],
                "description": m["description"],
                "family": m.get("family") or m.get("architecture") or "unknown",
                "parameters": m.get("parameters"),
                "base_model": m.get("base_model"),
                "license": m.get("license"),
                "languages": m["languages"],
                "capabilities": {
                    "streaming": bool(caps.get("streaming")),
                    "translate": bool(caps.get("translate")),
                    "lang_detect": bool(caps.get("lang_detect")),
                    "timestamps": caps.get("timestamps") or "none",
                },
                "quants": [
                    {
                        "quant": f["quant"],
                        "filename": f["filename"],
                        "url": f"{HF_BASE}/{repo}/resolve/{revision}/{f['filename']}",
                        "sha256": files[f["filename"]]["sha256"],
                        "size_bytes": files[f["filename"]]["size_bytes"],
                    }
                    for f in m["files"]
                ],
                "default_quant": pick_default_quant(m),
                "recommended": bool(m.get("recommended")),
                "recommended_rank": m.get("recommended_rank"),
                "speed_score": m.get("speed_score"),
                "accuracy_score": m.get("accuracy_score"),
            }
        )
        print(f"  ok  {slug} ({len(filenames)} quants)", file=sys.stderr)

    converted = {
        "catalog_version": handy.get("catalog_version"),
        "generated_at": None,  # filled in by main(), see stamp_for
        "source": "cjpais/Handy catalog.json",
        "models": models,
    }
    return converted, skipped


def load_previous(out: Path) -> dict:
    try:
        return json.loads(out.read_text())
    except (OSError, json.JSONDecodeError):
        return {}


def stamp_for(previous: dict, catalog: dict) -> str:
    """Keep the previous timestamp when only it would change."""
    if previous:
        kept = previous.get("generated_at")
        if kept and {**previous, "generated_at": None} == catalog:
            return kept
    return datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default=HANDY_CATALOG_URL,
                        help="Path or URL of Handy's catalog.json")
    parser.add_argument("--out", default=str(CATALOG_OUT))
    args = parser.parse_args()

    handy = load_source(args.source)
    print(f"Converting {len(handy['models'])} models…", file=sys.stderr)
    out = Path(args.out)
    previous = load_previous(out)
    catalog, skipped = convert(handy, previous)

    # The release gate re-syncs and asks git whether anything moved, so a
    # stamp that advances on every run would fail every tag. It marks when
    # the pins last changed, not when the script last ran.
    catalog["generated_at"] = stamp_for(previous, catalog)

    out.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n")

    print(f"\nWrote {args.out} ({len(catalog['models'])} models)", file=sys.stderr)
    if skipped:
        print("\nSkipped:", file=sys.stderr)
        for s in skipped:
            print(f"  - {s}", file=sys.stderr)


if __name__ == "__main__":
    main()
