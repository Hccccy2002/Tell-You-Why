from PIL import Image
from tellwhy_kb.ingest.assessment import assess_page
from tellwhy_kb.schemas import Block, Box, PageResult


def test_high_confidence_equation_is_not_treated_as_validated_prose(tmp_path):
    page = PageResult(
        number=1,
        width=100,
        height=100,
        status="success",
        method="ocr",
        blocks=[
            Block(
                id="one",
                page=1,
                order=0,
                kind="text",
                text="=27.5",
                confidence=0.99,
                bbox=Box(x0=1, y0=2, x1=50, y1=20),
                eligible=True,
            ),
            Block(
                id="two",
                page=1,
                order=1,
                kind="text",
                text="存储器用于存放程序和数据。",
                confidence=0.99,
                bbox=Box(x0=1, y0=22, x1=90, y1=40),
                eligible=True,
            ),
            Block(
                id="three",
                page=1,
                order=2,
                kind="text",
                text="为处理I/0中断",
                confidence=0.99,
                bbox=Box(x0=1, y0=42, x1=90, y1=60),
                eligible=True,
            ),
        ],
    )
    with Image.new("RGB", (200, 200), "white") as image:
        result = assess_page(page, image, tmp_path)
    assert result.status == "needs_review"
    assert not result.blocks[0].eligible
    assert (tmp_path / result.blocks[0].asset).is_file()
    assert result.blocks[1].eligible
    assert not result.blocks[2].eligible


def test_detected_text_region_without_ocr_is_preserved_for_review(tmp_path):
    page = PageResult(
        number=1,
        width=100,
        height=100,
        status="success",
        method="ocr",
        blocks=[
            Block(id="missing", page=1, order=0, kind="text", text="", bbox=Box(x0=1, y0=2, x1=90, y1=20))
        ],
    )
    with Image.new("RGB", (200, 200), "white") as image:
        result = assess_page(page, image, tmp_path)
    assert result.status == "needs_review"
    assert "layout_region_has_no_recognized_text" in result.blocks[0].limitations
    assert (tmp_path / result.blocks[0].asset).is_file()
