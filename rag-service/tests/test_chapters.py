import pytest
from tellwhy_kb.processing.chapters import build_chapters, chapter_for, heading_text
from tellwhy_kb.processing.normalize import normalize_text, normalize_blocks
from tellwhy_kb.schemas import PageResult, Block, Box


def test_missing_part_destination_and_mid_page_section_boundary():
    blocks = [
        Block(
            id=str(i),
            page=5,
            order=i,
            kind="paragraph_title" if i else "text",
            text=t,
            bbox=Box(x0=1, y0=10 + i * 20, x1=90, y1=20 + i * 20),
        )
        for i, t in enumerate(["前一节的结束内容。", "1.2 新节", "1.2.1 新小节"])
    ]
    pages = [PageResult(number=5, width=100, height=100, status="success", method="ocr", blocks=blocks)]
    probe = {
        "sha256": "abc",
        "pages": 10,
        "bookmarks": [
            {"title": "第1篇 概论", "page": None, "depth": 0},
            {"title": "第1章 基础", "page": 1, "depth": 0},
            {"title": "1.1 原节", "page": 1, "depth": 1},
            {"title": "1.2 新节", "page": 5, "depth": 1},
        ],
    }
    nodes = build_chapters(probe, pages)
    assert nodes[0]["start"] == [1, 0]
    assert chapter_for(nodes, 5, 0)["title"] == "1.1 原节"
    assert chapter_for(nodes, 5, 1)["title"] == "1.2 新节"
    assert chapter_for(nodes, 5, 2)["title"] == "1.2.1新小节"
    assert chapter_for(nodes, 5, 2)["path"][0] == "第1篇 概论"
    with pytest.raises(ValueError, match="different PDF"):
        build_chapters(probe, pages, {"source_sha256": "wrong"})


def test_nonmonotonic_bookmark_is_not_silently_used():
    probe = {
        "sha256": "abc",
        "pages": 10,
        "bookmarks": [
            {"title": "第1章 基础", "page": 1, "depth": 0},
            {"title": "1.1 原节", "page": 5, "depth": 1},
            {"title": "1.2 错节", "page": 3, "depth": 1},
        ],
    }
    nodes = build_chapters(probe, [])
    assert next(n for n in nodes if n["title"] == "1.2 错节")["start"] is None


def test_normalization_preserves_numeric_semantics_and_source_mapping():
    raw = "地\n址总线 2²⁰ −1\nCPU Cache 1 0"
    text, mapping = normalize_text(raw)
    assert text == "地址总线2²⁰−1 CPU Cache 1 0"
    assert len(text) == len(mapping)
    assert mapping == sorted(mapping)
    assert all(c == raw[i] or (c == " " and raw[i].isspace()) for c, i in zip(text, mapping, strict=True))


def test_heading_number_baseline_does_not_break_matching():
    assert heading_text("控制单元的设计\n第10章") == "第10章控制单元的设计"
    assert heading_text("组合逻辑设计\n10.1") == "10.1组合逻辑设计"


def test_unresolved_destination_keeps_original_chapter_parent():
    probe = {
        "sha256": "abc",
        "pages": 20,
        "bookmarks": [
            {"title": "第1章 基础", "page": 1, "depth": 0},
            {"title": "1.1 缺少目标", "page": None, "depth": 1},
            {"title": "第2章 应用", "page": 10, "depth": 0},
        ],
    }
    nodes = build_chapters(probe, [])
    unresolved = next(n for n in nodes if n["start"] is None)
    assert unresolved["parent_id"] == "b0000"
    assert unresolved["path"] == ["第1章 基础", "1.1 缺少目标"]


@pytest.mark.parametrize("excluded_title", ["思考题与习题", "参考文献"])
def test_numbered_question_cannot_reopen_excluded_content(excluded_title):
    probe = {
        "sha256": "abc",
        "pages": 8,
        "bookmarks": [
            {"title": "第4章 存储器", "page": 1, "depth": 0},
            {"title": excluded_title, "page": 5, "depth": 1},
            {"title": "第5章 输入输出", "page": 8, "depth": 0},
        ],
    }
    blocks = [
        Block(
            id=f"q{i}",
            page=6,
            order=i,
            kind=kind,
            text=text,
            eligible=True,
            bbox=Box(x0=5, y0=20 + i * 20, x1=95, y1=30 + i * 20),
        )
        for i, (kind, text) in enumerate(
            [
                ("paragraph_title", "4.4 说明存取周期和存取时间的区别。"),
                ("text", "4.5 什么是存储器的带宽？"),
            ]
        )
    ]
    pages = [PageResult(number=6, width=100, height=100, status="success", method="ocr", blocks=blocks)]
    nodes = build_chapters(probe, pages)
    assert len(nodes) == 3
    assert chapter_for(nodes, 6, 1)["title"] == excluded_title
    normalized = normalize_blocks(pages, nodes)
    assert all(not b["eligible"] for b in normalized)
    assert all(b["content_kind"] in {"exercise", "reference"} for b in normalized)
