from __future__ import annotations

import re
from pathlib import Path

from ..schemas import PageResult


def text_limitations(text: str) -> list[str]:
    limits = []
    if re.search(r"[=＝_$^⁰¹²³⁴⁵⁶⁷⁸⁹]|\\(?:frac|bar|overline)", text):
        limits.append("mathematical_notation_requires_review")
    if re.search(r"(?<![A-Za-z])I\s*/\s*0(?![0-9])", text):
        limits.append("ambiguous_letter_O_or_digit_zero")
    if re.search(r"(?:如|见|见下|如下)?(?:图|表|公式)\s*\d+[.．]\d+", text):
        limits.append("depends_on_visual_evidence")
    if re.search(r"(?:CS|MREQ|WR).*低电平|低电平.*(?:CS|MREQ|WR)", text):
        limits.append("signal_polarity_notation_requires_review")
    return limits


def block_limitations(block) -> list[str]:
    limits = text_limitations(block.text)
    if block.kind in {"text", "paragraph_title", "doc_title", "abstract"} and not block.text.strip():
        limits.append("layout_region_has_no_recognized_text")
    return limits


def assess_page(result: PageResult, image, assets: Path) -> PageResult:
    """Conservative fallbacks for symbols or layout that the detector did not understand.

    Scores are OCR confidence, not proof of factual accuracy. Preserve questionable
    content and its pixels, but do not put it into the default learning evidence.
    """
    for block in result.blocks:
        block.limitations.extend(block_limitations(block))
        block.limitations = sorted(set(block.limitations))
        if block.limitations:
            block.eligible = False
            if not block.asset and block.bbox.x1 > block.bbox.x0 and block.bbox.y1 > block.bbox.y0:
                sx, sy = image.width / result.width, image.height / result.height
                b = block.bbox
                box = (
                    max(0, int(b.x0 * sx) - 3),
                    max(0, int(b.y0 * sy) - 3),
                    min(image.width, int(b.x1 * sx) + 4),
                    min(image.height, int(b.y1 * sy) + 4),
                )
                assets.mkdir(parents=True, exist_ok=True)
                block.asset = f"{block.id}.png"
                crop = image.crop(box)
                try:
                    crop.save(assets / block.asset)
                finally:
                    crop.close()
    if result.status == "success" and any(b.limitations for b in result.blocks):
        result.status = "needs_review"
    return result
