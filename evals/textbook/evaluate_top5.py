"""Compare five displayed source blocks, at equal budget, with the pinned local reranker."""

import argparse
import json
import os
import time
from pathlib import Path

from dataset import load_source, validate
from metrics import coverage, summarize
from tellwhy_kb.evidence import assemble
from tellwhy_kb.indexing.embedding import Encoder
from tellwhy_kb.indexing.reranker import Reranker
from tellwhy_kb.search import SearchIndex
from tellwhy_kb.util import file_hash


def run(dataset, data, progress=None, case_ids=None):
    folder, manifest, blocks = load_source(dataset, data)
    validation = validate(dataset, blocks)
    model_root = (
        Path(os.environ.get("TELLWHY_KB_MODELS", data / "models")) / "embedding"
    ).resolve()
    for relative, expected in manifest["models"]["embedding"]["files"].items():
        resource = (model_root / relative).resolve()
        if not resource.is_relative_to(model_root) or file_hash(resource) != expected:
            raise ValueError("Query embedding model differs from the frozen index")
    if file_hash(folder / "embeddings.npy") != manifest["files"]["embeddings.npy"]:
        raise ValueError("Embedding index checksum mismatch")
    started = time.perf_counter()
    encoder, reranker = (
        Encoder(Path(os.environ.get("TELLWHY_KB_MODELS", data / "models"))),
        Reranker(Path(os.environ.get("TELLWHY_KB_MODELS", data / "models"))),
    )
    warmup_ms = (time.perf_counter() - started) * 1000
    records = []
    cases = [c for c in dataset["cases"] if case_ids is None or c["id"] in case_ids]
    if not cases or (
        case_ids is not None and {c["id"] for c in cases} != set(case_ids)
    ):
        raise ValueError("Unknown or empty evaluation case IDs")
    with SearchIndex(folder, encoder) as index:
        for case in cases:
            if progress:
                progress(len(records) // 2, len(cases), case["id"])
            for mode in ["baseline_top5", "reranked_top5"]:
                started = time.perf_counter()
                packet = assemble(
                    index,
                    manifest,
                    dataset["kb"],
                    case["question"],
                    None,
                    reranker=reranker if mode == "reranked_top5" else None,
                )
                # Old display order at the SAME five-block budget, never compare five against twelve.
                sources = packet["evidence"][:5]
                ranking = [
                    {"locations": [{"block_id": e["block_id"]}]} for e in sources
                ]
                records.append(
                    {
                        "id": case["id"],
                        "question": case["question"],
                        "split": case["split"],
                        "mode": mode,
                        "search_ms": (time.perf_counter() - started) * 1000,
                        "metrics": coverage(case, ranking, {"evidence": sources}),
                        "sources": sources,
                        "ranking": packet.get("ranking"),
                    }
                )
            print(
                json.dumps(
                    {
                        "case": case["id"],
                        "seconds": round(records[-1]["search_ms"] / 1000, 2),
                        "before": records[-2]["metrics"]["mrr_at_5"],
                        "after": records[-1]["metrics"]["mrr_at_5"],
                    }
                ),
                flush=True,
            )
            if progress:
                progress(len(records) // 2, len(cases), case["id"])
    return {
        "schema_version": 1,
        **validation,
        "knowledge_version": dataset["knowledge_version"],
        "source_sha256": dataset["source_sha256"],
        "embedding_index_sha256": manifest["files"]["embeddings.npy"],
        "reranker_manifest": reranker.manifest,
        "warmup_ms": warmup_ms,
        "note": "Equal five-block display budget. Non-exhaustive source labels measure coverage/rank, not precision or factuality. Previously used splits are regression cohorts, not a new blind holdout.",
        "metrics": summarize(records),
        "records": records,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dataset", type=Path, default=Path(__file__).with_name("dataset.json")
    )
    parser.add_argument("--data", type=Path, default=Path("data"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise ValueError("Select a new output path; previous experiments are immutable")
    report = run(json.loads(args.dataset.read_text(encoding="utf-8")), args.data)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report["metrics"], indent=2))
