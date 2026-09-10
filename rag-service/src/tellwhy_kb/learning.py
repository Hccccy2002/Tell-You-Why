"""Local learning units. Source blocks, not model-generated concepts, define identity."""

from __future__ import annotations

import hashlib
import json

RULE_VERSION = "1"


def block_key(source_hash, block):
    normalized = " ".join(block["text"].split())
    text_hash = hashlib.sha256(normalized.encode()).hexdigest()
    return f"{source_hash}:{block['id']}:{block['page']}:{text_hash}"


def units(db, manifest, kb, chapter=None):
    chapters = {r[0]: json.loads(r[1]) for r in db.execute("SELECT id,data FROM chapters")}
    if chapter and chapter not in chapters:
        raise ValueError("章节不存在")
    scope_depth, scope_node, scope_seen = 0, chapter, set()
    while scope_node in chapters and scope_node not in scope_seen:
        scope_seen.add(scope_node)
        scope_depth += 1
        scope_node = chapters[scope_node].get("parent_id")
    selected = set(chapters) if not chapter else {chapter}
    while chapter:
        expanded = selected | {cid for cid, c in chapters.items() if c.get("parent_id") in selected}
        if expanded == selected:
            break
        selected = expanded
    blocks = [json.loads(r[0]) for r in db.execute("SELECT data FROM blocks ORDER BY page,rowid")]
    groups, group = [], []
    for block in blocks:
        allowed = (
            block.get("eligible")
            and block.get("section_id") in selected
            and block.get("kind") not in {"paragraph_title", "doc_title", "header", "footer", "number"}
            and block.get("content_kind") not in {"exercise", "reference", "toc", "figure", "formula"}
            and chapters.get(block.get("section_id"), {}).get("kind") not in {"exercise", "reference"}
            and bool(block.get("text", "").strip())
            and len(block["text"]) <= 2000
        )
        if group and (
            not allowed
            or block.get("section_id") != group[-1].get("section_id")
            or block["page"] > group[-1]["page"] + 1
            or len(group) >= 3
            or sum(len(b["text"]) for b in group) + len(block["text"]) > 2000
        ):
            groups.append(group)
            group = []
        if allowed:
            group.append(block)
    if group:
        groups.append(group)
    result = []
    for group in groups:
        path, visited = [], set()
        section = group[0]["section_id"]
        while section in chapters and section not in visited:
            visited.add(section)
            path.insert(0, chapters[section]["title"])
            section = chapters[section].get("parent_id")
        keys = [block_key(manifest["source"]["sha256"], b) for b in group]
        source_key = hashlib.sha256("\n".join(keys).encode()).hexdigest()
        identity = f"{kb}:{manifest['version']}:{RULE_VERSION}:{source_key}"
        result.append(
            {
                "id": hashlib.sha256(identity.encode()).hexdigest(),
                "kb": kb,
                "version": manifest["version"],
                "rule_version": RULE_VERSION,
                "source_key": source_key,
                "source_keys": keys,
                "block_ids": [b["id"] for b in group],
                "chapter_path": path,
                "sampling_group": path[scope_depth] if len(path) > scope_depth else "本章正文",
                "section_id": group[0]["section_id"],
                "page": group[0]["page"],
                "text_chars": sum(len(b["text"]) for b in group),
                "query": group[0]["text"][:800],
            }
        )
    return result
