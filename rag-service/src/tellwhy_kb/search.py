from __future__ import annotations

import json

import numpy as np

from .indexing.embedding import validate_matrix
from .indexing.keyword import KeywordTokenizer
from .storage.database import open_readonly

RRF_RANK_CONSTANT = 60
KEYWORD_WEIGHT = 1.0
DENSE_WEIGHT = 3.0


class SearchIndex:
    """Retrieval only: scores rank passages and do not establish answer sufficiency."""

    def __init__(self, directory, encoder=None):
        self.db = open_readonly(directory / "knowledge.sqlite")
        self.vectors = np.load(directory / "embeddings.npy", mmap_mode="r", allow_pickle=False)
        count = self.db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
        validate_matrix(self.vectors, count)
        settings = json.loads(
            self.db.execute("SELECT data FROM index_metadata WHERE key='keyword'").fetchone()[0]
        )
        self.keywords = KeywordTokenizer(settings["terms"])
        self.encoder = encoder

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.db.close()
        self.vectors._mmap.close()

    def search(self, query: str, top_k: int = 5, mode: str = "hybrid", chapter: str | None = None) -> dict:
        if not query.strip() or not 1 <= top_k <= 100 or mode not in {"keyword", "dense", "hybrid"}:
            raise ValueError("Invalid query, mode or top_k")
        clause, params = "c.eligible=1", []
        if chapter:
            rows = self.db.execute(
                "SELECT id FROM chapters WHERE id=? OR title=?", (chapter, chapter)
            ).fetchall()
            if len(rows) != 1:
                raise ValueError("Chapter must identify exactly one chapter ID or full title")
            clause += """ AND c.section_id IN (
                WITH RECURSIVE selected(id) AS (
                    SELECT id FROM chapters WHERE id=?
                    UNION
                    SELECT child.id FROM chapters child JOIN selected ON child.parent_id=selected.id
                ) SELECT id FROM selected
            )"""
            params.append(rows[0][0])
        lexical, semantic = [], []
        lexical_scores, semantic_scores = {}, {}
        candidate_limit = max(50, top_k * 3)
        if mode in {"keyword", "hybrid"}:
            match = self.keywords.match_query(query)
            if match:
                rows = self.db.execute(
                    f"""SELECT c.id,bm25(chunks_fts) AS rank FROM chunks_fts
                    JOIN chunks c ON c.id=chunks_fts.chunk_id WHERE chunks_fts MATCH ? AND {clause}
                    ORDER BY rank,c.id LIMIT ?""",
                    [match] + params + [candidate_limit],
                ).fetchall()
                lexical = [r[0] for r in rows]
                lexical_scores = {r[0]: float(r[1]) for r in rows}
        if mode in {"dense", "hybrid"}:
            if self.encoder is None:
                raise ValueError("Dense retrieval requires the matching local embedding model")
            vector = self.encoder.query(query)
            rows = self.db.execute(
                f"""SELECT v.row_index,c.id FROM vector_rows v
                JOIN chunks c ON c.id=v.chunk_id WHERE {clause} ORDER BY v.row_index""",
                params,
            ).fetchall()
            if rows:
                indices = np.asarray([r[0] for r in rows], dtype=np.int64)
                scores = self.vectors[indices] @ vector
                ranked = sorted(range(len(rows)), key=lambda i: (-float(scores[i]), rows[i][1]))[
                    :candidate_limit
                ]
                semantic = [rows[i][1] for i in ranked]
                semantic_scores = {rows[i][1]: float(scores[i]) for i in ranked}
        if mode == "hybrid":
            combined = {}
            # Natural-language questions need semantic evidence to survive weak generic keyword matches.
            for weight, ranking in [(KEYWORD_WEIGHT, lexical), (DENSE_WEIGHT, semantic)]:
                for rank, cid in enumerate(ranking, 1):
                    combined[cid] = combined.get(cid, 0) + weight / (RRF_RANK_CONSTANT + rank)
            order = sorted(combined, key=lambda cid: (-combined[cid], -semantic_scores.get(cid, -1), cid))
        else:
            order = lexical if mode == "keyword" else semantic
            combined = {}
        results = []
        for cid in order[:top_k]:
            chunk = json.loads(self.db.execute("SELECT data FROM chunks WHERE id=?", (cid,)).fetchone()[0])
            results.append(
                {
                    "chunk_id": cid,
                    "text": chunk["text"],
                    "chapter_id": chunk["chapter_id"],
                    "chapter_path": chunk["chapter_path"],
                    "content_kind": chunk["content_kind"],
                    "locations": chunk["locations"],
                    "quality": chunk["quality"],
                    "limitations": chunk["limitations"],
                    "scores": {
                        "bm25": lexical_scores.get(cid),
                        "cosine": semantic_scores.get(cid),
                        "rrf": combined.get(cid),
                    },
                }
            )
        return {
            "query": query,
            "mode": mode,
            "fusion": {
                "method": "weighted_rrf",
                "rank_constant": RRF_RANK_CONSTANT,
                "keyword_weight": KEYWORD_WEIGHT,
                "dense_weight": DENSE_WEIGHT,
                "candidate_limit_per_method": candidate_limit,
            }
            if mode == "hybrid"
            else None,
            "answerability": "not_assessed",
            "note": "Retrieved passages are evidence candidates; no answer was generated.",
            "results": results,
        }
