import copy
import json

import pytest

from tellwhy_kb.evidence import assemble
from tellwhy_kb.search import SearchIndex
import test_desktop


@pytest.fixture
def desktop(tmp_path):
    return test_desktop.desktop.__wrapped__(tmp_path)


def test_evidence_is_source_backed_bounded_and_repeatable(desktop):
    library, _, _ = desktop
    packet = library.evidence("test", "存储器", "version1", "c1", "keyword")
    assert packet == library.evidence("test", "存储器", "version1", "c1", "keyword")
    assert packet["answerability"] == "not_assessed"
    assert packet["text_chars"] == 11
    assert packet["version"] == "version1"
    assert packet["evidence"][0]["text"] == "存储器存放程序和数据。"
    assert packet["evidence"][0]["page"] == 1
    assert packet["evidence"][0]["block_id"] == "b1"
    assert packet["evidence"][0]["chunk_ids"] == ["k1"]
    assert library.page("test", 1, "version1")["image"].startswith("data:image/png")


def test_scope_version_and_empty_evidence(desktop):
    library, _, _ = desktop
    for kwargs in ({"version": "old"}, {"chapter": "missing"}, {"query": ""}, {"mode": "dense"}):
        with pytest.raises(ValueError):
            library.evidence(
                **{"kb": "test", "query": "存储器", "version": "version1", "mode": "keyword", **kwargs}
            )
    with pytest.raises((OSError, ValueError)):
        library.evidence("another-book", "存储器", "version1")
    assert library.evidence("test", "香蕉", "version1", mode="keyword")["status"] == "no_evidence"


def test_overlap_dedup_and_neighbor_quality_scope(desktop):
    library, root, args = desktop
    import sqlite3

    path = root / "versions/version1/knowledge.sqlite"
    with sqlite3.connect(path) as db:
        # Adjacent eligible same-section block is included; other chapter and restricted text aren't.
        for bid, section, eligible in [("b2", "c1", True), ("b3", "c2", True), ("b4", "c1", False)]:
            b = {
                **args[3][0],
                "id": bid,
                "section_id": section,
                "eligible": eligible,
                "text": bid,
                "raw_text": bid,
            }
            db.execute(
                "INSERT INTO blocks VALUES (?,?,?,?,?,?)", (bid, 1, bid, bid, int(eligible), json.dumps(b))
            )
    with SearchIndex(path.parent) as index:
        original = index.search

        def repeated(*a, **kw):
            result = original(*a, **kw)
            result["results"] *= 3
            return result

        index.search = repeated
        result = assemble(index, library.published("test")[1], "test", "存储器", "c1", "keyword")
    assert [e["block_id"] for e in result["evidence"]] == ["b1", "b2"]
    assert result["evidence"][1]["role"] == "neighbor"


def test_corrupted_source_mapping_rejected(desktop):
    library, root, args = desktop
    import sqlite3

    path = root / "versions/version1/knowledge.sqlite"
    chunk = copy.deepcopy(args[4][0])
    chunk["locations"][0]["raw_end"] = 2
    with sqlite3.connect(path) as db:
        db.execute("UPDATE chunks SET data=?", (json.dumps(chunk),))
    with SearchIndex(path.parent) as index, pytest.raises(ValueError):
        assemble(index, library.published("test")[1], "test", "存储器", "c1", "keyword")


def test_eligible_appendix_text_remains_evidence(desktop):
    library, root, args = desktop
    import sqlite3

    path = root / "versions/version1/knowledge.sqlite"
    block = {**args[3][0], "content_kind": "appendix"}
    with sqlite3.connect(path) as db:
        db.execute("UPDATE blocks SET data=?", (json.dumps(block),))
    with SearchIndex(path.parent) as index:
        packet = assemble(index, library.published("test")[1], "test", "存储器", "c1", "keyword")
    assert packet["evidence"][0]["text"] == block["text"]
