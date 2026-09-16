"""Versioned source labels. Validation does not imply human-reviewed answer accuracy."""

import argparse
import hashlib
import json
import sqlite3
import unicodedata
from pathlib import Path


def digest(value):
    return hashlib.sha256(
        json.dumps(
            value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        ).encode()
    ).hexdigest()


def normalized(text):
    return "".join(
        c
        for c in text
        if not c.isspace() and not unicodedata.category(c).startswith("P")
    )


def validate(dataset, blocks=None, allow_drafts=False):
    if dataset.get("schema_version") not in {1, 2} or not dataset.get(
        "dataset_version"
    ):
        raise ValueError("Unknown dataset schema/version")
    if len(dataset.get("source_sha256", "")) != 64 or not dataset.get(
        "knowledge_version"
    ):
        raise ValueError("Missing source/version pin")
    ids, questions, gaps = set(), set(), []
    for case in dataset["cases"]:
        draft = (
            allow_drafts
            and dataset["schema_version"] == 2
            and case["review"]["method"] == "unreviewed"
        )
        cid = case["id"]
        question = normalized(case["question"])
        if cid in ids or not cid or not question or question in questions:
            raise ValueError("Duplicate or empty case")
        ids.add(cid)
        questions.add(question)
        if case["split"] not in {"development", "validation", "regression", "holdout"}:
            raise ValueError("Unknown evaluation split")
        if case["kind"] not in {
            "single",
            "multi",
            "contrast",
            "absent",
            "unsupported_visual",
            "ocr_error",
            "false_premise",
        }:
            raise ValueError("Unknown question kind")
        if not draft and not case["reference_answer"].strip():
            raise ValueError("Missing reference answer")
        if case["review"]["method"] == "human" and not case["review"].get(
            "human_reviewer"
        ):
            raise ValueError("Human review requires attribution")
        negative = case["kind"] in {"absent", "unsupported_visual"}
        if dataset["schema_version"] == 2:
            if case.get("expected_behavior") not in {
                "answer",
                "refuse",
                "correct_premise",
            }:
                raise ValueError("Missing expected answer behavior")
            if not draft and not case.get("acceptance_criteria", "").strip():
                raise ValueError("Missing acceptable-answer criteria")
            if case["review"]["method"] == "human" and (
                not case["review"].get("date")
                or not case["review"].get("notes", "").strip()
            ):
                raise ValueError("Human labels require date and rationale")
            if not draft and negative and case["expected_behavior"] != "refuse":
                raise ValueError("Unanswerable cases must expect refusal")
            negative = case["expected_behavior"] == "refuse"
        if not draft and (
            (not negative and not case["evidence"])
            or (dataset["schema_version"] == 1 and negative and case["evidence"])
        ):
            raise ValueError("Evidence labels disagree with question kind")
        if not draft and case["kind"] == "multi" and len(case["evidence"]) < 2:
            raise ValueError("Multi-source question needs two source blocks")
        seen = set()
        for evidence in case["evidence"]:
            bid = evidence["block_id"]
            if bid in seen or not 1 <= evidence["page"] <= dataset["source_pages"]:
                raise ValueError("Duplicate evidence or invalid page")
            seen.add(bid)
            if not normalized(evidence["quote"]):
                raise ValueError("Empty evidence quote")
            if blocks is not None:
                block = blocks.get(bid)
                if block is None or block["page"] != evidence["page"]:
                    raise ValueError(f"Invalid evidence location: {cid}/{bid}")
                if normalized(evidence["quote"]) not in normalized(block["text"]):
                    raise ValueError(f"Quote does not occur in source: {cid}/{bid}")
                if (
                    hashlib.sha256(block["text"].encode()).hexdigest()
                    != evidence["text_sha256"]
                ):
                    raise ValueError(f"Source text changed: {cid}/{bid}")
                if not block["eligible"]:
                    gaps.append(
                        {"case_id": cid, "block_id": bid, "page": block["page"]}
                    )
    if not ids:
        raise ValueError("Empty dataset")
    return {
        "dataset_version": dataset["dataset_version"],
        "dataset_sha256": digest(dataset),
        "case_count": len(ids),
        "source_checked": blocks is not None,
        "source_gaps": gaps,
        "human_reviewed": sum(
            c["review"]["method"] == "human" for c in dataset["cases"]
        ),
    }


def load_source(dataset, data_root):
    kb = data_root / "knowledge-bases" / dataset["kb"]
    pointer = json.loads((kb / "active.json").read_text(encoding="utf-8"))
    if pointer["version"] != dataset["knowledge_version"]:
        raise ValueError("Active knowledge version differs from frozen dataset")
    folder = kb / "versions" / pointer["version"]
    manifest_bytes = (folder / "manifest.json").read_bytes()
    if hashlib.sha256(manifest_bytes).hexdigest() != pointer["manifest_sha256"]:
        raise ValueError("Manifest checksum mismatch")
    manifest = json.loads(manifest_bytes)
    if manifest["source"]["sha256"] != dataset["source_sha256"]:
        raise ValueError("Different source PDF")
    source = (kb / manifest["source"]["path"]).resolve()
    if not source.is_relative_to(kb.resolve()):
        raise ValueError("Source path escapes knowledge base")
    if hashlib.sha256(source.read_bytes()).hexdigest() != dataset["source_sha256"]:
        raise ValueError("Source PDF checksum mismatch")
    db_path = folder / "knowledge.sqlite"
    if (
        hashlib.sha256(db_path.read_bytes()).hexdigest()
        != manifest["files"]["knowledge.sqlite"]
    ):
        raise ValueError("Knowledge index checksum mismatch")
    with sqlite3.connect(db_path.resolve().as_uri() + "?mode=ro", uri=True) as db:
        blocks = {
            b["id"]: b
            for (raw,) in db.execute("SELECT data FROM blocks")
            for b in [json.loads(raw)]
        }
    return folder, manifest, blocks


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dataset", type=Path, default=Path(__file__).with_name("dataset.json")
    )
    parser.add_argument("--data", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    dataset = json.loads(args.dataset.read_text(encoding="utf-8"))
    blocks = load_source(dataset, args.data)[2] if args.data else None
    report = validate(dataset, blocks)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
