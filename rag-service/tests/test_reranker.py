import copy
import json
import sqlite3
from types import SimpleNamespace

import numpy as np
import pytest

import test_desktop
from tellwhy_kb.evidence import assemble
from tellwhy_kb.indexing.reranker import FILES, REPOSITORY, REVISION, Reranker, verify, windows
from tellwhy_kb.search import SearchIndex
from tellwhy_kb.util import atomic_json, file_hash, read_json


@pytest.fixture
def desktop(tmp_path):
    return test_desktop.desktop.__wrapped__(tmp_path)


class Scorer:
    manifest = {"repository": "test-cross-encoder", "revision": "test"}

    def score(self, query, texts):
        assert query == "存储器"
        return [float(text.split("：")[0]) if "：" in text else 0.0 for text in texts]


def candidates(desktop):
    library, root, args = desktop
    folder = root / "versions/version1"
    records = []
    with sqlite3.connect(folder / "knowledge.sqlite") as db:
        for i in range(1, 18):
            # Late, highly relevant blocks; repeated text must not use a second display position.
            text = f"{16 if i == 17 else i}：存储器的说明"
            block = {
                **copy.deepcopy(args[3][0]),
                "id": f"b{i + 1}",
                "text": text,
                "raw_text": text,
                "normalized_to_raw": list(range(len(text))),
            }
            location = {
                **args[4][0]["locations"][0],
                "block_id": block["id"],
                "raw_end": len(text),
                "normalized_end": len(text),
                "chunk_end": len(text),
            }
            chunk = {**copy.deepcopy(args[4][0]), "id": f"k{i + 1}", "text": text, "locations": [location]}
            db.execute(
                "INSERT INTO blocks VALUES (?,?,?,?,?,?)", (block["id"], 1, text, text, 1, json.dumps(block))
            )
            # Only block lookup and verified source mappings are relevant to this reranking test.
            records.append((chunk, block))
        db.execute("CREATE TABLE test_chunks (id TEXT, data TEXT)")
        db.executemany("INSERT INTO test_chunks VALUES (?,?)", [(c["id"], json.dumps(c)) for c, _ in records])
    return library, folder, records


def test_top5_reranks_before_cutoff_and_deduplicates_source_text(desktop):
    library, folder, records = candidates(desktop)
    with SearchIndex(folder) as index:
        real_db = index.db

        class Lookup:
            def execute(self, sql, *args):
                if sql == "SELECT data FROM chunks WHERE id=?" and args[0][0] != "k1":
                    sql = sql.replace("chunks", "test_chunks")
                return real_db.execute(sql, *args)

        index.db = Lookup()

        def search(query, **kwargs):
            assert kwargs == {"top_k": 24, "mode": "keyword", "chapter": "c1"}
            return {"results": [{"chunk_id": c["id"], "chapter_path": ["Memory"]} for c, _ in records]}

        index.search = search
        packet = assemble(
            index, library.published("test")[1], "test", "存储器", "c1", mode="keyword", reranker=Scorer()
        )
        repeated = assemble(
            index, library.published("test")[1], "test", "存储器", "c1", mode="keyword", reranker=Scorer()
        )
        index.db = real_db
    assert packet == repeated
    assert [e["relevance_score"] for e in packet["evidence"]] == [16, 15, 14, 13, 12]
    assert len({e["text"] for e in packet["evidence"]}) == 5
    assert [e["id"] for e in packet["evidence"]] == ["E1", "E2", "E3", "E4", "E5"]
    assert packet["ranking"]["candidate_blocks"] > 12
    assert packet["text_chars"] == sum(len(e["text"]) for e in packet["evidence"])
    for source in packet["evidence"]:
        block = next(b for _, b in records if b["id"] == source["block_id"])
        assert all(source[k] == block[k] for k in ["page", "bbox", "text", "raw_text"])


def test_empty_and_small_results_are_not_padded_and_bad_scores_fail(desktop):
    library, root, _ = desktop
    with SearchIndex(root / "versions/version1") as index:
        manifest = library.published("test")[1]
        packet = assemble(index, manifest, "test", "存储器", "c1", "keyword", reranker=Scorer())
        assert len(packet["evidence"]) == 1
        packet = assemble(index, manifest, "test", "香蕉", "c1", "keyword", reranker=Scorer())
        assert packet["evidence"] == []
        for scores in ([float("nan")], [], [float("inf")]):
            bad = SimpleNamespace(manifest=Scorer.manifest, score=lambda *_, value=scores: value)
            with pytest.raises(ValueError, match="重排结果无效"):
                assemble(index, manifest, "test", "存储器", "c1", "keyword", reranker=bad)
        with pytest.raises(ValueError, match="Chapter"):
            assemble(index, manifest, "test", "存储器", "other", "keyword", reranker=Scorer())


def test_related_sources_dispatch_pins_old_version_and_checks_integrity(desktop, monkeypatch):
    library, root, _ = desktop
    path = root / "versions/version1/manifest.json"
    manifest = read_json(path)
    manifest["models"]["embedding"] = {"files": {}}
    atomic_json(path, manifest)
    atomic_json(root / "active.json", {"version": "new-version"})
    monkeypatch.setattr("tellwhy_kb.indexing.reranker.Reranker", lambda _: Scorer())
    vector = np.zeros(512, dtype=np.float32)
    vector[0] = 1
    monkeypatch.setattr(
        "tellwhy_kb.indexing.embedding.Encoder", lambda _: SimpleNamespace(query=lambda _: vector)
    )
    request = {
        "op": "related_sources",
        "kb": "test",
        "version": "version1",
        "query": "存储器",
        "chapter": "c1",
    }
    result = library.dispatch(request)
    assert result["version"] == "version1"
    assert len(result["evidence"]) == 1
    for field, value in [
        ("kb", "../test"),
        ("version", "../version1"),
        ("query", ""),
        ("chapter", "missing"),
    ]:
        with pytest.raises(ValueError):
            library.dispatch({**request, field: value})
    (root / "versions/version1/embeddings.npy").write_bytes(b"corrupt")
    with pytest.raises(ValueError, match="索引校验失败"):
        library.dispatch(request)


def test_reranker_model_manifest_is_pinned_and_hash_checked(tmp_path):
    with pytest.raises(ValueError, match="未准备"):
        verify(tmp_path)
    folder = tmp_path / "reranker"
    folder.mkdir()
    for name in FILES:
        (folder / name).write_bytes(b"fixture")
    manifest = {
        "repository": REPOSITORY,
        "revision": REVISION,
        "files": {name: file_hash(folder / name) for name in FILES},
    }
    atomic_json(folder / "reranker-manifest.json", manifest)
    assert verify(tmp_path) == manifest
    (folder / "model.safetensors").write_bytes(b"corrupted")
    with pytest.raises(ValueError, match="校验失败"):
        verify(tmp_path)
    atomic_json(folder / "reranker-manifest.json", {**manifest, "revision": "unapproved"})
    with pytest.raises(ValueError, match="版本不匹配"):
        verify(tmp_path)


def test_long_source_scoring_sees_the_tail_and_preserves_model_input_limit():
    import torch

    sizes = []

    class Tokenizer:
        bos_token_id = 0
        eos_token_id = 0

        def encode(self, text, **_):
            return [ord(c) for c in text]

        def num_special_tokens_to_add(self, **_):
            return 4

        def pad(self, batch, **_):
            sizes.extend(len(x["input_ids"]) for x in batch)
            width = max(len(x["input_ids"]) for x in batch)
            return {
                "input_ids": torch.tensor(
                    [x["input_ids"] + [0] * (width - len(x["input_ids"])) for x in batch]
                )
            }

    ranker = Reranker.__new__(Reranker)
    ranker.tokenizer = Tokenizer()
    ranker.model = lambda input_ids: SimpleNamespace(logits=(input_ids == ord("z")).any(dim=1).float())
    assert ranker.score("q" * 240, ["a" * 1100 + "z", "a" * 1200]) == [1, 0]
    # Every question window must see the entire passage, including its beginning.
    ranker.model = lambda input_ids: SimpleNamespace(
        logits=((input_ids == ord("Z")).any(dim=1) & (input_ids == ord("A")).any(dim=1)).float()
    )
    assert ranker.score("q" * 240 + "Z", ["A" + "a" * 1100, "a" * 1200]) == [1, 0]
    assert max(sizes) <= 512
    assert set(sum(windows(list(range(1100)), 316), [])) == set(range(1100))


def test_token_window_pairs_match_the_installed_xlm_roberta_tokenizer():
    import torch
    from transformers import XLMRobertaTokenizer

    tokenizer = XLMRobertaTokenizer(
        vocab=[
            ("<s>", 0.0),
            ("<pad>", 0.0),
            ("</s>", 0.0),
            ("<unk>", 0.0),
            ("▁", -1.0),
            ("q", -1.0),
            ("a", -1.0),
        ]
    )
    expected = tokenizer("q q", "a a", return_tensors="pt")
    actual = []
    ranker = Reranker.__new__(Reranker)
    ranker.tokenizer = tokenizer

    def model(**inputs):
        actual.append(inputs)
        return SimpleNamespace(logits=torch.tensor([0.0]))

    ranker.model = model
    assert ranker.score("q q", ["a a"]) == [0.0]
    assert torch.equal(actual[0]["input_ids"], expected["input_ids"])
    assert torch.equal(actual[0]["attention_mask"], expected["attention_mask"])
