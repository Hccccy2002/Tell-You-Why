import copy
import pytest
from tokenizers import Tokenizer, models, pre_tokenizers, processors
from transformers import PreTrainedTokenizerFast

from tellwhy_kb.processing.chunking import make_chunks, validate_locations
from tellwhy_kb.schemas import IngestConfig


@pytest.fixture
def tokenizer():
    chars = list(
        dict.fromkeys("计算机系统由硬件和软件组成存储器存放程序数据。地址总线决定寻址范围ABC0123章节")
    )
    vocab = {c: i for i, c in enumerate(["[UNK]", "[CLS]", "[SEP]"] + chars)}
    backend = Tokenizer(models.WordPiece(vocab, unk_token="[UNK]"))
    backend.pre_tokenizer = pre_tokenizers.BertPreTokenizer()
    from tokenizers import normalizers

    backend.normalizer = normalizers.BertNormalizer(lowercase=False)
    backend.post_processor = processors.TemplateProcessing(
        single="[CLS] $A [SEP]", special_tokens=[("[CLS]", 1), ("[SEP]", 2)]
    )
    return PreTrainedTokenizerFast(
        tokenizer_object=backend, unk_token="[UNK]", cls_token="[CLS]", sep_token="[SEP]"
    )


def block(id, page, text, section="s1", eligible=True):
    return {
        "id": id,
        "page": page,
        "kind": "text",
        "text": text,
        "raw_text": text,
        "normalized_to_raw": list(range(len(text))),
        "eligible": eligible,
        "chapter_id": "c1",
        "section_id": section,
        "content_kind": "body",
        "chapter_path": ["章节"],
        "bbox": {"x0": 1, "y0": 2, "x1": 90, "y1": 40},
    }


def test_chunks_cover_long_text_with_overlap_and_exact_source_ranges(tokenizer):
    text = "计算机系统由硬件和软件组成。" * 90
    blocks = [block("b1", 1, text)]
    config = IngestConfig(chunk_tokens=100, overlap_tokens=20)
    chunks = make_chunks(blocks, tokenizer, config, "test", "sha", "version1")
    assert len(chunks) > 5 and max(c["tokens"] for c in chunks) <= 512
    validate_locations(chunks, blocks)
    covered = set()
    for c in chunks:
        for loc in c["locations"]:
            covered.update(range(loc["normalized_start"], loc["normalized_end"]))
    assert covered == set(range(len(text)))
    assert chunks == make_chunks(blocks, tokenizer, config, "test", "sha", "version1")
    broken = copy.deepcopy(chunks)
    broken[0]["locations"][0]["raw_end"] += 1
    with pytest.raises(ValueError, match="Raw source"):
        validate_locations(broken, blocks)


def test_cross_page_provenance_and_exclusion_boundaries(tokenizer):
    blocks = [
        block("b1", 1, "计算机系统由硬件"),
        block("b2", 2, "和软件组成。"),
        block("bad", 2, "地址总线", eligible=False),
        block("b3", 2, "存储器存放数据。"),
        block("b4", 4, "程序数据。"),
        block("b5", 4, "地址总线决定寻址范围。", section="s2"),
    ]
    chunks = make_chunks(blocks, tokenizer, IngestConfig(), "test", "sha", "v1")
    assert len(chunks) == 4
    assert {loc["page"] for loc in chunks[0]["locations"]} == {1, 2}
    assert chunks[0]["text"] == "计算机系统由硬件和软件组成。"
    assert all(loc["block_id"] != "bad" for c in chunks for loc in c["locations"])
    validate_locations(chunks, blocks)
