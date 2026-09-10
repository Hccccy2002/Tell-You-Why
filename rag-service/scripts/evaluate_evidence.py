"""Offline textbook evidence evaluation; never calls a generation provider.

Usage: python rag-service/scripts/evaluate_evidence.py --data data --output tmp/rag-reports/evidence-final.json
"""
import argparse
import json
import unicodedata
from pathlib import Path

from tellwhy_kb.desktop import DesktopLibrary
from tellwhy_kb.evidence import assemble
from tellwhy_kb.indexing.embedding import Encoder
from tellwhy_kb.search import SearchIndex


def normalized(text):
    return "".join(c for c in text if not c.isspace() and not unicodedata.category(c).startswith("P"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library = DesktopLibrary(args.data, args.data / "models")
    folder, manifest = library.published("computer-organization", verify_data=True)
    labels = json.loads((Path(__file__).parents[1] / "tests/fixtures/retrieval-cases.json").read_text(encoding="utf-8"))
    if manifest["source"]["sha256"] != labels["source_sha256"]:
        raise ValueError("Evaluation annotations belong to a different PDF")
    reports = []
    with SearchIndex(folder, Encoder(args.data / "models")) as index:
        chapters = [json.loads(r[0]) for r in index.db.execute("SELECT data FROM chapters ORDER BY rowid")]
        roots = [c for c in chapters if c.get("kind") == "chapter"]
        blocks = [json.loads(r[0]) for r in index.db.execute("SELECT data FROM blocks ORDER BY page,rowid")]
        cases = list(labels["cases"])
        for i, case in enumerate(cases[:10]):
            matching = [c for c in roots if c.get("start") and c["start"][0] <= case["page"]]
            chapter = matching[-1]["id"]
            wrong = next(c["id"] for c in roots if c["id"] != chapter)
            cases.append({**case, "id": f"scope-positive-{i+1}", "split": "scope", "chapter": chapter})
            cases.append({"id": f"scope-isolation-{i+1}", "split": "isolation", "query": case["query"], "chapter": wrong})
        for case in cases:
            packet = assemble(index, manifest, "computer-organization", case["query"], case.get("chapter"))
            record = {"case": case, "packet": packet}
            assert len({e["block_id"] for e in packet["evidence"]}) == len(packet["evidence"])
            assert packet["text_chars"] <= 10000
            allowed = {case.get("chapter")}
            if case.get("chapter"):
                for _ in chapters:
                    expanded = allowed | {c["id"] for c in chapters if c.get("parent_id") in allowed}
                    if expanded == allowed:
                        break
                    allowed = expanded
                ids = {b["id"] for b in blocks if b.get("section_id") in allowed}
                record["scope_valid"] = all(e["block_id"] in ids for e in packet["evidence"])
                assert record["scope_valid"]
            if "quote" in case:
                quote = normalized(case["quote"])
                record["hit"] = any(e["page"] == case["page"] and quote in normalized(e["text"]) for e in packet["evidence"])
                record["eligible"] = any(b["eligible"] and b.get("kind") not in {"paragraph_title", "doc_title"}
                                         and b["page"] == case["page"] and quote in normalized(b["text"]) for b in blocks)
            reports.append(record)
    metrics = {}
    for split in ["development", "held_out", "scope"]:
        group = [r for r in reports if r["case"]["split"] == split]
        metrics[split] = {"all": len(group), "hit": sum(r["hit"] for r in group),
                          "eligible": sum(r["eligible"] for r in group),
                          "eligible_hit": sum(r["eligible"] and r["hit"] for r in group)}
    result = {"source_sha256": manifest["source"]["sha256"], "version": manifest["version"],
              "count": len(reports), "metrics": metrics, "records": reports,
              "note": "Evidence coverage only. Unsupported probes require separate real-model assessment. Held-out cases are now regression data after fixing appendix eligibility."}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"count": len(reports), "metrics": metrics, "scope_checks": sum("scope_valid" in r for r in reports)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
