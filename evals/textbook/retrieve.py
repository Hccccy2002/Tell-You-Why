"""Offline evaluation against the published PDF index, with no model API calls."""

import os

import argparse
import hashlib
import json
import time
from pathlib import Path

from dataset import load_source, validate
from metrics import coverage, summarize


def run(dataset, data, splits, modes, progress=None, case_ids=None):
    from tellwhy_kb.evidence import assemble
    from tellwhy_kb.indexing.embedding import Encoder
    from tellwhy_kb.search import SearchIndex

    folder, manifest, blocks = load_source(dataset, data)
    validation = validate(dataset, blocks)
    models = json.loads(
        (
            Path(os.environ.get("TELLWHY_KB_MODELS", data / "models"))
            / "model-manifest.json"
        ).read_text(encoding="utf-8")
    )
    if models["embedding"] != manifest["models"]["embedding"]:
        raise ValueError("Query embedding model differs from the published index")
    embedding_root = (
        Path(os.environ.get("TELLWHY_KB_MODELS", data / "models")) / "embedding"
    ).resolve()
    for relative, expected in models["embedding"]["files"].items():
        path = (embedding_root / relative).resolve()
        if (
            not path.is_relative_to(embedding_root)
            or hashlib.sha256(path.read_bytes()).hexdigest() != expected
        ):
            raise ValueError("Query embedding model resource changed")
    if (
        hashlib.sha256((folder / "embeddings.npy").read_bytes()).hexdigest()
        != manifest["files"]["embeddings.npy"]
    ):
        raise ValueError("Embedding index checksum mismatch")
    started = time.perf_counter()
    encoder = (
        Encoder(Path(os.environ.get("TELLWHY_KB_MODELS", data / "models")))
        if set(modes) != {"keyword"}
        else None
    )
    warmup_ms = (time.perf_counter() - started) * 1000
    records = []
    cases = [
        c
        for c in dataset["cases"]
        if c["split"] in splits and (case_ids is None or c["id"] in case_ids)
    ]
    if case_ids is not None and {c["id"] for c in cases} != set(case_ids):
        raise ValueError("Unknown evaluation case IDs")
    with SearchIndex(folder, encoder) as index:
        for position, case in enumerate(cases):
            if progress:
                progress(position, len(cases), case["id"])
            for mode in modes:
                started = time.perf_counter()
                ranked = index.search(case["question"], top_k=5, mode=mode)
                search_ms = (time.perf_counter() - started) * 1000
                started = time.perf_counter()
                packet = assemble(
                    index, manifest, dataset["kb"], case["question"], None, mode=mode
                )
                records.append(
                    {
                        "id": case["id"],
                        "split": case["split"],
                        "mode": mode,
                        "search_ms": search_ms,
                        "packet_ms": (time.perf_counter() - started) * 1000,
                        "metrics": coverage(case, ranked["results"], packet),
                        "ranking": ranked,
                        "packet": packet,
                    }
                )
            if progress:
                progress(position + 1, len(cases), case["id"])
    return {
        "schema_version": 1,
        **validation,
        "knowledge_version": dataset["knowledge_version"],
        "source_sha256": dataset["source_sha256"],
        "embedding_index_sha256": manifest["files"]["embeddings.npy"],
        "embedding_model_manifest_sha256": hashlib.sha256(
            (
                Path(os.environ.get("TELLWHY_KB_MODELS", data / "models"))
                / "model-manifest.json"
            ).read_bytes()
        ).hexdigest(),
        "warmup_ms": warmup_ms,
        "metrics": summarize(records),
        "records": records,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dataset", type=Path, default=Path(__file__).with_name("dataset.json")
    )
    parser.add_argument("--data", type=Path, default=Path("data"))
    parser.add_argument(
        "--splits",
        nargs="+",
        choices=["development", "regression", "validation"],
        default=["development", "regression"],
    )
    parser.add_argument(
        "--modes",
        nargs="+",
        choices=["keyword", "dense", "hybrid"],
        default=["keyword", "dense", "hybrid"],
    )
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise ValueError(
            "Refusing to overwrite an experiment; select a new output path"
        )
    dataset = json.loads(args.dataset.read_text(encoding="utf-8"))
    report = run(dataset, args.data, args.splits, args.modes)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report["metrics"]))


if __name__ == "__main__":
    main()
