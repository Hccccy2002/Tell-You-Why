from __future__ import annotations

import json
from pathlib import Path

from . import PIPELINE_VERSION
from .ingest.assessment import assess_page, block_limitations
from .ingest.extract import PdfSource
from .indexing.embedding import Encoder
from .indexing.keyword import DEFAULT_TERMS
from .jobs import JobStore, exclusive_lock
from .processing.chapters import build_chapters
from .processing.normalize import normalize_blocks
from .processing.chunking import local_tokenizer, make_chunks, validate_locations
from .schemas import IngestConfig
from .storage.manifest import publish_version, validate_version
from .util import atomic_json, digest, event, file_hash, read_json


def write_jsonl(path, rows):
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False, allow_nan=False) + "\n")
    temporary.replace(path)


def build_job(
    folder: Path,
    models_root: Path,
    until: str = "publish",
    overrides: dict | None = None,
    terms: list[str] | None = None,
    allow_partial: bool = False,
) -> dict:
    if until not in {"structure", "chunks", "publish"}:
        raise ValueError("Unknown build stage")
    terms = terms if terms is not None else DEFAULT_TERMS
    meta = read_json(folder / "job.json")
    config = IngestConfig.model_validate(meta["config"])
    if file_hash(Path(meta["source"])) != meta["source_sha256"]:
        raise ValueError("Imported source changed")
    with exclusive_lock(folder / "writer.lock"), JobStore(folder) as store:
        pages = []
        missing = []
        with PdfSource(Path(meta["source"])) as source:
            for number in range(1, meta["probe"]["pages"] + 1):
                result = store.cached(number)
                if result is None:
                    row = store.db.execute(
                        "SELECT status,error FROM pages WHERE page=?", (number,)
                    ).fetchone()
                    if row["status"] not in {"pending", "running", "failed"}:
                        raise ValueError(f"Page {number} checkpoint damaged; resume extraction first")
                    missing.append({"number": number, "status": row["status"], "error": row["error"]})
                    continue
                if any(set(block_limitations(b)) - set(b.limitations) for b in result.blocks):
                    with source.render(number, config.dpi) as image:
                        assess_page(result, image, folder / "crops")
                    store.save(result)
                pages.append(result)
        if until == "publish" and missing and not allow_partial:
            raise ValueError(
                "Unfinished or failed pages remain; resume first, or explicitly use --allow-partial"
            )
        if not pages:
            raise ValueError("No processed pages available")
        chapters = build_chapters(meta["probe"], pages, overrides)
        blocks = normalize_blocks(pages, chapters)
        atomic_json(folder / "chapters.json", chapters)
        write_jsonl(folder / "blocks.jsonl", blocks)
        report = {
            "job": meta["id"],
            "stage": "structure",
            "processed_pages": len(pages),
            "unprocessed_pages": len(missing),
            "chapters": sum(c["kind"] == "chapter" for c in chapters),
            "chapter_nodes": len(chapters),
            "blocks": len(blocks),
            "eligible_blocks": sum(b["eligible"] for b in blocks),
        }
        atomic_json(folder / "stage.json", report)
        event("structure_validated", details=report)
        if until == "structure":
            return report
        tokenizer = local_tokenizer(models_root)
        chunks = make_chunks(blocks, tokenizer, config, meta["kb"], meta["source_sha256"], meta["id"])
        validate_locations(chunks, blocks)
        write_jsonl(folder / "chunks.jsonl", chunks)
        report.update(
            {
                "stage": "chunks",
                "chunks": len(chunks),
                "maximum_input_tokens": max((c["tokens"] for c in chunks), default=0),
                "silent_truncations": 0,
                "source_locations_valid": True,
            }
        )
        atomic_json(folder / "stage.json", report)
        event("chunks_validated", details=report)
        if until == "chunks":
            return report
        kb_root = folder.parent.parent
        all_pages = sorted([p.model_dump() for p in pages] + missing, key=lambda p: p["number"])
        version = digest(
            {
                "job": meta["id"],
                "source_name": meta.get("source_name"),
                "pipeline": PIPELINE_VERSION,
                "pages": all_pages,
                "chapters": chapters,
                "chunks": chunks,
                "terms": terms,
            }
        )[:24]
        destination = kb_root / "versions" / version
        if destination.exists():
            result = validate_version(destination)
            atomic_json(
                kb_root / "active.json",
                {"version": version, "manifest_sha256": file_hash(destination / "manifest.json")},
            )
            report.update({**result, "stage": "published", "reused": True})
            atomic_json(folder / "stage.json", report)
            return report
        encoder = Encoder(models_root, config.cpu_threads)
        matrix = encoder.encode([c["embedding_text"] for c in chunks], config.embedding_batch_size)
        result = publish_version(
            kb_root, version, meta, all_pages, chapters, blocks, chunks, matrix, folder / "crops", terms
        )
        report.update({**result, "stage": "published"})
        atomic_json(folder / "stage.json", report)
        atomic_json(kb_root / "reports" / "build-validation.json", report)
        return report
