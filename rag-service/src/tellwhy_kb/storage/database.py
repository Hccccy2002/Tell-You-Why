from __future__ import annotations

import json
import sqlite3

from ..indexing.keyword import KeywordTokenizer
from ..processing.chunking import validate_locations


def json_text(value) -> str:
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def write_database(path, document, pages, chapters, blocks, chunks, terms):
    if path.exists():
        raise ValueError("Refusing to overwrite an existing version database")
    db = sqlite3.connect(path)
    try:
        db.execute("PRAGMA foreign_keys=ON")
        db.executescript("""
            CREATE TABLE documents(id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE pages(page INTEGER PRIMARY KEY, status TEXT NOT NULL, data TEXT NOT NULL);
            CREATE TABLE chapters(id TEXT PRIMARY KEY,parent_id TEXT,title TEXT NOT NULL,data TEXT NOT NULL);
            CREATE TABLE blocks(id TEXT PRIMARY KEY,page INTEGER NOT NULL REFERENCES pages(page),
                text TEXT NOT NULL,raw_text TEXT NOT NULL,eligible INTEGER NOT NULL,data TEXT NOT NULL);
            CREATE TABLE chunks(id TEXT PRIMARY KEY,chapter_id TEXT REFERENCES chapters(id),
                section_id TEXT REFERENCES chapters(id),text TEXT NOT NULL,tokens INTEGER NOT NULL,
                eligible INTEGER NOT NULL,kind TEXT NOT NULL,data TEXT NOT NULL);
            CREATE TABLE chunk_locations(chunk_id TEXT REFERENCES chunks(id),position INTEGER,
                block_id TEXT REFERENCES blocks(id),page INTEGER REFERENCES pages(page),data TEXT NOT NULL,
                PRIMARY KEY(chunk_id,position));
            CREATE TABLE vector_rows(row_index INTEGER PRIMARY KEY,chunk_id TEXT UNIQUE NOT NULL REFERENCES chunks(id));
            CREATE TABLE assets(name TEXT PRIMARY KEY,block_id TEXT REFERENCES blocks(id));
            CREATE TABLE index_metadata(key TEXT PRIMARY KEY,data TEXT NOT NULL);
            CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED,tokens,tokenize='unicode61');
            CREATE INDEX chunks_chapter ON chunks(chapter_id);
            CREATE INDEX blocks_page ON blocks(page);
        """)
        tokenizer = KeywordTokenizer(terms)
        db.execute("INSERT INTO documents VALUES (?,?)", (document["source_sha256"], json_text(document)))
        db.executemany(
            "INSERT INTO pages VALUES (?,?,?)", [(p["number"], p["status"], json_text(p)) for p in pages]
        )
        db.executemany(
            "INSERT INTO chapters VALUES (?,?,?,?)",
            [(c["id"], c["parent_id"], c["title"], json_text(c)) for c in chapters],
        )
        db.executemany(
            "INSERT INTO blocks VALUES (?,?,?,?,?,?)",
            [
                (b["id"], b["page"], b["text"], b["raw_text"], int(b["eligible"]), json_text(b))
                for b in blocks
            ],
        )
        for i, chunk in enumerate(chunks):
            db.execute(
                "INSERT INTO chunks VALUES (?,?,?,?,?,?,?,?)",
                (
                    chunk["id"],
                    chunk["chapter_id"],
                    chunk["section_id"],
                    chunk["text"],
                    chunk["tokens"],
                    int(chunk["eligible"]),
                    chunk["content_kind"],
                    json_text(chunk),
                ),
            )
            db.executemany(
                "INSERT INTO chunk_locations VALUES (?,?,?,?,?)",
                [
                    (chunk["id"], j, loc["block_id"], loc["page"], json_text(loc))
                    for j, loc in enumerate(chunk["locations"])
                ],
            )
            db.execute("INSERT INTO vector_rows VALUES (?,?)", (i, chunk["id"]))
            db.execute(
                "INSERT INTO chunks_fts(rowid,chunk_id,tokens) VALUES (?,?,?)",
                (i + 1, chunk["id"], tokenizer.index_text(chunk["embedding_text"])),
            )
        db.executemany(
            "INSERT OR IGNORE INTO assets VALUES (?,?)",
            [(b["asset"], b["id"]) for b in blocks if b.get("asset")],
        )
        db.execute(
            "INSERT INTO index_metadata VALUES ('keyword',?)",
            (json_text({"terms": terms, "mode": "jieba_search"}),),
        )
        db.commit()
    finally:
        db.close()


def open_readonly(path):
    db = sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    return db


def validate_database(path, expected_pages: int) -> dict:
    db = open_readonly(path)
    try:
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("SQLite integrity check failed")
        if db.execute("PRAGMA foreign_key_check").fetchall():
            raise ValueError("Broken database references")
        page_numbers = [r[0] for r in db.execute("SELECT page FROM pages ORDER BY page")]
        if page_numbers != list(range(1, expected_pages + 1)):
            raise ValueError("Not all physical pages are accounted for")
        chunks = [json.loads(r[0]) for r in db.execute("SELECT data FROM chunks ORDER BY id")]
        blocks = [json.loads(r[0]) for r in db.execute("SELECT data FROM blocks")]
        validate_locations(chunks, blocks)
        rows = db.execute("SELECT row_index,chunk_id FROM vector_rows ORDER BY row_index").fetchall()
        if [r[0] for r in rows] != list(range(len(chunks))) or {r[1] for r in rows} != {
            c["id"] for c in chunks
        }:
            raise ValueError("Vector row mapping differs from chunks")
        fts_ids = [r[0] for r in db.execute("SELECT chunk_id FROM chunks_fts")]
        if sorted(fts_ids) != sorted(c["id"] for c in chunks):
            raise ValueError("Keyword index differs from chunks")
        for chunk in chunks:
            stored = [
                json.loads(r[0])
                for r in db.execute(
                    "SELECT data FROM chunk_locations WHERE chunk_id=? ORDER BY position", (chunk["id"],)
                )
            ]
            if stored != chunk["locations"]:
                raise ValueError("Location table differs from chunk provenance")
        return {
            "pages": len(page_numbers),
            "chunks": len(chunks),
            "blocks": len(blocks),
            "vectors": len(rows),
        }
    finally:
        db.close()
