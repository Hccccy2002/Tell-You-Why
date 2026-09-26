from __future__ import annotations

import pypdfium2 as pdfium

from ..schemas import Box


def display_box(rect, crop, rotation: int) -> Box:
    left, bottom, right, top = crop
    width, height = right - left, top - bottom
    points = []
    for x, y in [(rect[0], rect[1]), (rect[2], rect[3])]:
        x, y = x - left, y - bottom
        if rotation == 0:
            point = (x, height - y)
        elif rotation == 90:
            point = (y, x)
        elif rotation == 180:
            point = (width - x, y)
        elif rotation == 270:
            point = (height - y, width - x)
        else:
            raise ValueError("Unsupported PDF rotation")
        points.append(point)
    w, h = (height, width) if rotation in (90, 270) else (width, height)
    return Box(
        x0=max(0, min(w, min(x for x, y in points))),
        y0=max(0, min(h, min(y for x, y in points))),
        x1=max(0, min(w, max(x for x, y in points))),
        y1=max(0, min(h, max(y for x, y in points))),
    )


class PdfSource:
    def __init__(self, path):
        self.doc = pdfium.PdfDocument(str(path))

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.doc.close()

    def read(self, number: int) -> dict:
        page = self.doc[number - 1]
        text = page.get_textpage()
        try:
            whole = text.get_text_range()
            lines = []
            # PDFium's page rectangles may represent just a few glyphs even when the
            # logical text layer contains complete lines. Build line boxes from the
            # character stream so native extraction never silently drops most text.
            offset = 0
            for raw_line in whole.splitlines(keepends=True):
                line = raw_line.rstrip("\r\n")
                boxes = []
                for index, character in enumerate(line, offset):
                    if character.isspace():
                        continue
                    try:
                        left, bottom, right, top = text.get_charbox(index, loose=True)
                    except (IndexError, RuntimeError):
                        continue
                    if right > left and top > bottom:
                        boxes.append((left, bottom, right, top))
                cleaned = line.strip()
                if cleaned and boxes:
                    rect = (
                        min(box[0] for box in boxes),
                        min(box[1] for box in boxes),
                        max(box[2] for box in boxes),
                        max(box[3] for box in boxes),
                    )
                    lines.append(
                        {
                            "text": cleaned,
                            "bbox": display_box(
                                rect, page.get_bbox(), page.get_rotation()
                            ).model_dump(),
                        }
                    )
                offset += len(raw_line)
            # splitlines() returns nothing for an empty page and omits a final empty
            # line, both of which are intentionally ignored.
            width, height = page.get_size()
            return {
                "number": number,
                "width": width,
                "height": height,
                "text": whole,
                "lines": lines,
                "rotation": page.get_rotation(),
            }
        finally:
            text.close()
            page.close()

    def render(self, number: int, dpi: int):
        page = self.doc[number - 1]
        try:
            bitmap = page.render(scale=dpi / 72)
            try:
                return bitmap.to_pil().convert("RGB")
            finally:
                bitmap.close()
        finally:
            page.close()
