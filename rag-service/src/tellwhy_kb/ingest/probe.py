from __future__ import annotations

import re
from pathlib import Path

from pypdf import PdfReader

from ..util import file_hash


def inspect_pdf(path: Path) -> dict:
    path = path.resolve(strict=True)
    with path.open("rb") as f:
        if b"%PDF-" not in f.read(1024):
            raise ValueError("Not a PDF file")
    reader = PdfReader(path)
    if reader.is_encrypted:
        raise ValueError("Encrypted PDF: supply an unencrypted copy")
    count = len(reader.pages)
    if not count:
        raise ValueError("PDF contains no pages")
    bookmarks = []

    def visit(items, depth=0):
        for item in items:
            if isinstance(item, list):
                visit(item, depth + 1)
                continue
            try:
                number = reader.get_destination_page_number(item)
                page = number + 1 if number is not None and 0 <= number < count else None
            except (ValueError, TypeError, KeyError):
                page = None
            bookmarks.append({"title": item.title, "page": page, "depth": depth})

    visit(reader.outline)
    metadata = {str(k): str(v) for k, v in (reader.metadata or {}).items()}
    return {
        "path": str(path),
        "filename": path.name,
        "sha256": file_hash(path),
        "pages": count,
        "bytes": path.stat().st_size,
        "metadata": metadata,
        "bookmarks": bookmarks,
        "has_page_labels": "/PageLabels" in reader.trailer["/Root"],
        "page_labels": reader.page_labels if "/PageLabels" in reader.trailer["/Root"] else None,
        "suspected_prior_ocr": bool(re.search(r"capture|ocr|scanner|ricoh", str(metadata), re.I)),
    }
