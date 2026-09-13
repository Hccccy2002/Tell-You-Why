"""Bounded, version-pinned source evidence; retrieval is not an answerability test."""

from __future__ import annotations

import hashlib
import json
import math
import unicodedata

from .processing.chunking import validate_locations


def assemble(index, manifest, kb, query, chapter, mode="hybrid", max_chars=10000, primary_unit=None,
             reranker=None):
    if not 100 <= max_chars <= 10000:
        raise ValueError("Invalid evidence budget")
    if reranker is not None and primary_unit is not None:
        raise ValueError("主素材证据不能由展示重排替换")
    ranked = index.search(query, top_k=24 if reranker is not None else 8, mode=mode, chapter=chapter)
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

    # Seed blocks first. Look past at most three unusable blocks (e.g. a diagram),
    # but stop at a page/section boundary or the nearest usable prose block.
    candidates = list(seeds)
    for bid in seeds:
        seed = by_id[bid]
        for direction in (-1, 1):
            for distance in range(1, 5):
                p = positions[bid] + direction * distance
                if not 0 <= p < len(blocks):
                    break
                neighbor = blocks[p]
                if not seed.get("section_id") or neighbor.get("section_id") != seed["section_id"]:
                    break
                if distance > 1 and neighbor["page"] != seed["page"]:
                    break
                if (
                    not neighbor.get("eligible")
                    or neighbor.get("kind") in {"paragraph_title", "doc_title"}
                    or not neighbor["text"].strip()
                ):
                    continue
                nid = neighbor["id"]
                if nid not in candidates:
                    candidates.append(nid)
                    paths[nid] = paths[bid]
                break

    ranking = None
    relevance = {}
    if reranker is not None:
        # Rank actual source blocks, including usable context, before imposing the display budget.
        # Chunk overlap and repeated text must not occupy multiple Top 5 positions.
        unique, seen_text = [], set()
        for bid in candidates:
            block = by_id[bid]
            normalized = "".join(unicodedata.normalize("NFKC", block["text"]).split())
            if (not block.get("eligible") or block.get("kind") in {"paragraph_title", "doc_title"}
                    or not normalized or normalized in seen_text):
                continue
            unique.append(bid)
            seen_text.add(normalized)
        scores = reranker.score(query, [by_id[bid]["text"] for bid in unique]) if unique else []
        if len(scores) != len(unique) or any(not math.isfinite(s) for s in scores):
            raise ValueError("原文重排结果无效")
        relevance = dict(zip(unique, scores, strict=True))
        candidates = sorted(unique, key=lambda bid: -relevance[bid])
        ranking = {
            "method": "retrieve_rerank", "retrieval": mode,
            "model": reranker.manifest["repository"], "revision": reranker.manifest["revision"],
            "candidate_chunks": len(ranked["results"]), "candidate_blocks": len(unique), "top_k": 5,
        }

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
        if len(evidence) >= (5 if reranker is not None else 12) or used + len(text) > max_chars:
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
        if reranker is not None:
            evidence[-1]["relevance_score"] = relevance[bid]
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
    if ranking is not None:
        packet["ranking"] = ranking
    if primary_unit:
        if not set(primary_ids).issubset({e["block_id"] for e in evidence}):
            raise ValueError("主素材未完整进入证据包")
        packet["learning_unit"] = primary_unit
        packet["query"] = "围绕主素材生成一张有教材依据的学习卡"
    encoded = json.dumps(packet, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    packet["sha256"] = hashlib.sha256(encoded.encode()).hexdigest()
    return packet
