import copy
import sqlite3
import numpy as np
import pytest

from tellwhy_kb.schemas import IngestConfig
from tellwhy_kb.search import SearchIndex
from tellwhy_kb.storage.manifest import publish_version, active_version, validate_version
from tellwhy_kb.storage.manifest import promote_directory
from tellwhy_kb.util import file_hash, read_json, atomic_json


def test_windows_promotion_retries_transient_locks_and_preserves_other_errors(tmp_path, monkeypatch):
    attempts = []
    locked = PermissionError("sharing violation")
    locked.winerror = 32

    def replace(*_):
        attempts.append(1)
        if len(attempts) < 3:
            raise locked

    monkeypatch.setattr("tellwhy_kb.storage.manifest.os.replace", replace)
    monkeypatch.setattr("tellwhy_kb.storage.manifest.time.sleep", lambda _: None)
    promote_directory(tmp_path / "staging", tmp_path / "published")
    assert len(attempts) == 3

    def denied(*_):
        raise PermissionError("permanent permission error")

    monkeypatch.setattr("tellwhy_kb.storage.manifest.os.replace", denied)
    with pytest.raises(PermissionError, match="permanent"):
        promote_directory(tmp_path / "staging", tmp_path / "published")


def sample(tmp_path):
    source = tmp_path / "sources"
    source.mkdir()
    original = source / "input.pdf"
    from reportlab.pdfgen import canvas

    pdf = canvas.Canvas(str(original))
    pdf.drawString(50, 700, "Memory stores instructions and data.")
    pdf.save()
    sha = file_hash(original)
    original.rename(source / f"{sha}.pdf")
    metadata = {
        "source_sha256": sha,
        "kb": "test",
        "id": "job1",
        "probe": {"pages": 1},
        "models": {},
        "runtime": {},
        "config": IngestConfig().model_dump(),
    }
    pages = [{"number": 1, "status": "success"}]
    chapters = [{"id": "c1", "parent_id": None, "title": "Memory"}]
    box = {"x0": 1, "y0": 2, "x1": 90, "y1": 20}
    block = {
        "id": "b1",
        "page": 1,
        "text": "存储器存放程序和数据。",
        "raw_text": "存储器存放程序和数据。",
        "normalized_to_raw": list(range(11)),
        "eligible": True,
        "kind": "text",
        "content_kind": "body",
        "chapter_id": "c1",
        "section_id": "c1",
        "bbox": box,
    }
    location = {
        "block_id": "b1",
        "page": 1,
        "bbox": box,
        "normalized_start": 0,
        "normalized_end": 11,
        "raw_start": 0,
        "raw_end": 11,
        "chunk_start": 0,
        "chunk_end": 11,
    }
    chunk = {
        "id": "k1",
        "chapter_id": "c1",
        "section_id": "c1",
        "text": block["text"],
        "tokens": 13,
        "eligible": True,
        "content_kind": "body",
        "embedding_text": block["text"],
        "locations": [location],
        "chapter_path": ["Memory"],
        "quality": "test",
        "limitations": [],
    }
    matrix = np.zeros((1, 512), dtype=np.float32)
    matrix[:, 0] = 1
    return metadata, pages, chapters, [block], [chunk], matrix, tmp_path / "crops", ["存储器"]


def test_atomic_publication_reuses_version_and_rejects_broken_rebuild(tmp_path):
    args = sample(tmp_path)
    first = publish_version(tmp_path, "version1", *args)
    assert first["status"] == "ready"
    assert validate_version(active_version(tmp_path))["valid"]
    assert publish_version(tmp_path, "version1", *args)["reused"]
    bad = copy.deepcopy(args)
    bad[4][0]["locations"][0]["raw_end"] = 10
    with pytest.raises(ValueError, match="Raw source"):
        publish_version(tmp_path, "version2", *bad)
    assert active_version(tmp_path).name == "version1"
    assert validate_version(active_version(tmp_path))["valid"]


def test_tampered_vector_and_fts_are_rejected(tmp_path):
    args = sample(tmp_path)
    publish_version(tmp_path, "version1", *args)
    folder = active_version(tmp_path)
    with sqlite3.connect(folder / "knowledge.sqlite") as db:
        db.execute("DELETE FROM chunks_fts")
    with pytest.raises(ValueError, match="Artifact changed"):
        validate_version(folder)
    manifest = read_json(folder / "manifest.json")
    manifest["files"]["knowledge.sqlite"] = file_hash(folder / "knowledge.sqlite")
    atomic_json(folder / "manifest.json", manifest)
    with pytest.raises(ValueError, match="Keyword index"):
        validate_version(folder)


def test_search_scope_includes_descendants_before_top_k(tmp_path):
    args = list(sample(tmp_path))
    args[2] = [
        {"id": "part", "parent_id": None, "title": "Hardware"},
        {"id": "c1", "parent_id": "part", "title": "Memory"},
        {"id": "s1", "parent_id": "c1", "title": "Main memory"},
        {"id": "s11", "parent_id": "s1", "title": "Memory access"},
        {"id": "other", "parent_id": None, "title": "Other"},
    ]
    block = copy.deepcopy(args[3][0])
    chunk = copy.deepcopy(args[4][0])
    # The unrelated chunk wins the unfiltered tie; filtering after top-k would lose the target.
    args[3][0].update(chapter_id="other", section_id="other")
    args[4][0].update(chapter_id="other", section_id="other")
    block.update(id="b2", chapter_id="c1", section_id="s11")
    chunk.update(id="k2", chapter_id="c1", section_id="s11")
    chunk["locations"][0]["block_id"] = "b2"
    args[3].append(block)
    args[4].append(chunk)
    args[5] = np.repeat(args[5], 2, axis=0)
    publish_version(tmp_path, "version1", *args)

    class Encoder:
        def query(self, _):
            return args[5][0]

    with SearchIndex(active_version(tmp_path), Encoder()) as index:
        for mode in ("keyword", "dense", "hybrid"):
            assert index.search("存储器", 1, mode)["results"][0]["chunk_id"] == "k1"
            for scope in ("part", "c1", "s1", "s11", "Main memory"):
                hits = index.search("存储器", 1, mode, scope)["results"]
                assert [h["chunk_id"] for h in hits] == ["k2"]
