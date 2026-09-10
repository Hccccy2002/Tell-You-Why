import json

import pytest
from pydantic import ValidationError

from tellwhy_kb.schemas import Box, IngestConfig, PageResult
from tellwhy_kb.util import atomic_json, digest, parse_pages, safe_name


@pytest.mark.parametrize("value", ["../book", "C:\\data", "x/y", "", "a" * 65])
def test_path_ids_cannot_escape(value):
    with pytest.raises(ValueError):
        safe_name(value)


def test_page_ranges_are_deduplicated_and_bounded():
    assert parse_pages("3,1-3,5", 5) == [1, 2, 3, 5]
    for spec in ["0", "6", "3-2", "1,,2", "-1", "1-99999999"]:
        with pytest.raises(ValueError):
            parse_pages(spec, 5)


def test_atomic_json_replaces_and_hash_is_order_independent(tmp_path):
    p = tmp_path / "a.json"
    atomic_json(p, {"one": 1})
    atomic_json(p, {"two": "教材"})
    assert json.loads(p.read_text(encoding="utf-8")) == {"two": "教材"}
    assert digest({"a": 1, "b": 2}) == digest({"b": 2, "a": 1})
    assert list(tmp_path.glob("*.tmp")) == []


def test_reject_invalid_geometry_and_missing_page_status():
    with pytest.raises(ValidationError):
        Box(x0=5, y0=0, x1=3, y1=1)
    with pytest.raises(ValidationError):
        PageResult(number=1, width=1, height=1, method="native")
    with pytest.raises(ValidationError):
        IngestConfig(chunk_tokens=64, overlap_tokens=64)
    with pytest.raises(ValidationError):
        IngestConfig(dpi=900)
