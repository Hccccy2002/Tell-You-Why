from __future__ import annotations

import re

from ..util import digest


def compact(text: str) -> str:
    return re.sub(r"\s+", "", text).replace("．", ".")


def heading_text(text: str) -> str:
    # OCR may emit the title before its number when they share a slightly uneven baseline.
    lines = [compact(t) for t in text.splitlines() if t.strip()]
    numbers = [t for t in lines if re.fullmatch(r"第\d+章|\d+(?:\.\d+){1,3}", t)]
    if len(numbers) == 1 and len(lines) > 1:
        return numbers[0] + "".join(t for t in lines if t != numbers[0])
    return compact(text)


def node_kind(title: str) -> str:
    if re.match(r"第[\d一二三四五六七八九十]+篇", title):
        return "part"
    if re.match(r"第[\d一二三四五六七八九十]+章|Chapter\s+\d+", title, re.I):
        return "chapter"
    if re.search(r"思考题|习题|练习题", title):
        return "exercise"
    if title.startswith("附录"):
        return "appendix"
    if title in {"参考文献", "参考资料", "References", "Bibliography"}:
        return "reference"
    return "section"


def heading_order(title: str, page) -> int | None:
    if page is None:
        return None
    target = compact(title)
    for i, block in enumerate(page.blocks):
        if block.kind in {"header", "footer", "number"}:
            continue
        candidates = [heading_text(block.text)]
        if i + 1 < len(page.blocks):
            other = page.blocks[i + 1]
            heading_pair = block.kind in {"paragraph_title", "doc_title"} and other.kind in {
                "paragraph_title",
                "doc_title",
            }
            number_pair = (
                any(re.fullmatch(r"第\d+章|\d+(?:\.\d+){1,3}", compact(t)) for t in (block.text, other.text))
                and max(len(block.text), len(other.text)) < 70
            )
            if heading_pair or number_pair:
                candidates.append(heading_text(block.text + "\n" + other.text))
        if any(text.startswith(target) for text in candidates):
            return block.order
    return None


def build_chapters(probe: dict, pages: list, overrides: dict | None = None) -> list[dict]:
    """Keep original bookmarks, record repairs, and anchor section starts to blocks.

    Positions are half-open (physical page, block order). Sections beginning in the
    middle of a page therefore never claim the preceding paragraph on that page.
    """
    page_map = {p.number: p for p in pages}
    fixes = {}
    if overrides:
        if overrides["source_sha256"] != probe["sha256"]:
            raise ValueError("Chapter overrides belong to a different PDF")
        fixes = {x["title"]: x for x in overrides.get("bookmarks", [])}
    bookmarks = probe["bookmarks"]
    nodes = []
    has_parts = any(node_kind(b["title"]) == "part" for b in bookmarks)
    chapter_level = 1 if has_parts else 0
    last_valid = 1
    for i, bookmark in enumerate(bookmarks):
        original = bookmark["title"]
        fix = fixes.get(original, {})
        title = fix.get("new_title", original)
        kind = node_kind(title)
        level = (
            0
            if kind == "part"
            else chapter_level
            if kind == "chapter"
            else bookmark["depth"] + int(has_parts)
        )
        number = fix.get("page", bookmark["page"])
        notes = []
        if fix:
            notes.append(fix["reason"])
        if number is not None and not 1 <= number <= probe["pages"]:
            raise ValueError("Chapter page outside document")
        if number is not None and number < last_valid:
            notes.append("non_monotonic_bookmark; position unresolved")
            number = None
        if number is not None:
            last_valid = number
        order = heading_order(title, page_map.get(number))
        if number is not None and order is None:
            notes.append("page_boundary_only; heading not located in extracted blocks")
        nodes.append(
            {
                "id": f"b{i:04d}",
                "title": title,
                "kind": kind,
                "level": level,
                "start": [number, order or 0] if number else None,
                "source": "manual_override" if fix else "pdf_bookmark",
                "original_title": original,
                "original_page": bookmark["page"],
                "notes": notes,
            }
        )
    for i, node in enumerate(nodes):
        if node["kind"] == "part" and node["start"] is None:
            following = next((n for n in nodes[i + 1 :] if n["start"] is not None), None)
            if following:
                node["start"] = following["start"].copy()
                node["notes"].append("part_range_inferred_from_following_chapter")
    if not nodes:
        nodes.append(
            {
                "id": "document",
                "title": "Document",
                "kind": "document",
                "level": 0,
                "start": [1, 0],
                "source": "document_fallback",
                "notes": [],
            }
        )

    # Establish preliminary parent scopes before adding heading-only subsections.
    finalize_ranges(nodes, probe["pages"])
    for node in nodes:
        node["outline_parent_id"] = node["parent_id"]
        node["outline_ancestors"] = node["ancestors"].copy()
        node["outline_path"] = node["path"].copy()
    seen = {compact(n["title"]) for n in nodes}
    additions = []
    for page in pages:
        if page.status in {"excluded", "blank"}:
            continue
        for i, block in enumerate(page.blocks):
            if block.kind not in {"paragraph_title", "doc_title"}:
                continue
            title = heading_text(block.text)
            if re.fullmatch(r"\d+(?:\.\d+){1,3}", title) and i + 1 < len(page.blocks):
                other = page.blocks[i + 1]
                if other.kind in {"paragraph_title", "doc_title"}:
                    title += compact(other.text)
            numbered = re.match(r"^(\d+(?:\.\d+){1,3})(?![\d.])\s*(\D.+)$", title)
            special = node_kind(title)
            if title in seen or (not numbered and special not in {"chapter", "reference"}):
                continue
            parent = chapter_for(nodes, page.number, block.order)
            if parent is None:
                continue
            number = numbered.group(1) if numbered else None
            if number:
                lineage = [n for n in nodes if n["id"] in parent["ancestors"] + [parent["id"]]]
                # Numbered questions and bibliography entries must not terminate their excluded scope.
                if any(n["kind"] in {"exercise", "reference"} for n in lineage):
                    continue
                roots = [n for n in lineage if n["kind"] == "chapter"]
                if roots:
                    chapter_number = re.search(r"\d+", roots[-1]["title"])
                    if chapter_number and number.split(".")[0] != chapter_number.group():
                        continue
                level = chapter_level + number.count(".")
            else:
                level = chapter_level if special in {"chapter", "reference"} else parent["level"] + 1
            additions.append(
                {
                    "id": "h" + digest(block.id)[:12],
                    "title": title,
                    "kind": special,
                    "level": level,
                    "start": [page.number, block.order],
                    "source": "ocr_heading",
                    "notes": ["heading_inferred_from_layout_and_numbering"],
                }
            )
            seen.add(title)
    nodes.extend(additions)
    # Sort known positions; unresolved destinations retain their original outline lineage.
    nodes.sort(key=lambda n: (n["start"] or [probe["pages"] + 1, 0], n["level"]))
    finalize_ranges(nodes, probe["pages"])
    return nodes


def finalize_ranges(nodes: list[dict], total_pages: int):
    stack = []
    for i, node in enumerate(nodes):
        if node["start"] is None and "outline_path" in node:
            node["parent_id"] = node["outline_parent_id"]
            node["ancestors"] = node["outline_ancestors"].copy()
            node["path"] = node["outline_path"].copy()
            node["end"] = None
            continue
        while stack and stack[-1]["level"] >= node["level"]:
            stack.pop()
        node["parent_id"] = stack[-1]["id"] if stack else None
        node["ancestors"] = [n["id"] for n in stack]
        node["path"] = [n["title"] for n in stack] + [node["title"]]
        node["end"] = (
            next(
                (
                    n["start"]
                    for n in nodes[i + 1 :]
                    if n["start"] is not None and n["level"] <= node["level"]
                ),
                [total_pages + 1, 0],
            )
            if node["start"]
            else None
        )
        if node["start"] and node["end"] < node["start"]:
            raise ValueError("Invalid chapter range")
        stack.append(node)


def chapter_for(nodes: list[dict], page: int, order: int) -> dict | None:
    position = [page, order]
    candidates = [n for n in nodes if n["start"] is not None and n["start"] <= position < n["end"]]
    return max(candidates, key=lambda n: (n["level"], n["start"])) if candidates else None
