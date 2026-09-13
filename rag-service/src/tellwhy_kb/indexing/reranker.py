"""Local Chinese/English cross-encoder; model acquisition is an explicit command."""

from __future__ import annotations

import math
from pathlib import Path

from ..models import local_environment
from ..util import atomic_json, file_hash, read_json

REPOSITORY = "BAAI/bge-reranker-base"
REVISION = "2cfc18c9415c912f9d8155881c133215df768a70"
FILES = (
    "config.json",
    "model.safetensors",
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "sentencepiece.bpe.model",
)


def prepare(models_root: Path) -> dict:
    local_environment(models_root)
    from huggingface_hub import snapshot_download

    folder = models_root / "reranker"
    snapshot_download(
        REPOSITORY,
        revision=REVISION,
        local_dir=folder,
        allow_patterns=list(FILES),
        token=False,
    )
    manifest = {
        "repository": REPOSITORY,
        "revision": REVISION,
        "files": {name: file_hash(folder / name) for name in FILES},
    }
    # Separate from the ingestion manifest: adding a reranker never invalidates an existing PDF index.
    atomic_json(folder / "reranker-manifest.json", manifest)
    return manifest


def verify(models_root: Path) -> dict:
    folder = models_root / "reranker"
    path = folder / "reranker-manifest.json"
    if not path.is_file():
        raise ValueError("本地原文重排模型未准备，请按 README 安装 BGE Reranker 后重试")
    manifest = read_json(path)
    if (
        manifest.get("repository") != REPOSITORY
        or manifest.get("revision") != REVISION
        or set(manifest.get("files", {})) != set(FILES)
    ):
        raise ValueError("原文重排模型版本不匹配，请重新准备模型")
    for name in FILES:
        file = (folder / name).resolve()
        if (
            not file.is_relative_to(folder.resolve())
            or not file.is_file()
            or file_hash(file) != manifest["files"][name]
        ):
            raise ValueError(f"原文重排模型校验失败：{name}")
    return manifest


def windows(tokens: list[int], size: int, overlap: int = 64):
    """Score long source blocks in overlapping windows without dropping their ending."""
    for start in range(0, len(tokens), size - overlap):
        yield tokens[start : start + size]
        if start + size >= len(tokens):
            break


class Reranker:
    def __init__(self, models_root: Path):
        self.manifest = verify(models_root)
        local_environment(models_root)
        import torch
        from transformers import AutoModelForSequenceClassification, AutoTokenizer

        torch.set_num_threads(4)
        folder = str(models_root / "reranker")
        self.tokenizer = AutoTokenizer.from_pretrained(folder, local_files_only=True, trust_remote_code=False)
        self.model = AutoModelForSequenceClassification.from_pretrained(
            folder,
            local_files_only=True,
            trust_remote_code=False,
            use_safetensors=True,
        ).eval()
        if (
            self.model.config.model_type != "xlm-roberta"
            or self.tokenizer.num_special_tokens_to_add(pair=True) != 4
        ):
            raise ValueError("重排模型的文本配对格式不匹配")

    def score(self, query: str, texts: list[str]) -> list[float]:
        import torch

        queries = list(windows(self.tokenizer.encode(query, add_special_tokens=False), 192))
        if not queries:
            raise ValueError("重排问题不能为空")
        scores = [-math.inf] * len(texts)
        batch, owners = [], []

        def flush():
            if not batch:
                return
            inputs = self.tokenizer.pad(batch, padding=True, return_tensors="pt")
            with torch.inference_mode():
                logits = self.model(**inputs).logits.reshape(-1).float().tolist()
            for owner, logit in zip(owners, logits, strict=True):
                if not math.isfinite(logit):
                    raise ValueError("重排模型返回了无效分数")
                scores[owner] = max(scores[owner], logit)
            batch.clear()
            owners.clear()

        special = self.tokenizer.num_special_tokens_to_add(pair=True)
        for owner, text in enumerate(texts):
            tokens = self.tokenizer.encode(text, add_special_tokens=False)
            if not tokens:
                raise ValueError("重排原文不能为空")
            for question in queries:
                for passage in windows(tokens, 512 - special - len(question)):
                    # Pinned XLM-R pair format: <s> question </s></s> passage </s>.
                    # Build from token windows directly; decoding/re-encoding can lose boundary tokens.
                    pair_tokens = [
                        self.tokenizer.bos_token_id,
                        *question,
                        self.tokenizer.eos_token_id,
                        self.tokenizer.eos_token_id,
                        *passage,
                        self.tokenizer.eos_token_id,
                    ]
                    batch.append({"input_ids": pair_tokens})
                    owners.append(owner)
                    if len(batch) == 8:
                        flush()
        flush()
        return scores


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Prepare or verify the local source reranker")
    parser.add_argument("action", choices=["prepare", "verify"])
    parser.add_argument("--models-root", type=Path, default=Path("data/models"))
    args = parser.parse_args()
    result = (prepare if args.action == "prepare" else verify)(args.models_root.resolve())
    print(f"{result['repository']} @ {result['revision']}: ready")
