from __future__ import annotations

import os

import numpy as np

from ..models import local_environment

QUERY_PREFIX = "为这个句子生成表示以用于检索相关文章："


def validate_matrix(matrix: np.ndarray, rows: int, dimensions: int = 512):
    if matrix.dtype != np.float32 or matrix.shape != (rows, dimensions):
        raise ValueError(f"Invalid embedding shape/dtype: {matrix.shape}/{matrix.dtype}")
    if not np.isfinite(matrix).all():
        raise ValueError("Non-finite embedding values")
    if rows and not np.allclose(np.linalg.norm(matrix, axis=1), 1, atol=0.001):
        raise ValueError("Embeddings must be L2 normalized")


class Encoder:
    def __init__(self, models_root, threads: int = 4):
        local_environment(models_root)
        os.environ["HF_HUB_OFFLINE"] = "1"
        os.environ["TRANSFORMERS_OFFLINE"] = "1"
        import torch

        torch.set_num_threads(threads)
        from sentence_transformers import SentenceTransformer

        self.model = SentenceTransformer(str(models_root / "embedding"), device="cpu", local_files_only=True)
        self.model.max_seq_length = 512
        if self.model.get_embedding_dimension() != 512:
            raise ValueError("Expected the pinned 512-dimensional BGE model")

    def encode(self, texts: list[str], batch_size: int = 16) -> np.ndarray:
        for text in texts:
            if (
                not text.strip()
                or len(self.model.tokenizer(text, truncation=False, verbose=False)["input_ids"]) > 512
            ):
                raise ValueError("Empty or oversized model input; silent truncation is forbidden")
        if not texts:
            return np.empty((0, 512), dtype=np.float32)
        matrix = self.model.encode(
            texts,
            batch_size=batch_size,
            normalize_embeddings=True,
            convert_to_numpy=True,
            show_progress_bar=False,
        ).astype(np.float32)
        validate_matrix(matrix, len(texts))
        return matrix

    def query(self, text: str):
        return self.encode([QUERY_PREFIX + text])[0]
