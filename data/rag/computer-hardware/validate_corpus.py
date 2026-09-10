"""Check extraction/provenance integrity. This does not fact-check the textbook."""

from collections import Counter
import datetime
import hashlib
import json
from pathlib import Path
import re
import sys
import zipfile

ROOT = Path(__file__).resolve().parent


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    downloads = json.loads((ROOT / "provenance/downloads.json").read_text(encoding="utf-8-sig"))
    for source in downloads["files"]:
        data = (ROOT / source["local_path"]).read_bytes()
        assert digest(data) == source["sha256"], source["path"]
        git_sha = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        assert git_sha == source["git_blob_sha1"], source["path"]
        repo_slug = source["repo"].replace("/", "--")
        tree = json.loads((ROOT / "provenance" / f"{repo_slug}-tree.json").read_text(encoding="utf-8-sig"))
        assert any(row["path"] == source["path"] and row["sha"] == git_sha for row in tree["tree"])
    docs = [json.loads(line) for line in (ROOT / "corpus.jsonl").read_text(encoding="utf-8").splitlines()]
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    assert len(docs) == 35 == manifest["document_count"]
    assert len(list((ROOT / "documents").glob("*.md"))) == len(docs)
    assert len({doc["id"] for doc in docs}) == len(docs)
    assert len({doc["text_sha256"] for doc in docs}) == len(docs)
    for doc, meta in zip(docs, manifest["documents"]):
        assert meta == {k: v for k, v in doc.items() if k != "text"}
        assert len(doc["text"]) >= 200, doc["id"]
        assert "\ufffd" not in doc["text"], doc["id"]
        assert digest(doc["text"].encode()) == doc["text_sha256"]
        assert not re.search(chr(96) * 3 + r"\{r|knitr::include_graphics|!\[[^\]]*\]\(|<img\b", doc["text"]), doc["id"]
        assert doc["text"].count(chr(96) * 3) % 2 == 0, doc["id"]
        raw_lines = (ROOT / doc["raw_snapshot_path"]).read_text(encoding="utf-8-sig").splitlines()
        start, end = doc["source_line_start"], doc["source_line_end"]
        assert 1 <= start <= end <= len(raw_lines)
        excerpt = ("\n".join(raw_lines[start - 1:end]) + "\n").encode()
        assert excerpt == (ROOT / doc["excerpt_path"]).read_bytes(), doc["id"]
        assert digest(excerpt) == doc["excerpt_sha256"]
        assert doc["source_url"].endswith(f"#L{start}-L{end}")
        assert len(doc["commit"]) == 40 and doc["commit"] in doc["source_url"]
        license_text = (ROOT / doc["license_snapshot"]).read_text(encoding="utf-8")
        assert "NonCommercial" in license_text
        assert ("ShareAlike" in license_text) == (doc["license"] == "CC-BY-NC-SA-4.0")
        mapping = json.loads((ROOT / doc["line_map_path"]).read_text(encoding="utf-8"))
        assert len(mapping["lines"]) == len(doc["text"].splitlines())
        previous = start
        for index, row in enumerate(mapping["lines"], 1):
            assert row["line"] == index
            assert previous <= row["source_line_start"] <= row["source_line_end"] <= end
            previous = row["source_line_start"]
        readable = (ROOT / doc["document_path"]).read_text(encoding="utf-8")
        assert doc["source_url"] in readable and doc["license"] in readable
        assert doc["review_status"] == "source_and_extraction_checked_not_fact_verified"
    for index, left in enumerate(docs):
        for right in docs[index + 1:]:
            if left["raw_snapshot_path"] == right["raw_snapshot_path"]:
                assert left["source_line_end"] < right["source_line_start"] or right["source_line_end"] < left["source_line_start"], (left["id"], right["id"])
    assert all("同一块内存不能被多个程序共享" not in doc["text"] for doc in docs)
    report = {
        "status": "passed",
        "checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "documents_checked": len(docs),
        "downloaded_source_files_checked": len(downloads["files"]),
        "source_works": len({doc["source_id"] for doc in docs}),
        "exact_duplicate_documents": 0,
        "overlapping_selected_source_ranges": 0,
        "checks": [
            "raw SHA-256 and Git blob SHA-1 match saved source tree",
            "UTF-8 / manifest and corpus consistency",
            "original excerpt bytes and normalized-text hashes",
            "pinned source URLs and valid line ranges",
            "line mappings cover every normalized text line",
            "license snapshots and attribution present",
            "no unresolved R execution blocks or image embeds",
            "selected ranges are disjoint; no exact text duplicates",
            "known overbroad memory-sharing paragraph excluded",
        ],
        "not_checked": [
            "full factual correctness or independent expert review",
            "semantic duplicate detection",
            "image, diagram or OCR interpretation",
            "downstream retrieval or generation quality",
        ],
    }
    (ROOT / "provenance/validation.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    if "--zip" in sys.argv:
        target = ROOT.parent / "computer-hardware-corpus.zip"
        with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(ROOT.rglob("*")):
                if path.is_file() and "__pycache__" not in path.parts and path.suffix != ".log":
                    archive.write(path, Path(ROOT.name) / path.relative_to(ROOT))
        with zipfile.ZipFile(target) as archive:
            assert archive.testzip() is None
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
