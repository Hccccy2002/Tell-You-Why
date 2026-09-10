from __future__ import annotations

import time
from pathlib import Path

import numpy as np

from ..models import local_environment
from ..schemas import Block, Box, IngestConfig, PageResult
from ..util import digest
from .assessment import assess_page

BODY_KINDS = {"text", "paragraph_title", "doc_title", "abstract"}
VISUAL_KINDS = {"image", "formula", "table", "chart", "algorithm"}
IGNORED_KINDS = {"header", "footer", "number", "header_image", "footer_image", "seal"}


def reading_order(regions: list[dict], width: float) -> list[dict]:
    """Recursive whitespace cuts: separate columns where possible, then horizontal bands."""
    if len(regions) < 2:
        return regions
    for axis, minimum in [(0, width * 0.035), (1, 5)]:
        intervals = sorted((r["coordinate"][axis], r["coordinate"][axis + 2]) for r in regions)
        end = intervals[0][1]
        gaps = []
        for start, stop in intervals[1:]:
            if start - end > minimum:
                gaps.append((start - end, (start + end) / 2))
            end = max(end, stop)
        if gaps:
            cut = max(gaps)[1]
            before = [r for r in regions if r["coordinate"][axis + 2] < cut]
            after = [r for r in regions if r["coordinate"][axis] > cut]
            if before and after and len(before) + len(after) == len(regions):
                return reading_order(before, width) + reading_order(after, width)
    return sorted(regions, key=lambda r: (r["coordinate"][1], r["coordinate"][0]))


def assign_lines(regions: list[dict], lines: list[dict]) -> list[dict]:
    grouped = [{**r, "lines": []} for r in regions]
    for line in lines:
        x0, y0, x1, y1 = line["box"]
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        candidates = []
        for i, region in enumerate(grouped):
            a, b, c, d = region["coordinate"]
            if a <= cx <= c and b <= cy <= d:
                # A formula inside a large text region must retain its more specific identity.
                priority = 0 if region["label"] in VISUAL_KINDS else 1
                candidates.append((priority, (c - a) * (d - b), i))
        if candidates:
            grouped[min(candidates)[2]]["lines"].append(line)
        else:
            grouped.append({"label": "unassigned", "coordinate": line["box"], "lines": [line]})
    return grouped


class OcrEngine:
    def __init__(self, models_root: Path, config: IngestConfig):
        local_environment(models_root)
        # Paddle/ModelScope imports Torch; loading Torch first avoids conflicting Windows DLLs.
        import torch

        torch.set_num_threads(config.cpu_threads)
        import cv2

        cv2.setNumThreads(1)
        from paddleocr import PaddleOCR, LayoutDetection

        options = {"device": "cpu", "cpu_threads": config.cpu_threads, "enable_mkldnn": True}
        self.ocr = PaddleOCR(
            text_detection_model_name="PP-OCRv5_server_det",
            text_detection_model_dir=str(models_root / "det"),
            text_recognition_model_name="PP-OCRv5_server_rec",
            text_recognition_model_dir=str(models_root / "rec"),
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_textline_orientation=False,
            **options,
        )
        self.layout = LayoutDetection(
            model_name="PP-DocLayout-S", model_dir=str(models_root / "layout"), **options
        )
        self.config = config

    def process(self, source, number: int, assets: Path) -> PageResult:
        start = time.monotonic()
        native = source.read(number)
        image = source.render(number, self.config.dpi)
        try:
            pixels = np.asarray(image)[:, :, ::-1].copy()
            result = list(self.ocr.predict(pixels))[0]
            detection = list(self.layout.predict(pixels))[0]
            lines = []
            for text, score, polygon in zip(
                result["rec_texts"], result["rec_scores"], result["rec_polys"], strict=True
            ):
                poly = np.asarray(polygon)
                lines.append(
                    {
                        "text": text,
                        "confidence": float(score),
                        "box": [
                            float(poly[:, 0].min()),
                            float(poly[:, 1].min()),
                            float(poly[:, 0].max()),
                            float(poly[:, 1].max()),
                        ],
                    }
                )
            regions = reading_order(assign_lines(detection["boxes"], lines), image.width)
            sx, sy = native["width"] / image.width, native["height"] / image.height
            blocks = []
            printed = None
            for order, region in enumerate(regions):
                kind = region["label"]
                group = sorted(region["lines"], key=lambda line: (line["box"][1], line["box"][0]))
                text = "\n".join(line["text"] for line in group)
                a, b, c, d = region["coordinate"]
                bbox = Box(
                    x0=max(0, min(native["width"], float(a) * sx)),
                    y0=max(0, min(native["height"], float(b) * sy)),
                    x1=max(0, min(native["width"], float(c) * sx)),
                    y1=max(0, min(native["height"], float(d) * sy)),
                )
                bid = f"p{number:04d}-{order:03d}-{digest(text)[:8]}"
                score = min((line["confidence"] for line in group), default=None)
                limitations = []
                if kind in VISUAL_KINDS:
                    limitations.append("visual_structure_not_interpreted")
                if kind == "unassigned":
                    limitations.append("unassigned_layout")
                if score is not None and score < self.config.min_confidence:
                    limitations.append("low_ocr_confidence")
                asset = None
                if kind in VISUAL_KINDS:
                    assets.mkdir(parents=True, exist_ok=True)
                    asset = f"{bid}.png"
                    image.crop(
                        (
                            max(0, int(a)),
                            max(0, int(b)),
                            min(image.width, int(c) + 1),
                            min(image.height, int(d) + 1),
                        )
                    ).save(assets / asset)
                if kind == "number" and text.strip().isdigit():
                    printed = text.strip()
                blocks.append(
                    Block(
                        id=bid,
                        page=number,
                        kind=kind,
                        text=text,
                        bbox=bbox,
                        order=order,
                        confidence=score,
                        eligible=kind in BODY_KINDS and bool(text) and not limitations,
                        limitations=limitations,
                        asset=asset,
                        lines=[
                            {
                                **line,
                                "box": [
                                    line["box"][0] * sx,
                                    line["box"][1] * sy,
                                    line["box"][2] * sx,
                                    line["box"][3] * sy,
                                ],
                            }
                            for line in group
                        ],
                    )
                )
            ink = float(np.mean(np.asarray(image.convert("L")) < 220))
            uncertain = any(b.limitations for b in blocks)
            status = "needs_review" if uncertain or not lines else "success"
            if not lines and ink < 0.002:
                status = "blank"
            if number < self.config.exclude_before_page:
                status = "excluded"
                for block in blocks:
                    block.eligible = False
                    block.limitations.append("outside_selected_body_range")
            page_result = PageResult(
                number=number,
                width=native["width"],
                height=native["height"],
                status=status,
                method="ocr",
                native_text=native["text"],
                printed_label=printed,
                blocks=blocks,
                limitations=["OCR content requires source verification"],
                elapsed_seconds=time.monotonic() - start,
            )
            return assess_page(page_result, image, assets)
        finally:
            image.close()
