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
            # Rectangles follow PDFium's text grouping; the preserved full text is not overwritten.
            for i in range(text.count_rects()):
                rect = text.get_rect(i)
                line = text.get_text_bounded(*rect).strip()
                if line:
                    lines.append(
                        {
                            "text": line,
                            "bbox": display_box(rect, page.get_bbox(), page.get_rotation()).model_dump(),
                        }
                    )
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
