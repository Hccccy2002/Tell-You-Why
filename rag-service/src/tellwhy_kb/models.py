"""Explicit local model resources; acquisition is a separate, user-run operation."""

from __future__ import annotations

import os
from pathlib import Path

from .util import atomic_json, event, file_hash, read_json

REPOSITORIES = {
    "det": "PaddlePaddle/PP-OCRv5_server_det",
    "rec": "PaddlePaddle/PP-OCRv5_server_rec",
    "layout": "PaddlePaddle/PP-DocLayout-S",
    "embedding": "BAAI/bge-small-zh-v1.5",
}

PINNED_REVISIONS = {
    "det": "ca867c897ecbca8873081573a802ad70d499cb94",
    "rec": "b26c3587fda8da3c8ec0ce357214b4d661ff1558",
    "layout": "8ac289e66575bb9bba6e15c53719d8b15cc9b3b2",
    "embedding": "7999e1d3359715c523056ef9478215996d62a620",
}


def local_environment(root: Path) -> None:
    # All model caches stay in the user-selected data directory.
    os.environ["PADDLE_PDX_CACHE_HOME"] = str(root / "paddlex-cache")
    os.environ["HF_HOME"] = str(root / "hf-cache")
    os.environ["HF_HUB_DISABLE_XET"] = "1"
    os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
    os.environ["TOKENIZERS_PARALLELISM"] = "false"
    os.environ["MODELSCOPE_CACHE"] = str(root / "modelscope-cache")
    os.environ["XDG_CACHE_HOME"] = str(root / "runtime-cache")


def prepare_models(root: Path) -> dict:
    root = root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    local_environment(root)
    from huggingface_hub import snapshot_download

    previous = read_json(root / "model-manifest.json") if (root / "model-manifest.json").exists() else {}
    manifest = dict(previous)
    for name, repository in REPOSITORIES.items():
        # A saved manifest pins the revision on repeated preparation.
        revision = previous.get(name, {}).get("revision") or PINNED_REVISIONS[name]
        event("model_download", model=repository, revision=revision)
        folder = root / name
        patterns = ["*.json", "*.yml", "*.yaml", "*.pdiparams", "*.safetensors", "*.txt"]
        snapshot_download(repository, revision=revision, local_dir=folder, allow_patterns=patterns)
        files = {
            str(p.relative_to(folder)).replace("\\", "/"): file_hash(p)
            for p in sorted(folder.rglob("*"))
            if p.is_file() and ".cache" not in p.parts
        }
        manifest[name] = {"repository": repository, "revision": revision, "files": files}
        atomic_json(root / "model-manifest.json", manifest)
    verify_models(root)
    return manifest


def verify_models(root: Path) -> dict:
    path = root / "model-manifest.json"
    if not path.exists():
        raise ValueError("Local models are missing; run models prepare first")
    manifest = read_json(path)
    for name in REPOSITORIES:
        entry = manifest.get(name)
        if not entry or not entry.get("files"):
            raise ValueError(f"Incomplete model: {name}")
        for relative, expected in entry["files"].items():
            file = (root / name / relative).resolve()
            if not file.is_relative_to((root / name).resolve()):
                raise ValueError("Model manifest path escapes model directory")
            if not file.is_file() or file_hash(file) != expected:
                raise ValueError(f"Model resource changed or missing: {name}/{relative}")
    return manifest
