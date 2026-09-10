"""Offline extraction of licensed textbook sections with exact source provenance."""

from collections import Counter
import hashlib
import html
import json
from pathlib import Path
import posixpath
import re
import urllib.parse

ROOT = Path(__file__).resolve().parent
BT = chr(96)
BOOKS = {
    "archbase": {
        "repo": "foxsen/archbase", "title": "计算机体系结构基础（第三版）",
        "authors": ["胡伟武", "汪文祥", "苏孟豪", "张福新", "王焕东", "章隆兵", "肖俊华", "刘苏", "陈新科", "吴瑞阳", "李晓钰", "高燕萍"],
        "license": "CC-BY-NC-4.0", "license_url": "https://creativecommons.org/licenses/by-nc/4.0/",
    },
    "hello-algo": {
        "repo": "krahets/hello-algo", "title": "Hello 算法",
        "authors": ["krahets", "Hello 算法贡献者"],
        "license": "CC-BY-NC-SA-4.0", "license_url": "https://creativecommons.org/licenses/by-nc-sa/4.0/",
    },
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def clean_heading(text):
    return re.sub(r"\s*\{[^}]*\}\s*$", "", text).strip().rstrip(" *")


def headings(lines):
    result, fence = [], None
    for i, line in enumerate(lines):
        marker = re.match(r"^\s*(" + BT + r"{3,}|~{3,})", line)
        if marker:
            kind = marker.group(1)[0]
            if fence is None:
                fence = kind
            elif kind == fence:
                fence = None
            continue
        match = re.match(r"^(#{1,6})\s+(.+)$", line)
        if match and not fence:
            result.append((i, len(match.group(1)), clean_heading(match.group(2))))
    return result


def select_section(lines, name):
    hs = headings(lines)
    if name is None:
        return 0, len(lines), [hs[0][2]]
    matches = [h for h in hs if h[2] == name]
    if len(matches) != 1:
        raise ValueError(f"Expected one section named {name!r}: {matches}")
    start, level, _ = matches[0]
    end = next((i for i, depth, title in hs if i > start and depth <= level), len(lines))
    ancestry = []
    for i, depth, title in hs:
        if i > start:
            break
        while ancestry and ancestry[-1][0] >= depth:
            ancestry.pop()
        if not title.startswith("(PART)"):
            ancestry.append((depth, title))
    return start, end, [title for depth, title in ancestry]


def normalize(lines, source_start, repo, commit, source_path):
    output, mapping, omitted = [], [], []
    i = 0
    base_level = len(re.match(r"^(#+)", lines[0]).group(1))

    def emit(text, line_start, line_end=None):
        output.append(text)
        mapping.append({"line": len(output), "source_line_start": line_start, "source_line_end": line_end or line_start})

    while i < len(lines):
        value = lines[i]
        source_line = source_start + i + 1
        if re.match(r"^\s*" + BT * 3 + r"\{r(?:\s|[,}])", value):
            end = i + 1
            while end < len(lines) and lines[end].strip() != BT * 3:
                end += 1
            caption = re.search(r"fig\.cap\s*=\s*(['\"])(.*?)\1", value)
            description = caption.group(2) if caption else "表格或排版代码块"
            source_end = source_start + min(end + 1, len(lines))
            omitted.append({"type": "r_render_block", "caption": description, "source_line_start": source_line, "source_line_end": source_end})
            emit(f"> [原文图表未展开：{description}。请查看原文；本块不能作为文字证据。]", source_line, source_end)
            i = end + 1
            continue
        if re.match(r"^\s*\{\{.*\}\}\s*$", value):
            omitted.append({"type": "template_macro", "source_line_start": source_line, "source_line_end": source_line})
            emit("> [原文代码示例宏未展开；请查看原文。]", source_line)
            i += 1
            continue
        for image in re.finditer(r"!\[([^\]]*)\]\(([^)]+)\)", value):
            omitted.append({"type": "image", "caption": image.group(1), "original_target": image.group(2), "source_line_start": source_line, "source_line_end": source_line})
        value = re.sub(r"!\[([^\]]*)\]\(([^)]+)\)", lambda m: f"[原文图片未收录：{m.group(1)}；请查看原文。]", value)
        heading = re.match(r"^(#+)\s+(.+)$", value)
        if heading:
            value = "#" * max(1, len(heading.group(1)) - base_level + 1) + " " + clean_heading(heading.group(2))
        value = re.sub(r"\\@ref\(([^)]+)\)", r"（原文交叉引用：\1）", value)
        value = re.sub(r"</?(?:u|span|p|center|div|id)\b[^>]*>", "", value)
        value = html.unescape(value).replace("\u00a0", " ").rstrip()

        def link_target(match):
            label, target = match.groups()
            if target.startswith(("https://", "http://", "#", "mailto:")):
                return match.group(0)
            resolved = posixpath.normpath(posixpath.join(posixpath.dirname(source_path), target))
            return f"[{label}](https://github.com/{repo}/blob/{commit}/{urllib.parse.quote(resolved, safe='/#')})"

        value = re.sub(r"\[([^\]]+)\]\(([^\s)]+)\)", link_target, value)
        emit(value, source_line)
        i += 1
    while output and not output[-1]:
        output.pop()
        mapping.pop()
    return "\n".join(output) + "\n", mapping, omitted


def main():
    selections = json.loads((ROOT / "selection.json").read_text(encoding="utf-8"))
    downloads = json.loads((ROOT / "provenance/downloads.json").read_text(encoding="utf-8"))
    lookup = {(item["repo"], item["path"]): item for item in downloads["files"]}
    for folder in ("documents", "line-maps", "excerpts"):
        (ROOT / folder).mkdir(exist_ok=True)
    docs = []
    for selected in selections:
        book = BOOKS[selected["source"]]
        info = lookup[(book["repo"], selected["file"])]
        raw_bytes = (ROOT / info["local_path"]).read_bytes()
        assert sha256(raw_bytes) == info["sha256"]
        lines = raw_bytes.decode("utf-8-sig").splitlines()
        start, end, ancestry = select_section(lines, selected["section"])
        source_url = info["source_url"] + f"#L{start + 1}-L{end}"
        text, line_map, omitted = normalize(lines[start:end], start, book["repo"], info["commit"], selected["file"])
        title = ancestry[-1]
        filename = f"{selected['id']}-{title.replace('/', '／')}.md"
        doc_path = Path("documents") / filename
        excerpt = "\n".join(lines[start:end]) + "\n"
        excerpt_path = Path("excerpts") / f"{selected['id']}.txt"
        (ROOT / excerpt_path).write_text(excerpt, encoding="utf-8", newline="\n")
        note = selected.get("note", "用于基础原理学习；具体历史产品或版本示例应保留原文适用条件。")
        doc = {
            "id": selected["id"], "title": title, "language": "zh-CN",
            "topic": selected["topic"], "difficulty": selected["difficulty"],
            "source_work": book["title"], "source_id": selected["source"], "authors": book["authors"],
            "repository": f"https://github.com/{book['repo']}", "source_url": source_url,
            "raw_url": info["raw_url"], "source_path": selected["file"], "commit": info["commit"],
            "retrieved_at": downloads["retrieved_at"], "source_published_at": None,
            "section_path": ancestry, "source_line_start": start + 1, "source_line_end": end,
            "license": book["license"], "license_url": book["license_url"],
            "license_snapshot": f"raw/{book['repo'].replace('/', '--')}/LICENSE",
            "raw_snapshot_path": info["local_path"], "raw_sha256": info["sha256"],
            "document_path": doc_path.as_posix(), "excerpt_path": excerpt_path.as_posix(),
            "excerpt_sha256": sha256(excerpt.encode()), "text_sha256": sha256(text.encode()),
            "line_map_path": f"line-maps/{selected['id']}.json", "text_characters": len(text),
            "review_status": "source_and_extraction_checked_not_fact_verified",
            "limitations": note, "omitted_nontext_blocks": omitted,
            "changes": "按自然章节节选；去除 R 排版执行块和图片并保留占位说明；规范标题和少量 HTML；将相对链接指回固定版本；不翻译、不用 AI 改写正文。",
            "text": text,
        }
        header = (
            f"# {title}\n\n> 资料编号：{doc['id']}｜主题：{doc['topic']}｜难度：{doc['difficulty']}\n"
            f"> 作者：{'、'.join(book['authors'])}\n> 来源：[{book['title']}]({source_url})\n"
            f"> 许可：[{book['license']}]({book['license_url']})；原作者署名及许可须随文本保留。\n"
            f"> 固定版本：{info['commit']}；原文第 {start + 1}–{end} 行。\n"
            f"> 整理说明：{doc['changes']}\n> 使用范围：{note}\n"
            "> 本文已核对来源和提取范围，未经逐条事实复核。\n\n---\n\n"
        )
        body = text.split("\n", 1)[1].lstrip("\n")
        (ROOT / doc_path).write_text(header + body, encoding="utf-8", newline="\n")
        (ROOT / doc["line_map_path"]).write_text(json.dumps({"id": doc["id"], "applies_to": "corpus.jsonl text field, excluding document header", "line_numbering": "one-based", "lines": line_map}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
        docs.append(doc)
    (ROOT / "corpus.jsonl").write_text("".join(json.dumps(doc, ensure_ascii=False) + "\n" for doc in docs), encoding="utf-8", newline="\n")
    manifest = [{k: v for k, v in doc.items() if k != "text"} for doc in docs]
    (ROOT / "manifest.json").write_text(json.dumps({"schema_version": 1, "document_count": len(docs), "source_work_count": len(BOOKS), "documents": manifest}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    catalog = ["# 计算机硬件基础资料目录", "", "35 篇中文文本资料，来自 2 部开源教材；其中 30 篇为《计算机体系结构基础》的自然章节，5 篇为《Hello 算法》的文章或自然章节。这些不是 35 个独立作者或独立站点。", "", "点击标题阅读整理文本，点击原文查看固定 Git 版本和行号。资料用于后续检索，尚未生成知识卡、向量或事实核验结论。", "", "| 编号 | 资料 | 主题 | 难度 | 出处 |", "| --- | --- | --- | --- | --- |"]
    for doc in docs:
        catalog.append(f"| {doc['id']} | [{doc['title']}]({urllib.parse.quote(doc['document_path'])}) | {doc['topic']} | {doc['difficulty']} | [{doc['source_work']} · 原文]({doc['source_url']}) |")
    (ROOT / "CATALOG.md").write_text("\n".join(catalog) + "\n", encoding="utf-8", newline="\n")
    report = {
        "documents": len(docs), "source_works": len(BOOKS),
        "by_source": dict(Counter(doc["source_work"] for doc in docs)),
        "by_topic": dict(Counter(doc["topic"] for doc in docs)),
        "by_difficulty": dict(Counter(doc["difficulty"] for doc in docs)),
        "text_characters": sum(doc["text_characters"] for doc in docs),
        "min_document_characters": min(doc["text_characters"] for doc in docs),
        "max_document_characters": max(doc["text_characters"] for doc in docs),
        "documents_with_omitted_nontext": sum(bool(doc["omitted_nontext_blocks"]) for doc in docs),
    }
    (ROOT / "provenance/build-report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
