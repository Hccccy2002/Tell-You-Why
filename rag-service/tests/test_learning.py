import copy
import sqlite3

import pytest

import test_desktop
from tellwhy_kb.learning import block_key, units
from tellwhy_kb.evidence import assemble
from tellwhy_kb.search import SearchIndex


@pytest.fixture
def desktop(tmp_path):
    return test_desktop.desktop.__wrapped__(tmp_path)


def test_learning_units_and_evidence_are_local_repeatable_and_version_pinned(desktop):
    library, _, _ = desktop
    first = library.learning_units("test", "version1", "c1")
    assert first == library.learning_units("test", "version1", "c1")
    assert len(first["items"]) == 1
    unit = first["items"][0]
    packet = library.learning_evidence("test", "version1", unit["id"], "c1")
    assert packet["evidence"][0]["role"] == "primary"
    assert packet["evidence"][0]["text"] == "存储器存放程序和数据。"
    assert packet["learning_unit"] == unit
    assert packet["text_chars"] <= 10000
    assert packet["version"] == "version1"


def test_learning_rejects_wrong_scope_version_and_unknown_unit(desktop):
    library, _, _ = desktop
    for version, chapter in [("old", "c1"), ("version1", "missing")]:
        with pytest.raises(ValueError):
            library.learning_units("test", version, chapter)
    with pytest.raises(ValueError):
        library.learning_evidence("test", "version1", "invented", "c1")


def test_learning_primary_survives_empty_retrieval_and_budget_is_not_truncated(desktop):
    library, root, _ = desktop
    unit = library.learning_units("test", "version1")["items"][0]
    with SearchIndex(root / "versions/version1") as index:
        index.search = lambda *args, **kwargs: {"results": []}
        packet = assemble(
            index, library.published("test")[1], "test", "不存在的查询", "c1", "keyword", primary_unit=unit
        )
        assert len(packet["evidence"]) == 1
        too_large = copy.deepcopy(unit)
        too_large["block_ids"] *= 13
        with pytest.raises(ValueError, match="预算"):
            assemble(
                index, library.published("test")[1], "test", "查询", "c1", "keyword", primary_unit=too_large
            )


def test_learning_quality_filter_and_stable_source_key(desktop):
    library, root, args = desktop
    original = args[3][0]
    manifest = library.published("test")[1]
    key = block_key(manifest["source"]["sha256"], original)
    assert key == block_key(manifest["source"]["sha256"], {**original, "text": " " + original["text"] + "\n"})
    assert key != block_key(manifest["source"]["sha256"], {**original, "text": original["text"] + "修改"})
    with sqlite3.connect(root / "versions/version1/knowledge.sqlite") as db:
        import json

        db.row_factory = sqlite3.Row
        for kind in ["exercise", "reference", "formula"]:
            block = {**original, "content_kind": kind}
            db.execute("UPDATE blocks SET data=?", [json.dumps(block)])
            assert units(db, manifest, "test") == []
        db.execute("UPDATE blocks SET data=?", [json.dumps({**original, "eligible": False})])
        assert units(db, manifest, "test") == []
