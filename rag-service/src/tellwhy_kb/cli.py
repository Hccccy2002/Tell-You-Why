from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import yaml

from .build import build_job
from .ingest.probe import inspect_pdf
from .indexing.embedding import Encoder
from .jobs import initialize_job, JobStore, runtime_versions
from .models import prepare_models, verify_models
from .pipeline import extract_pages
from .schemas import IngestConfig
from .search import SearchIndex
from .storage.database import open_readonly
from .storage.manifest import active_version, validate_version
from .util import file_hash, parse_pages, read_json, safe_name


def parser():
    app = argparse.ArgumentParser(
        prog="tellwhy-kb", description="Local PDF preparation and evidence retrieval"
    )
    commands = app.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--data-root", type=Path, default=Path("data"))
    common.add_argument("--models-root", type=Path)
    inspect = commands.add_parser("inspect")
    inspect.add_argument("--pdf", type=Path, required=True)
    models = commands.add_parser("models")
    model_actions = models.add_subparsers(dest="action", required=True)
    for action in ("prepare", "verify"):
        model_actions.add_parser(action, parents=[common])
    for action in ("ingest", "resume"):
        cmd = commands.add_parser(action, parents=[common])
        if action == "ingest":
            cmd.add_argument("--pdf", type=Path, required=True)
            cmd.add_argument("--kb", required=True)
            cmd.add_argument("--config", type=Path)
        else:
            cmd.add_argument("--job", required=True)
            cmd.add_argument("--kb")
        cmd.add_argument("--pages")
        cmd.add_argument("--until", choices=["extract", "structure", "chunks", "publish"], default="publish")
        cmd.add_argument("--workers", type=int, default=1)
        cmd.add_argument("--overrides", type=Path)
        cmd.add_argument("--terms", type=Path)
        cmd.add_argument("--allow-partial", action="store_true")
    for action in ("status", "cancel"):
        cmd = commands.add_parser(action, parents=[common])
        cmd.add_argument("--job", required=True)
        cmd.add_argument("--kb")
    for action in ("validate", "search", "chapters"):
        cmd = commands.add_parser(action, parents=[common])
        cmd.add_argument("--kb", required=True)
        cmd.add_argument("--version")
        if action == "search":
            cmd.add_argument("--query", required=True)
            cmd.add_argument("--top-k", type=int, default=5)
            cmd.add_argument("--mode", choices=["keyword", "dense", "hybrid"], default="hybrid")
            cmd.add_argument("--chapter")
    return app


def find_job(data_root, identifier, kb=None):
    safe_name(identifier)
    root = data_root / "knowledge-bases"
    matches = [root / safe_name(kb) / "work" / identifier] if kb else list(root.glob("*/work/" + identifier))
    matches = [p for p in matches if (p / "job.json").is_file()]
    if len(matches) != 1:
        raise ValueError("Job not found or ambiguous; supply --kb and the job ID")
    return matches[0]


def run(args):
    if args.command == "inspect":
        return inspect_pdf(args.pdf)
    data_root = args.data_root.resolve()
    models_root = (args.models_root or data_root / "models").resolve()
    if args.command == "models":
        return prepare_models(models_root) if args.action == "prepare" else verify_models(models_root)
    if args.command in {"ingest", "resume"}:
        models = verify_models(models_root)
        if args.command == "ingest":
            values = yaml.safe_load(args.config.read_text(encoding="utf-8")) if args.config else {}
            config = IngestConfig.model_validate(values or {})
            folder, meta = initialize_job(
                data_root, safe_name(args.kb), inspect_pdf(args.pdf), config, models
            )
        else:
            folder = find_job(data_root, args.job, args.kb)
            meta = read_json(folder / "job.json")
            if models != meta["models"]:
                raise ValueError("Model resources changed; create a new ingest job instead of resuming")
            if runtime_versions() != meta["runtime"]:
                raise ValueError("Inference runtime changed; create a new ingest job instead of resuming")
        selected = parse_pages(args.pages, meta["probe"]["pages"])
        (folder / "cancel.request").unlink(missing_ok=True)
        counts = extract_pages(folder, models_root, selected, args.workers)
        if (folder / "cancel.request").exists():
            return {"job": meta["id"], "status": "cancelled", "counts": counts}
        if args.until == "extract":
            return {"job": meta["id"], "counts": counts}
        overrides = read_json(args.overrides) if args.overrides else None
        terms = (
            [s.strip() for s in args.terms.read_text(encoding="utf-8").splitlines() if s.strip()]
            if args.terms
            else None
        )
        return build_job(folder, models_root, args.until, overrides, terms, args.allow_partial)
    if args.command in {"status", "cancel"}:
        folder = find_job(data_root, args.job, args.kb)
        if args.command == "cancel":
            (folder / "cancel.request").write_text("cancel requested", encoding="utf-8")
            return {"job": args.job, "status": "cancellation_requested; current pages will finish"}
        with JobStore(folder) as store:
            return {
                "job": args.job,
                "counts": store.counts(),
                "stage": read_json(folder / "stage.json")
                if (folder / "stage.json").exists()
                else "extracting",
            }
    kb_root = data_root / "knowledge-bases" / safe_name(args.kb)
    directory = active_version(kb_root, args.version)
    validation = validate_version(directory)
    if args.command == "validate":
        return validation
    if args.command == "chapters":
        db = open_readonly(directory / "knowledge.sqlite")
        try:
            return [json.loads(r[0]) for r in db.execute("SELECT data FROM chapters")]
        finally:
            db.close()
    manifest = read_json(directory / "manifest.json")
    encoder = None
    if args.mode != "keyword":
        expected = manifest["models"]["embedding"]
        for relative, checksum in expected["files"].items():
            path = (models_root / "embedding" / relative).resolve()
            if not path.is_relative_to((models_root / "embedding").resolve()) or file_hash(path) != checksum:
                raise ValueError("Query embedding model differs from the published index")
        encoder = Encoder(models_root)
    with SearchIndex(directory, encoder) as index:
        result = index.search(args.query, args.top_k, args.mode, args.chapter)
    result.update(
        {
            "kb": args.kb,
            "version": manifest["version"],
            "status": manifest["status"],
            "source_pdf": str(kb_root / manifest["source"]["path"]),
        }
    )
    return result


def main():
    args = parser().parse_args()
    try:
        result = run(args)
        print(json.dumps(result, ensure_ascii=False, allow_nan=False, indent=2))
    except KeyboardInterrupt:
        print(
            json.dumps({"error": "Interrupted; committed page checkpoints can be resumed"}), file=sys.stderr
        )
        raise SystemExit(130)
    except (ValueError, RuntimeError, OSError, KeyError) as exc:
        print(json.dumps({"error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        raise SystemExit(2)
