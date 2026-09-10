"""Repeatable, offline case acceptance using fixed visual transcriptions and source labels."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from tellwhy_kb.indexing.embedding import Encoder
from tellwhy_kb.quality import comparison_text, snippet_distance
from tellwhy_kb.search import SearchIndex
from tellwhy_kb.storage.database import open_readonly
from tellwhy_kb.storage.manifest import active_version, contained, validate_version
from tellwhy_kb.util import atomic_json, read_json, file_hash


def evaluate(kb_root: Path, models_root: Path, fixtures: Path):
    directory = active_version(kb_root)
    validation = validate_version(directory)
    manifest = read_json(directory / "manifest.json")
    cases = read_json(fixtures / "retrieval-cases.json")
    if cases["source_sha256"] != manifest["source"]["sha256"]:
        raise ValueError("Evaluation labels belong to a different PDF")
    db = open_readonly(directory / "knowledge.sqlite")
    try:
        pages = {r["page"]: json.loads(r["data"]) for r in db.execute("SELECT page,data FROM pages")}
        chunks = [json.loads(r[0]) for r in db.execute("SELECT data FROM chunks")]
        blocks = [json.loads(r[0]) for r in db.execute("SELECT data FROM blocks")]
    finally:
        db.close()
    ocr = []
    for sample in read_json(fixtures / "ocr-gold.json")["samples"]:
        page = pages[sample["page"]]
        text = "\n".join(b["text"] for b in page["blocks"])
        ocr.append(
            {
                "page": sample["page"],
                "ocr": snippet_distance(sample["text"], text),
                "native": snippet_distance(sample["text"], page["native_text"]),
            }
        )
    characters = sum(p["ocr"]["reference_characters"] for p in ocr)
    ocr_cer = sum(p["ocr"]["errors"] for p in ocr) / characters
    native_cer = sum(p["native"]["errors"] for p in ocr) / characters
    for relative, expected in manifest["models"]["embedding"]["files"].items():
        if file_hash(contained(models_root / "embedding", relative)) != expected:
            raise ValueError("Evaluation embedding model differs from the published index")
    encoder = Encoder(models_root)
    results = []
    retrieval_config = None
    with SearchIndex(directory, encoder) as index:
        for case in cases["cases"]:
            started = time.perf_counter()
            hits = index.search(case["query"], 5)
            retrieval_config = hits["fusion"]
            result = {
                **case,
                "elapsed_seconds": time.perf_counter() - started,
                "retrieved_ids": [h["chunk_id"] for h in hits["results"]],
                "retrieved_pages": sorted({loc["page"] for h in hits["results"] for loc in h["locations"]}),
                "answerability": hits["answerability"],
            }
            if case["split"] != "unsupported":
                quote = comparison_text(case["quote"])
                relevant = {
                    c["id"]
                    for c in chunks
                    if any(loc["page"] == case["page"] for loc in c["locations"])
                    and quote in comparison_text(c["text"])
                }
                result["eligible_gold_chunks"] = sorted(relevant)
                result["hit_at_5"] = bool(relevant & set(result["retrieved_ids"]))
                result["evidence_available"] = bool(relevant)
                if not relevant:
                    result["source_diagnostics"] = [
                        {"id": b["id"], "eligible": b["eligible"], "limitations": b["limitations"]}
                        for b in blocks
                        if b["page"] == case["page"] and quote in comparison_text(b["text"])
                    ]
            else:
                result["no_answer_claimed"] = hits["answerability"] == "not_assessed"
                if case["kind"] == "uninterpreted_visual":
                    assets = [
                        b
                        for b in blocks
                        if b["page"] == case["page"] and b.get("asset") and not b["eligible"]
                    ]
                    result["excluded_visual_assets"] = len(assets)
            results.append(result)
    metrics = {}
    for split in ["development", "held_out"]:
        selected = [r for r in results if r["split"] == split]
        available = [r for r in selected if r["evidence_available"]]
        hits = sum(r["hit_at_5"] for r in selected)
        metrics[split] = {
            "hit_count": hits,
            "all_source_questions": len(selected),
            "questions_with_eligible_evidence": len(available),
            "hit_at_5_all_source_questions": hits / len(selected),
            "hit_at_5_eligible_evidence": hits / len(available) if available else 0,
        }
    unsupported = [r for r in results if r["split"] == "unsupported"]
    unsupported_passed = all(
        r["no_answer_claimed"] and (r["kind"] != "uninterpreted_visual" or r["excluded_visual_assets"] > 0)
        for r in unsupported
    )
    passed = (
        ocr_cer <= 0.05
        and max(p["ocr"]["cer"] for p in ocr) <= 0.10
        and all(m["hit_at_5_eligible_evidence"] >= 0.90 for m in metrics.values())
        and unsupported_passed
    )
    report = {
        "validation": validation,
        "status": manifest["status"],
        "fixed_fixture_hashes": {
            p.name: file_hash(p) for p in [fixtures / "ocr-gold.json", fixtures / "retrieval-cases.json"]
        },
        "ocr": {
            "sampled_prose_snippets": len(ocr),
            "reference_characters": characters,
            "weighted_cer": ocr_cer,
            "native_weighted_cer": native_cer,
            "pages": ocr,
            "scope": "Sampled prose, not whole-book character accuracy",
            "comparison_normalization": "Whitespace and Unicode punctuation removed; letters, digits and math symbols retained",
        },
        "retrieval": metrics,
        "retrieval_config": retrieval_config,
        "evidence_scope": "Availability and Hit@5 refer to the annotated physical page and quote; equivalent evidence elsewhere has not been exhaustively labeled",
        "unsupported_contract": {"probes": len(unsupported), "passed": unsupported_passed},
        "cases": results,
        "passed": passed,
        "limits": [
            "Gold questions about excluded content remain visible as coverage gaps.",
            "Unsupported probes test the retrieval-only contract; answer/refusal accuracy is not measured.",
            "Retrieval tests cover a bounded case set and do not prove correctness of all future questions.",
        ],
    }
    atomic_json(kb_root / "reports" / "acceptance.json", report)
    print(
        json.dumps(
            {"passed": passed, "ocr_cer": ocr_cer, "retrieval": metrics, "status": manifest["status"]},
            ensure_ascii=False,
        )
    )
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--kb-root", type=Path, required=True)
    parser.add_argument("--models-root", type=Path, required=True)
    parser.add_argument(
        "--fixtures", type=Path, default=Path(__file__).resolve().parents[1] / "tests/fixtures"
    )
    args = parser.parse_args()
    report = evaluate(args.kb_root, args.models_root, args.fixtures)
    raise SystemExit(0 if report["passed"] else 1)
