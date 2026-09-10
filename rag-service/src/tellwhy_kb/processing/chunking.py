from __future__ import annotations

import bisect
import re

from ..schemas import IngestConfig
from ..util import digest

CHUNK_VERSION = "1"


def local_tokenizer(models_root):
    from transformers import AutoTokenizer

    return AutoTokenizer.from_pretrained(str(models_root / "embedding"), local_files_only=True, use_fast=True)


def _tokens(tokenizer, text):
    return tokenizer(
        text, add_special_tokens=False, return_offsets_mapping=True, truncation=False, verbose=False
    )


def _runs(blocks):
    run = []
    for block in blocks:
        if block["kind"] in {"header", "footer", "number", "header_image", "footer_image"}:
            continue
        if (
            not block["eligible"]
            or block["kind"] in {"paragraph_title", "doc_title"}
            or not block["text"].strip()
        ):
            if run:
                yield run
                run = []
            continue
        if run and (
            any(block[k] != run[-1][k] for k in ("chapter_id", "section_id", "content_kind"))
            or block["page"] > run[-1]["page"] + 1
        ):
            yield run
            run = []
        run.append(block)
    if run:
        yield run


def make_chunks(
    blocks: list[dict], tokenizer, config: IngestConfig, kb: str, source_sha256: str, extraction_version: str
) -> list[dict]:
    chunks = []
    for run in _runs(blocks):
        prefix = " > ".join(run[0]["chapter_path"])
        title_tokens = _tokens(tokenizer, prefix)
        title_shortened = len(title_tokens["input_ids"]) > 160
        if title_shortened:
            prefix = prefix[: title_tokens["offset_mapping"][159][1]]
        overhead = len(tokenizer(prefix + "\n", truncation=False)["input_ids"])
        budget = min(config.chunk_tokens, 512 - overhead - 4)
        if budget <= config.overlap_tokens:
            raise ValueError("Chapter prefix leaves too little space for body and overlap")
        joined = ""
        segments = []
        previous = None
        for block in run:
            if joined:
                if previous["page"] != block["page"] and not re.search(r"[。！？；:：]$", joined):
                    separator = " " if joined[-1].isascii() and block["text"][0].isascii() else ""
                else:
                    separator = "\n\n"
                joined += separator
            start = len(joined)
            joined += block["text"]
            segments.append((start, len(joined), block))
            previous = block
        encoded = _tokens(tokenizer, joined)
        offsets = encoded["offset_mapping"]
        ends = [end for start, end in offsets]
        start_token = 0
        while start_token < len(offsets):
            end_token = min(start_token + budget, len(offsets))
            start = offsets[start_token][0]
            end = offsets[end_token - 1][1]
            if end_token < len(offsets):
                lower = offsets[min(end_token - 1, start_token + budget // 2)][0]
                boundaries = [
                    m.end()
                    for m in re.finditer(r"[。！？；]\s*|\n\n", joined[start:end])
                    if start + m.end() >= lower
                ]
                if boundaries:
                    candidate = bisect.bisect_right(ends, start + boundaries[-1])
                    if candidate > start_token:
                        end_token = candidate
                        end = offsets[end_token - 1][1]
            while True:
                text = joined[start:end]
                embedding_text = prefix + "\n" + text if prefix else text
                token_count = len(tokenizer(embedding_text, truncation=False, verbose=False)["input_ids"])
                if token_count <= 512:
                    break
                end_token -= 1
                if end_token <= start_token:
                    raise ValueError("Cannot fit even one body token")
                end = offsets[end_token - 1][1]
            locations = []
            for a, b, block in segments:
                left, right = max(a, start), min(b, end)
                if left >= right:
                    continue
                n0, n1 = left - a, right - a
                mapping = block["normalized_to_raw"]
                locations.append(
                    {
                        "block_id": block["id"],
                        "page": block["page"],
                        "printed_label": block.get("printed_label"),
                        "bbox": block["bbox"],
                        "normalized_start": n0,
                        "normalized_end": n1,
                        "raw_start": mapping[n0],
                        "raw_end": mapping[n1 - 1] + 1,
                        "chunk_start": left - start,
                        "chunk_end": right - start,
                    }
                )
            if not locations:
                raise ValueError("Chunk lacks source locations")
            identity = {
                "source": source_sha256,
                "extraction": extraction_version,
                "chunking": CHUNK_VERSION,
                "locations": locations,
                "text": text,
            }
            chunks.append(
                {
                    "id": digest(identity)[:32],
                    "kb": kb,
                    "source_sha256": source_sha256,
                    "extraction_version": extraction_version,
                    "chunk_version": CHUNK_VERSION,
                    "chapter_id": run[0]["chapter_id"],
                    "section_id": run[0]["section_id"],
                    "chapter_path": run[0]["chapter_path"],
                    "content_kind": run[0]["content_kind"],
                    "text": text,
                    "embedding_text": embedding_text,
                    "tokens": token_count,
                    "locations": locations,
                    "eligible": True,
                    "quality": "machine_checked; source verification recommended",
                    "limitations": ["embedding_title_shortened"] if title_shortened else [],
                }
            )
            if end_token == len(offsets):
                break
            start_token = max(start_token + 1, end_token - config.overlap_tokens)
    if len({c["id"] for c in chunks}) != len(chunks):
        raise ValueError("Duplicate chunk identities")
    return chunks


def validate_locations(chunks, blocks):
    by_id = {b["id"]: b for b in blocks}
    for chunk in chunks:
        if not chunk["locations"] or not 0 < chunk["tokens"] <= 512:
            raise ValueError("Invalid chunk size or empty provenance")
        for loc in chunk["locations"]:
            block = by_id[loc["block_id"]]
            if (
                not block["eligible"]
                or chunk["chapter_id"] != block["chapter_id"]
                or chunk["section_id"] != block["section_id"]
            ):
                raise ValueError("Chunk includes excluded content or crosses chapter boundaries")
            if loc["bbox"] != block["bbox"]:
                raise ValueError("Chunk geometry differs from source block")
            a, b = loc["normalized_start"], loc["normalized_end"]
            ca, cb = loc["chunk_start"], loc["chunk_end"]
            if not (0 <= a < b <= len(block["text"]) and 0 <= ca < cb <= len(chunk["text"])):
                raise ValueError("Invalid source range")
            if chunk["text"][ca:cb] != block["text"][a:b] or loc["page"] != block["page"]:
                raise ValueError("Chunk source text mismatch")
            if (
                loc["raw_start"] != block["normalized_to_raw"][a]
                or loc["raw_end"] != block["normalized_to_raw"][b - 1] + 1
            ):
                raise ValueError("Raw source range mismatch")
