import pytest
from tellwhy_kb.ingest.routing import use_native
from tellwhy_kb.schemas import IngestConfig


def test_existing_ocr_is_not_trusted_based_on_character_count():
    native = {"text": "足够长但可能有错误的旧文本层" * 10}
    assert not use_native(native, IngestConfig(ocr_mode="auto"))
    assert use_native(native, IngestConfig(ocr_mode="auto", native_text_trusted=True))
    assert not use_native({"text": ""}, IngestConfig(ocr_mode="auto", native_text_trusted=True))
    with pytest.raises(ValueError, match="explicit"):
        use_native(native, IngestConfig(ocr_mode="native"))
