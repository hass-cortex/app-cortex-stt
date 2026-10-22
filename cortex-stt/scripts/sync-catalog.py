# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
"""Sync the vendored model catalog from Handy's catalog.json.

Converts Handy's catalog (schema 2, handy-computer GGUF releases) into our
own schema. Upstream pins each model to a Hugging Face commit; the download
URL uses that commit, and its sha256 and size are kept only when Hugging
Face serves the same file there, so a release keeps installing the bytes it
pinned after upstream re-uploads a file. Only transcription models the
pinned transcribe-cpp can load are offered.
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
import re
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
CARGO_TOML = REPO_ROOT / "Cargo.toml"


HF = requests.Session()
COMMIT_SHA = re.compile(r"[0-9a-f]{40}")


def load_source(source: str) -> dict:
    if source.startswith(("http://", "https://")):
        resp = HF.get(source, timeout=30)
        resp.raise_for_status()
        return resp.json()
    return json.loads(Path(source).read_text())


def repo_accessible(repo: str) -> bool:
    """Whether the repo serves files without authentication (gated repos don't)."""
    resp = HF.get(f"{HF_BASE}/api/models/{repo}", timeout=30)
    if resp.status_code in (401, 403, 404):
        return False
    resp.raise_for_status()
    return not resp.json().get("gated")


def fetch_lfs_files(repo: str, revision: str, filenames: list[str]) -> dict[str, dict]:
    """sha256 and size of each LFS file at `revision`; empty when HF has no such revision."""
    url = f"{HF_BASE}/api/models/{repo}/paths-info/{revision}"
    resp = HF.post(url, json={"paths": filenames}, timeout=60)
    if resp.status_code == 404:
        return {}
    resp.raise_for_status()
    out: dict[str, dict] = {}
    for entry in resp.json():
        lfs = entry.get("lfs") or {}
        if lfs.get("oid"):
            out[entry["path"]] = {"sha256": lfs["oid"], "size_bytes": lfs["size"]}
    return out


def main_history(repo: str) -> list[str]:
    """Commit shas on the repo's main branch, newest first."""
    resp = HF.get(f"{HF_BASE}/api/models/{repo}/commits/main", timeout=30)
    resp.raise_for_status()
    return [c["id"] for c in resp.json()]


def parse_version(v: str) -> tuple[int, ...] | None:
    """`x.y.z` as a tuple; None for anything else (pre-releases included)."""
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", v.strip())
    return tuple(int(p) for p in m.groups()) if m else None


def engine_version() -> tuple[int, ...]:
    """The transcribe-cpp version Cargo.toml pins."""
    text = CARGO_TOML.read_text()
    m = re.search(r'^transcribe-cpp\s*=.*?"=?([^"]+)"', text, re.MULTILINE)
    version = parse_version(m.group(1)) if m else None
    if version is None:
        raise SystemExit("transcribe-cpp version not found in Cargo.toml")
    return version


def base_languages(tags: list[str]) -> list[str]:
    """BCP-47 tags reduced to their language subtag, deduplicated in order.

    Home Assistant matches an STT language exactly, and the engine resolves
    a base code itself, so `en-US` and `en-GB` are both offered as `en`.
    """
    return list(dict.fromkeys(t.split("-", 1)[0].lower() for t in tags))


def check(m: dict, engine: tuple[int, ...]) -> str | None:
    """Why the model cannot be offered, or None when it can."""
    if m["role"] != "asr":
        return f"role {m['role']} is not transcription"
    needs = parse_version(m["minEngine"])
    if needs is None or needs > engine:
        return f"needs transcribe-cpp {m['minEngine']}"
    repo, revision = m["repository"], m["revision"]
    # A branch or tag would let the bytes change under a shipped sha256.
    if not COMMIT_SHA.fullmatch(revision):
        return f"revision {revision!r} is not a commit"
    if not repo_accessible(repo):
        return f"repo {repo} not anonymously accessible"
    # Trust upstream's pin only as far as Hugging Face confirms it.
    files = fetch_lfs_files(
        repo, revision, [f["filename"] for f in m["files"].values()]
    )
    for f in m["files"].values():
        served = files.get(f["filename"])
        if served != {"sha256": f["sha256"], "size_bytes": f["size"]}:
            return f"{f['filename']} at {revision[:8]} does not match the catalog"
    return None


def pinned_revision(quants: list[dict]) -> str | None:
    """The commit a snapshot entry's URLs point at, if they share one."""
    revs = {q["url"].split("/resolve/", 1)[1].split("/", 1)[0] for q in quants}
    return revs.pop() if len(revs) == 1 else None


def is_older(repo: str, candidate: str, current: str) -> bool:
    """Whether `candidate` precedes `current` on main (both must be on it)."""
    history = main_history(repo)
    if candidate not in history or current not in history:
        return False
    return history.index(candidate) > history.index(current)


def convert(handy: dict, previous: dict) -> tuple[dict, list[str], list[str]]:
    if handy.get("schema") != 2:
        raise SystemExit(f"unsupported Handy catalog schema {handy.get('schema')}")
    engine = engine_version()
    prev_models = {m["id"]: m for m in previous.get("models", [])}
    models = []
    skipped: list[str] = []
    notes: list[str] = []
    for m in handy["models"]:
        slug = m["id"]
        reason = check(m, engine)
        if reason:
            skipped.append(f"{slug} ({reason})")
            continue
        repo, revision = m["repository"], m["revision"]
        caps = m["capabilities"]
        quants = [
            {
                "quant": quant,
                "filename": f["filename"],
                "url": f"{HF_BASE}/{repo}/resolve/{revision}/{f['filename']}",
                "sha256": f["sha256"],
                "size_bytes": f["size"],
            }
            for quant, f in m["files"].items()
        ]
        default_quant = m["quant"]
        prev = prev_models.get(slug)
        kept = pinned_revision(prev["quants"]) if prev else None
        same_bytes = prev is not None and {
            q["filename"]: q["sha256"] for q in prev["quants"]
        } == {q["filename"]: q["sha256"] for q in quants}
        if same_bytes:
            # Same files under another commit: keep ours, nothing to install.
            quants = prev["quants"]
        # Moving a pin back would offer installed users an "update" to older bytes.
        elif prev and kept and kept != revision and is_older(repo, revision, kept):
            notes.append(
                f"{slug}: upstream pins {revision[:8]}, older than our "
                f"{kept[:8]}; kept ours"
            )
            quants = prev["quants"]
            if default_quant not in {q["quant"] for q in quants}:
                default_quant = prev["default_quant"]
        if prev and prev["default_quant"] != default_quant:
            sizes = {q["quant"]: q["size_bytes"] for q in quants}
            before = sizes.get(prev["default_quant"])
            after = sizes[default_quant]
            if before and after > before:
                notes.append(
                    f"{slug}: default quant {prev['default_quant']} -> "
                    f"{default_quant} grows the default download "
                    f"{before / 1e9:.1f} -> {after / 1e9:.1f} GB"
                )
        models.append(
            {
                "id": slug,
                "name": m["name"],
                "description": m["description"],
                "family": m["family"],
                "parameters": m.get("parameters"),
                "license": m.get("license"),
                "languages": base_languages(m["languages"]),
                "capabilities": {
                    "streaming": bool(caps.get("streaming")),
                    "translate": bool(caps.get("translate")),
                    "lang_detect": bool(caps.get("languageDetection")),
                    "timestamps": bool(caps.get("timestamps")),
                },
                "quants": quants,
                "default_quant": default_quant,
                "recommended": bool(m.get("curated")),
            }
        )
        print(f"  ok  {slug} ({len(quants)} quants)", file=sys.stderr)

    removed = sorted(prev_models.keys() - {m["id"] for m in models})
    notes.extend(f"{slug}: no longer offered (removed or renamed upstream)" for slug in removed)

    converted = {
        "catalog_version": handy["schema"],
        "generated_at": None,  # filled in by main(), see stamp_for
        "source": "cjpais/Handy catalog.json",
        "models": models,
    }
    return converted, skipped, notes


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
    catalog, skipped, notes = convert(handy, previous)

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
    if notes:
        print("\nReview before committing:", file=sys.stderr)
        for n in notes:
            print(f"  - {n}", file=sys.stderr)


if __name__ == "__main__":
    main()
