from __future__ import annotations

import re

from ..schemas import Block, Box, IngestConfig, PageResult
from ..util import digest


def use_native(native: dict, config: IngestConfig) -> bool:
    if config.ocr_mode == "always":
        return False
    if not config.native_text_trusted:
        if config.ocr_mode == "native":
            raise ValueError("Native extraction requires explicit native_text_trusted=true")
        return False
    text = native["text"].strip()
    usable = len(text) >= 20 and "\ufffd" not in text and not re.search(r"[\x00-\x08]", text)
    return usable


def native_page(native: dict, number: int, config: IngestConfig) -> PageResult:
    blocks = []
    for order, line in enumerate(native["lines"]):
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
                lines=[line],
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
