from __future__ import annotations

import logging
import re

import jieba

DEFAULT_TERMS = [
    "CPU",
    "Cache",
    "SRAM",
    "DRAM",
    "DMA",
    "PCIe",
    "地址总线",
    "数据总线",
    "控制总线",
    "直接存储器访问",
    "微程序",
    "补码",
    "主存储器",
]
STOP_WORDS = set(
    "的 了 是 在 和 与 有 什么 为什么 怎么 如何 哪些 一个 一种 这个 那个 请 解释 介绍 区别".split()
)


class KeywordTokenizer:
    def __init__(self, terms: list[str] | None = None):
        jieba.setLogLevel(logging.ERROR)
        self.tokenizer = jieba.Tokenizer()
        self.terms = terms if terms is not None else DEFAULT_TERMS
        for term in self.terms:
            if term.strip():
                self.tokenizer.add_word(term.strip().lower(), freq=100000)

    def tokens(self, text: str) -> list[str]:
        return [
            t
            for t in self.tokenizer.cut_for_search(text.lower())
            if re.search(r"[\w\u4e00-\u9fff]", t) and t not in STOP_WORDS
        ]

    def index_text(self, text: str) -> str:
        return " ".join(self.tokens(text))

    def match_query(self, text: str) -> str | None:
        terms = list(dict.fromkeys(self.tokens(text)))[:48]
        return " OR ".join('"' + t.replace('"', '""') + '"' for t in terms) if terms else None
