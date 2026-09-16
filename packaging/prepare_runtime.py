"""Prepare allowlisted relocatable resources, excluding personal PDFs/databases."""

import importlib.metadata as metadata
import json
import os
from pathlib import Path
import shutil
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "src-tauri/pdf-runtime"
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
from tellwhy_kb.models import verify_models, prepare_models
from tellwhy_kb.indexing.reranker import verify, prepare


def main():
    # Bytecode caches embed build paths and add tens of thousands of unused installer entries.
    python_root = (DEST / "python").resolve()
    for cache in list(python_root.rglob("__pycache__")):
        if cache.is_dir() and cache.resolve().is_relative_to(python_root):
            shutil.rmtree(cache)
    models = ROOT / "data/models"
    if not (models / "model-manifest.json").exists():
        prepare_models(models)
    if not (models / "reranker/reranker-manifest.json").exists():
        prepare(models)
    manifests = verify_models(models)
    reranker = verify(models)
    shutil.copytree(
        ROOT / "rag-service/src/tellwhy_kb",
        DEST / "rag-service/src/tellwhy_kb",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", ".cache"),
    )
    for name in ["requirements.lock.txt", "pyproject.toml", "README.md"]:
        shutil.copy2(ROOT / "rag-service" / name, DEST / "rag-service" / name)
    (DEST / "models").mkdir(exist_ok=True)
    shutil.copy2(models / "model-manifest.json", DEST / "models/model-manifest.json")
    for name, manifest in {**manifests, "reranker": reranker}.items():
        for relative in manifest["files"]:
            source = models / name / relative
            target = DEST / "models" / name / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    shutil.copy2(
        models / "reranker/reranker-manifest.json",
        DEST / "models/reranker/reranker-manifest.json",
    )
    for source in (ROOT / "evals").rglob("*"):
        if (
            source.is_file()
            and source.suffix in {".py", ".json", ".md"}
            and not any(p in {"results", "runs", "__pycache__"} for p in source.parts)
        ):
            target = DEST / "evals" / source.relative_to(ROOT / "evals")
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    shutil.copy2(ROOT / "packaging/check_runtime.py", DEST / "check_runtime.py")
    notices = DEST / "third-party"
    notices.mkdir(exist_ok=True)
    packages = []
    for dist in metadata.distributions():
        packages.append(
            {
                "name": dist.metadata["Name"],
                "version": dist.version,
                "license": dist.metadata.get("License-Expression")
                or dist.metadata.get("License", "See distribution license files"),
            }
        )
    (notices / "python-packages.json").write_text(
        json.dumps(packages, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    sources = {}
    for name, manifest in {**manifests, "reranker": reranker}.items():
        base = f"https://huggingface.co/{manifest['repository']}/resolve/{manifest['revision']}/"
        target = notices / (name + "-MODEL-CARD.md")
        if not target.exists():
            with urllib.request.urlopen(base + "README.md", timeout=90) as response:
                target.write_bytes(response.read())
        sources[name] = {
            "repository": manifest["repository"],
            "revision": manifest["revision"],
            "model_card": target.name,
        }
    for name, url in {
        "Apache-2.0.txt": "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/v3.3.2/LICENSE",
        "BGE-MIT-LICENSE": "https://raw.githubusercontent.com/FlagOpen/FlagEmbedding/master/LICENSE",
    }.items():
        target = notices / name
        if not target.exists():
            with urllib.request.urlopen(url, timeout=90) as response:
                target.write_bytes(response.read())
    (notices / "model-sources.json").write_text(
        json.dumps(sources, indent=2), encoding="utf-8"
    )
    (notices / "README.txt").write_text(
        "Third-party components retain their original licenses.\n"
        "Python: ../python/LICENSE.txt; dependency licenses remain in python/Lib/site-packages.\n"
        "Model cards and revisions are included here (Paddle models: Apache-2.0; BGE: MIT).\n"
        "Python executable manifests are updated by this project to enable a UTF-8 process code page (Windows 10 1903+).\nWebView2 is redistributed by the Windows installer under Microsoft terms.\n",
        encoding="utf-8",
    )
    verify_models(DEST / "models")
    verify(DEST / "models")
    print(
        json.dumps(
            {
                "runtime": str(DEST),
                "python_packages": len(packages),
                "models": list(sources),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
