from io import BytesIO

import pytest
from pypdf import PdfReader, PdfWriter
from reportlab.pdfgen import canvas

from tellwhy_kb.ingest.extract import PdfSource, display_box
from tellwhy_kb.ingest.probe import inspect_pdf


def make_pdf(path, rotation=0):
    data = BytesIO()
    c = canvas.Canvas(data, pagesize=(300, 400))
    c.drawString(40, 330, "CPU and memory")
    c.showPage()
    c.drawString(40, 330, "System bus")
    c.save()
    reader = PdfReader(data)
    writer = PdfWriter()
    for page in reader.pages:
        if rotation:
            page.rotate(rotation)
        writer.add_page(page)
    writer.add_outline_item("Chapter 1", 0)
    writer.add_outline_item("Chapter 2", 1)
    with path.open("wb") as f:
        writer.write(f)


def test_bookmark_targets_and_character_positions(tmp_path):
    path = tmp_path / "教材.pdf"
    make_pdf(path)
    info = inspect_pdf(path)
    assert info["pages"] == 2
    assert [x["page"] for x in info["bookmarks"]] == [1, 2]
    assert len(info["sha256"]) == 64
    with PdfSource(path) as pdf:
        page = pdf.read(1)
        assert "CPU and memory" in page["text"]
        assert page["lines"][0]["bbox"]["y0"] < 100
        image = pdf.render(1, 144)
        assert image.size == (600, 800)
        image.close()


@pytest.mark.parametrize("rotation", [0, 90, 180, 270])
def test_rotated_page_coordinates_remain_inside_render(tmp_path, rotation):
    path = tmp_path / "rotated.pdf"
    make_pdf(path, rotation)
    with PdfSource(path) as pdf:
        page = pdf.read(1)
        for line in page["lines"]:
            box = line["bbox"]
            assert 0 <= box["x0"] <= box["x1"] <= page["width"]
            assert 0 <= box["y0"] <= box["y1"] <= page["height"]
    assert display_box((10, 20, 30, 40), (0, 0, 100, 200), rotation)


def test_non_pdf_and_encrypted_input_rejected(tmp_path):
    p = tmp_path / "fake.pdf"
    p.write_text("not a PDF")
    with pytest.raises(ValueError, match="Not a PDF"):
        inspect_pdf(p)
    w = PdfWriter()
    w.add_blank_page(100, 100)
    w.encrypt("secret")
    with p.open("wb") as f:
        w.write(f)
    with pytest.raises(ValueError, match="Encrypted"):
        inspect_pdf(p)
