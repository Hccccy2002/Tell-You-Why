"""Bounded, version-pinned source evidence; retrieval is not an answerability test."""

from __future__ import annotations

import hashlib
import json

from .processing.chunking import validate_locations


def assemble(index, manifest, kb, query, chapter, mode="hybrid", max_chars=10000, primary_unit=None):
    if not 100 <= max_chars <= 10000:
        raise ValueError("Invalid evidence budget")
    ranked = index.search(query, top_k=8, mode=mode, chapter=chapter)
    rows = index.db.execute("SELECT data FROM blocks ORDER BY page,rowid").fetchall()
    blocks = [json.loads(r[0]) for r in rows]
    by_id = {b["id"]: b for b in blocks}
    positions = {b["id"]: i for i, b in enumerate(blocks)}
    seeds, paths, chunks = [], {}, {}
    primary_ids = primary_unit["block_ids"] if primary_unit else []
    if len(primary_ids) > 12 or sum(len(by_id[bid]["text"]) for bid in primary_ids) > max_chars:
        raise ValueError("主素材超出完整证据预算")
    for bid in primary_ids:
        block = by_id[bid]
        if not block.get("eligible") or block.get("kind") in {"paragraph_title", "doc_title"}:
            raise ValueError("主素材已不可用于出题")
        seeds.append(bid)
        paths[bid] = primary_unit["chapter_path"]
    for result in ranked["results"]:
        chunk = json.loads(
            index.db.execute("SELECT data FROM chunks WHERE id=?", (result["chunk_id"],)).fetchone()[0]
        )
        # Recheck source mappings before offering text to a generation model.
        validate_locations([chunk], [by_id[loc["block_id"]] for loc in chunk["locations"]])
        for loc in chunk["locations"]:
            bid = loc["block_id"]
            if bid not in seeds:
                seeds.append(bid)
            paths[bid] = result["chapter_path"]
            chunks.setdefault(bid, []).append(result["chunk_id"])

    # Seed blocks first. Only adjacent blocks in the same section can supplement them.
    candidates = list(seeds)
    for bid in seeds:
        seed = by_id[bid]
        for p in (positions[bid] - 1, positions[bid] + 1):
            if 0 <= p < len(blocks):
                neighbor = blocks[p]
                if seed.get("section_id") and neighbor.get("section_id") == seed["section_id"]:
                    nid = neighbor["id"]
                    if nid not in candidates:
                        candidates.append(nid)
                        paths[nid] = paths[bid]

    evidence, used, omitted = [], 0, 0
    for bid in candidates:
        block = by_id[bid]
        text = block["text"]
        if (
            not block.get("eligible")
            or block.get("kind") in {"paragraph_title", "doc_title"}
            or not text.strip()
        ):
            continue
        if len(evidence) >= 12 or used + len(text) > max_chars:
            omitted += 1
            continue
        # Keep whole source blocks: no model-input truncation can remove a condition.
        evidence.append(
            {
                "id": f"E{len(evidence) + 1}",
                "block_id": bid,
                "chunk_ids": sorted(set(chunks.get(bid, []))),
                "text": text,
                "raw_text": block.get("raw_text", text),
                "chapter_path": paths[bid],
                "page": block["page"],
                "bbox": block["bbox"],
                "printed_label": block.get("printed_label"),
                "quality": block.get("quality"),
                "limitations": block.get("limitations", []),
                "role": "primary" if bid in primary_ids else "retrieved" if bid in seeds else "neighbor",
            }
        )
        used += len(text)
    packet = {
        "schema_version": 1,
        "kb": kb,
        "version": manifest["version"],
        "source_sha256": manifest["source"]["sha256"],
        "filename": manifest["source"].get("filename", kb + ".pdf"),
        "query": query,
        "chapter": chapter,
        "mode": mode,
        "answerability": "not_assessed",
        "status": "candidates" if evidence else "no_evidence",
        "knowledge_base_status": manifest["status"],
        "evidence": evidence,
        "text_chars": used,
        "omitted_for_budget": omitted,
    }
    if primary_unit:
        if not set(primary_ids).issubset({e["block_id"] for e in evidence}):
            raise ValueError("主素材未完整进入证据包")
        packet["learning_unit"] = primary_unit
        packet["query"] = "围绕主素材生成一张有教材依据的学习卡"
    encoded = json.dumps(packet, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    packet["sha256"] = hashlib.sha256(encoded.encode()).hexdigest()
    return packet
