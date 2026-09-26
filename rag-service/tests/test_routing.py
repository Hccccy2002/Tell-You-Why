import pytest
from tellwhy_kb.ingest.routing import merge_native_lines, use_native
from tellwhy_kb.schemas import IngestConfig


def line(text, x0, y0, x1, y1):
    return {"text": text, "bbox": {"x0": x0, "y0": y0, "x1": x1, "y1": y1}}


def test_existing_ocr_is_not_trusted_based_on_character_count():
    text = "足够长但可能有错误的旧文本层" * 10
    native = {"text": text, "lines": [line(text, 10, 10, 400, 24)]}
    assert not use_native(native, IngestConfig(ocr_mode="auto"))
    assert use_native(native, IngestConfig(ocr_mode="auto", native_text_trusted=True))
    assert not use_native(
        {"text": "", "lines": []}, IngestConfig(ocr_mode="auto", native_text_trusted=True)
    )
    with pytest.raises(ValueError, match="explicit"):
        use_native(native, IngestConfig(ocr_mode="native"))


def test_native_fragments_are_merged_into_wrapped_prose():
    lines = [
        line("高速缓存", 10, 10, 70, 20),
        line("为什么能够", 72, 10, 145, 20),
        line("可以减少处理器等待数据的", 10, 24, 180, 34),
        line("时间。", 10, 38, 55, 48),
    ]
    merged = merge_native_lines(lines)
    assert [item["text"] for item in merged] == [
        "高速缓存为什么能够可以减少处理器等待数据的时间。"
    ]
    assert len(merged[0]["lines"]) == 4


def test_auto_rejects_a_page_that_remains_highly_fragmented():
    fragments = [line(f"词{i}", 10, i * 15, 30, i * 15 + 10) for i in range(60)]
    native = {"text": "\n".join(item["text"] for item in fragments), "lines": fragments}
    assert not use_native(native, IngestConfig(ocr_mode="auto", native_text_trusted=True))
