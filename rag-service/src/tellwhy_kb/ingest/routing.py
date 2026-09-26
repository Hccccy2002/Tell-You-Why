from __future__ import annotations

import re
import statistics

from ..schemas import Block, Box, IngestConfig, PageResult
from ..util import digest


def _box(line):
    return line["bbox"]


def _height(line):
    box = _box(line)
    return max(1.0, box["y1"] - box["y0"])


def _join_text(left: str, right: str) -> str:
    if left[-1:].isascii() and right[:1].isascii() and left[-1:].isalnum() and right[:1].isalnum():
        return left + " " + right
    return left + right


def _combined(left: dict, right: dict, text: str) -> dict:
    a, b = _box(left), _box(right)
    return {
        "text": text,
        "bbox": {
            "x0": min(a["x0"], b["x0"]),
            "y0": min(a["y0"], b["y0"]),
            "x1": max(a["x1"], b["x1"]),
            "y1": max(a["y1"], b["y1"]),
        },
        "lines": [*left.get("lines", [left]), *right.get("lines", [right])],
    }


def _same_visual_line(left: dict, right: dict) -> bool:
    a, b = _box(left), _box(right)
    center_gap = abs((a["y0"] + a["y1"]) / 2 - (b["y0"] + b["y1"]) / 2)
    if center_gap > max(2.0, min(_height(left), _height(right)) * 0.65):
        return False
    horizontal_gap = max(0.0, max(a["x0"], b["x0"]) - min(a["x1"], b["x1"]))
    return horizontal_gap <= max(24.0, max(_height(left), _height(right)) * 4)


def _looks_like_heading(text: str) -> bool:
    value = text.strip()
    return len(value) <= 36 and bool(
        re.match(
            r"^(?:第[一二三四五六七八九十百]+[章节篇]|\d+$|\d+(?:[.．]\d+){0,3}\s*\S)",
            value,
        )
    )


def merge_native_lines(lines: list[dict]) -> list[dict]:
    """Merge PDF text-layer glyph fragments into lines and wrapped prose blocks."""
    fragments = [line for line in lines if line.get("text", "").strip() and line.get("bbox")]
    visual_lines = []
    for fragment in fragments:
        item = {
            "text": fragment["text"].strip(),
            "bbox": dict(fragment["bbox"]),
            "lines": [fragment],
        }
        if visual_lines and _same_visual_line(visual_lines[-1], item):
            visual_lines[-1] = _combined(
                visual_lines[-1], item, _join_text(visual_lines[-1]["text"], item["text"])
            )
        else:
            visual_lines.append(item)

    if len(visual_lines) < 2:
        return visual_lines
    vertical_gaps = [
        max(0.0, current["bbox"]["y0"] - previous["bbox"]["y1"])
        for previous, current in zip(visual_lines, visual_lines[1:])
        if current["bbox"]["y0"] >= previous["bbox"]["y0"]
    ]
    normal_gap = statistics.median(vertical_gaps) if vertical_gaps else 0.0
    paragraphs = []
    for line in visual_lines:
        if not paragraphs:
            paragraphs.append(line)
            continue
        previous = paragraphs[-1]
        gap = line["bbox"]["y0"] - previous["bbox"]["y1"]
        aligned = abs(line["bbox"]["x0"] - previous["bbox"]["x0"]) <= max(
            18.0, min(_height(line), _height(previous)) * 2
        )
        wrapped = (
            gap >= -2.0
            and gap <= max(18.0, normal_gap * 1.8)
            and aligned
            and len(previous["text"]) >= 8
            and not re.search(r"[。！？；]$", previous["text"])
            and not _looks_like_heading(line["text"])
        )
        if wrapped:
            previous["text"] = _join_text(previous["text"], line["text"])
            previous["bbox"] = _combined(previous, line, previous["text"])["bbox"]
            previous["lines"].extend(line["lines"])
        else:
            paragraphs.append(line)
    return paragraphs


def native_is_too_fragmented(native: dict) -> bool:
    text_chars = len(re.sub(r"\s+", "", native.get("text", "")))
    merged = merge_native_lines(native.get("lines", []))
    lengths = [len(re.sub(r"\s+", "", line["text"])) for line in merged]
    covered = sum(lengths)
    if text_chars == 0 or not lengths or covered / text_chars < 0.8:
        return True
    if text_chars < 120 or len(lengths) < 8:
        return False
    median = statistics.median(lengths)
    short_fraction = sum(length <= 6 for length in lengths) / len(lengths)
    return median <= 6 and short_fraction >= 0.6


def use_native(native: dict, config: IngestConfig) -> bool:
    if config.ocr_mode == "always":
        return False
    if not config.native_text_trusted:
        if config.ocr_mode == "native":
            raise ValueError("Native extraction requires explicit native_text_trusted=true")
        return False
    text = native["text"].strip()
    usable = len(text) >= 20 and "\ufffd" not in text and not re.search(r"[\x00-\x08]", text)
    return usable and not native_is_too_fragmented(native)


def native_page(native: dict, number: int, config: IngestConfig) -> PageResult:
    blocks = []
    for order, line in enumerate(merge_native_lines(native["lines"])):
        text = line["text"]
        if not text.strip():
            continue
        blocks.append(
            Block(
                id=f"p{number:04d}-{order:03d}-{digest(text)[:8]}",
                page=number,
                kind="text",
                text=text,
                bbox=Box(**line["bbox"]),
                order=order,
                eligible=True,
                limitations=[],
                lines=line.get("lines", [line]),
            )
        )
    excluded = number < config.exclude_before_page
    if excluded:
        for block in blocks:
            block.eligible = False
    return PageResult(
        number=number,
        width=native["width"],
        height=native["height"],
        status="excluded" if excluded else "success",
        method="native",
        native_text=native["text"],
        blocks=blocks,
        limitations=["native_text_explicitly_trusted; layout not classified"],
    )
