# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
"""Sync the vendored model catalog from Handy's catalog.json.

Converts Handy's catalog schema (handy-computer GGUF releases) into our
own schema and resolves per-file sha256 from Hugging Face LFS metadata.
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


def fetch_sha256_map(repo: str, filenames: list[str]) -> dict[str, str] | None:
    """Resolve sha256 for each file via the HF paths-info API.

    Returns None when the repo is not accessible anonymously (gated/missing).
    """
    url = f"{HF_BASE}/api/models/{repo}/paths-info/main"
    resp = requests.post(url, json={"paths": filenames}, timeout=60)
    if resp.status_code in (401, 403, 404):
        return None
    resp.raise_for_status()
    out: dict[str, str] = {}
    for entry in resp.json():
        lfs = entry.get("lfs") or {}
        oid = lfs.get("oid")
        if oid:
            out[entry["path"]] = oid
    return out


def pick_default_quant(model: dict) -> str:
    if model.get("default_quant"):
        return model["default_quant"]
    quants = [f["quant"] for f in model["files"]]
    return "Q8_0" if "Q8_0" in quants else quants[0]


def convert(handy: dict) -> tuple[dict, list[str]]:
    models = []
    skipped: list[str] = []
    for m in handy["models"]:
        repo = m["id"]  # e.g. handy-computer/whisper-small-gguf
        slug = m.get("slug") or repo.split("/")[-1].removesuffix("-gguf")
        filenames = [f["filename"] for f in m["files"]]
        sha_map = fetch_sha256_map(repo, filenames)
        if sha_map is None:
            skipped.append(f"{slug} (repo {repo} not anonymously accessible)")
            continue
        missing = [f for f in filenames if f not in sha_map]
        if missing:
            skipped.append(f"{slug} (no LFS sha256 for {', '.join(missing)})")
            continue
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
                        "url": f"{HF_BASE}/{repo}/resolve/main/{f['filename']}",
                        "sha256": sha_map[f["filename"]],
                        "size_bytes": f["size_bytes"],
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
        "generated_at": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "cjpais/Handy catalog.json",
        "models": models,
    }
    return converted, skipped


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default=HANDY_CATALOG_URL,
                        help="Path or URL of Handy's catalog.json")
    parser.add_argument("--out", default=str(CATALOG_OUT))
    args = parser.parse_args()

    handy = load_source(args.source)
    print(f"Converting {len(handy['models'])} models…", file=sys.stderr)
    catalog, skipped = convert(handy)

    Path(args.out).write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n")

    print(f"\nWrote {args.out} ({len(catalog['models'])} models)", file=sys.stderr)
    if skipped:
        print("\nSkipped:", file=sys.stderr)
        for s in skipped:
            print(f"  - {s}", file=sys.stderr)


if __name__ == "__main__":
    main()
