from __future__ import annotations

import re
from collections import defaultdict

from .chapters import chapter_for


def normalize_text(raw: str) -> tuple[str, list[int]]:
    """Join OCR wraps without changing digits, signs, superscripts, or letters.

    Each output character maps to an original character; a retained separator maps
    to the first original whitespace character. No semantic corrections are made.
    """
    output, mapping = [], []
    i = 0
    while i < len(raw):
        if not raw[i].isspace():
            output.append(raw[i])
            mapping.append(i)
            i += 1
            continue
        start = i
        while i < len(raw) and raw[i].isspace():
            i += 1
        previous = output[-1] if output else ""
        following = raw[i] if i < len(raw) else ""
        if (
            previous
            and following
            and previous.isascii()
            and following.isascii()
            and previous.isalnum()
            and following.isalnum()
        ):
            output.append(" ")
            mapping.append(start)
    return "".join(output), mapping


def normalize_blocks(pages: list, chapters: list[dict]) -> list[dict]:
    recurring = defaultdict(set)
    for page in pages:
        for b in page.blocks:
            if b.bbox.y0 < page.height * 0.12 or b.bbox.y1 > page.height * 0.92:
                key = re.sub(r"[\d\s]", "", b.text)
                if len(key) < 70:
                    recurring[key].add(page.number)
    headers = {k for k, v in recurring.items() if len(v) >= max(3, int(len(pages) * 0.03))}
    by_id = {c["id"]: c for c in chapters}
    normalized = []
    for page in sorted(pages, key=lambda p: p.number):
        for b in page.blocks:
            text, mapping = normalize_text(b.text)
            section = chapter_for(chapters, page.number, b.order)
            data = b.model_dump()
            data.update(
                {
                    "raw_text": b.text,
                    "text": text,
                    "normalized_to_raw": mapping,
                    "printed_label": page.printed_label,
                    "section_id": section["id"] if section else None,
                    "chapter_path": section["path"] if section else [],
                    "chapter_id": None,
                    "content_kind": "body",
                }
            )
            lineage = [by_id[n] for n in section["ancestors"]] + [section] if section else []
            for node in lineage:
                if node["kind"] == "chapter":
                    data["chapter_id"] = node["id"]
                if node["kind"] in {"exercise", "appendix", "reference"}:
                    data["content_kind"] = node["kind"]
            if not data["chapter_id"] and section:
                data["chapter_id"] = section["id"]
            if (b.bbox.y0 < page.height * 0.12 or b.bbox.y1 > page.height * 0.92) and re.sub(
                r"[\d\s]", "", b.text
            ) in headers:
                data["eligible"] = False
                data["limitations"].append("recurring_header_or_footer")
            if not section or data["content_kind"] in {"exercise", "reference"}:
                data["eligible"] = False
                data["limitations"].append("outside_default_learning_content")
            normalized.append(data)
    return normalized
