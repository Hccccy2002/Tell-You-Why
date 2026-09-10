from __future__ import annotations

import os
import shutil
import time
import uuid
from collections import Counter
from pathlib import Path

import numpy as np

from ..indexing.embedding import validate_matrix
from ..util import atomic_json, file_hash, read_json, safe_name
from .database import write_database, validate_database


def contained(root: Path, relative: str) -> Path:
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("Artifact path escapes its storage directory")
    return path


def promote_directory(staging: Path, destination: Path) -> None:
    """Windows scanners can briefly hold newly written files without share-delete.

    Keep the active pointer untouched until the immutable directory is promoted.
    Do not retry permission errors on other platforms or overwrite a destination.
    """
    for attempt in range(7):
        try:
            os.replace(staging, destination)
            return
        except PermissionError as exc:
            if getattr(exc, "winerror", None) not in {5, 32, 33} or attempt == 6:
                raise
            time.sleep(min(0.1 * 2**attempt, 2))


def validate_version(directory: Path) -> dict:
    manifest = read_json(directory / "manifest.json")
    if manifest.get("schema_version") != 1:
        raise ValueError("Unsupported knowledge-base schema")
    required = {"knowledge.sqlite", "embeddings.npy", "terms.txt"}
    if not required.issubset(manifest["files"]):
        raise ValueError("Incomplete knowledge-base manifest")
    for relative, expected in manifest["files"].items():
        path = contained(directory, relative)
        if not path.is_file() or file_hash(path) != expected:
            raise ValueError(f"Artifact changed or missing: {relative}")
    source = contained(directory.parent.parent, manifest["source"]["path"])
    if not source.is_file() or file_hash(source) != manifest["source"]["sha256"]:
        raise ValueError("Source PDF changed or missing")
    counts = validate_database(directory / "knowledge.sqlite", manifest["source"]["pages"])
    matrix = np.load(directory / "embeddings.npy", mmap_mode="r", allow_pickle=False)
    try:
        validate_matrix(matrix, counts["chunks"])
    finally:
        matrix._mmap.close()
    for key, actual in counts.items():
        if manifest["counts"][key] != actual:
            raise ValueError(f"Manifest count mismatch: {key}")
    return {"valid": True, "version": manifest["version"], "status": manifest["status"], "counts": counts}


def active_version(kb_root: Path, version: str | None = None) -> Path:
    pointer = read_json(kb_root / "active.json") if version is None else None
    identifier = safe_name(version or pointer["version"])
    directory = contained(kb_root, "versions/" + identifier)
    if pointer and file_hash(directory / "manifest.json") != pointer["manifest_sha256"]:
        raise ValueError("Active manifest no longer matches its published pointer")
    return directory


def publish_version(
    kb_root: Path,
    version: str,
    metadata: dict,
    pages: list[dict],
    chapters: list[dict],
    blocks: list[dict],
    chunks: list[dict],
    matrix: np.ndarray,
    assets_root: Path,
    terms: list[str],
) -> dict:
    if not chunks:
        raise ValueError("No eligible text: a searchable knowledge base cannot be published")
    validate_matrix(matrix, len(chunks))
    version = safe_name(version)
    versions = kb_root / "versions"
    versions.mkdir(parents=True, exist_ok=True)
    destination = versions / version
    if destination.exists():
        result = validate_version(destination)
        atomic_json(
            kb_root / "active.json",
            {"version": version, "manifest_sha256": file_hash(destination / "manifest.json")},
        )
        return {**result, "reused": True}
    staging = versions / f".{version}.{uuid.uuid4().hex}.building"
    staging.mkdir()
    document = {
        "source_sha256": metadata["source_sha256"],
        "kb": metadata["kb"],
        "extraction_version": metadata["id"],
        "index_version": version,
        "source_path": "sources/" + metadata["source_sha256"] + ".pdf",
        "source_name": metadata.get("source_name", metadata["source_sha256"] + ".pdf"),
    }
    write_database(staging / "knowledge.sqlite", document, pages, chapters, blocks, chunks, terms)
    with (staging / "embeddings.npy").open("wb") as handle:
        np.save(handle, matrix, allow_pickle=False)
        handle.flush()
        os.fsync(handle.fileno())
    (staging / "terms.txt").write_text("\n".join(terms) + "\n", encoding="utf-8")
    assets = sorted({b["asset"] for b in blocks if b.get("asset")})
    for relative in assets:
        origin = contained(assets_root, relative)
        target = contained(staging, "assets/" + relative)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(origin, target)
    counts = validate_database(staging / "knowledge.sqlite", metadata["probe"]["pages"])
    counts["assets"] = len(assets)
    page_counts = dict(Counter(p["status"] for p in pages))
    ready = not any(page_counts.get(k, 0) for k in ("pending", "running", "failed", "needs_review"))
    body = [
        b
        for b in blocks
        if b["kind"] == "text"
        and b["content_kind"] in {"body", "appendix"}
        and b["page"] >= metadata["config"]["exclude_before_page"]
    ]
    available = sum(len(b["text"]) for b in body if b["eligible"])
    recognized = sum(len(b["text"]) for b in body)
    manifest = {
        "schema_version": 1,
        "kb": metadata["kb"],
        "version": version,
        "status": "ready" if ready else "partial_ready",
        "job_id": metadata["id"],
        "source": {
            "path": document["source_path"],
            "sha256": metadata["source_sha256"],
            "pages": metadata["probe"]["pages"],
            "filename": document["source_name"],
        },
        "models": metadata["models"],
        "runtime": metadata["runtime"],
        "config": metadata["config"],
        "counts": counts,
        "page_status_counts": page_counts,
        "coverage": {
            "eligible_recognized_body_characters": available,
            "recognized_body_characters": recognized,
            "eligible_fraction_of_recognized_body": available / recognized if recognized else 0,
            "scope": "Recognized body/appendix text, not total PDF visual content",
        },
        "limitations": [
            "OCR prose is machine checked, not fully manually proofread",
            "Uninterpreted figures, tables, formulas and ambiguous symbols are excluded from default evidence",
            "Retrieval ranking does not establish whether the PDF can answer a question",
        ],
        "files": {
            str(p.relative_to(staging)).replace("\\", "/"): file_hash(p)
            for p in sorted(staging.rglob("*"))
            if p.is_file()
        },
    }
    atomic_json(staging / "manifest.json", manifest)
    # Validate the finished immutable set before exposing it through active.json.
    validate_version(staging)
    promote_directory(staging, destination)
    atomic_json(
        kb_root / "active.json",
        {"version": version, "manifest_sha256": file_hash(destination / "manifest.json")},
    )
    return {
        "valid": True,
        "version": version,
        "status": manifest["status"],
        "counts": counts,
        "reused": False,
    }
