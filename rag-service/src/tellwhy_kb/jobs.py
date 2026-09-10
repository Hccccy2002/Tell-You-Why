from __future__ import annotations

import importlib.metadata
import json
import os
import shutil
import sqlite3
from contextlib import contextmanager
from pathlib import Path

from . import PIPELINE_VERSION
from .schemas import IngestConfig, PageResult
from .util import atomic_json, digest, file_hash, read_json, safe_name


def runtime_versions() -> dict:
    return {
        name: importlib.metadata.version(name)
        for name in ("pypdf", "pypdfium2", "paddleocr", "paddlex", "paddlepaddle", "numpy", "torch")
    }


@contextmanager
def exclusive_lock(path: Path):
    """OS-released advisory lock; a crashed process never leaves a stale held lock."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+b") as handle:
        handle.seek(0, 2)
        if handle.tell() == 0:
            handle.write(b"0")
            handle.flush()
        handle.seek(0)
        try:
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            raise RuntimeError(f"Another writer is using {path.parent}") from exc
        try:
            yield
        finally:
            handle.seek(0)
            if os.name == "nt":
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(handle, fcntl.LOCK_UN)


def initialize_job(
    data_root: Path, kb: str, probe: dict, config: IngestConfig, models: dict, runtime: dict | None = None
) -> tuple[Path, dict]:
    kb_root = data_root.resolve() / "knowledge-bases" / safe_name(kb)
    identity = {
        "source_sha256": probe["sha256"],
        "config": config.model_dump(),
        "models": models,
        "pipeline": PIPELINE_VERSION,
        "runtime": runtime if runtime is not None else runtime_versions(),
    }
    job_id = digest(identity)[:24]
    folder = kb_root / "work" / job_id
    folder.mkdir(parents=True, exist_ok=True)
    source = kb_root / "sources" / f"{probe['sha256']}.pdf"
    with exclusive_lock(kb_root / "source.lock"):
        if not source.exists() or file_hash(source) != probe["sha256"]:
            source.parent.mkdir(parents=True, exist_ok=True)
            if shutil.disk_usage(source.parent).free < probe["bytes"] * 2 + 100_000_000:
                raise ValueError("Insufficient disk space for the source and working files")
            temporary = source.with_suffix(".copying")
            shutil.copyfile(probe["path"], temporary)
            if file_hash(temporary) != probe["sha256"]:
                raise ValueError("Source changed during import; retry with a stable file")
            os.replace(temporary, source)
    metadata = {
        **identity,
        "id": job_id,
        "kb": kb,
        "source": str(source),
        "source_name": probe.get("filename", Path(probe["path"]).name),
        "probe": {**probe, "path": str(source)},
    }
    with exclusive_lock(folder / "writer.lock"):
        if not (folder / "job.json").exists():
            atomic_json(folder / "job.json", metadata)
        with JobStore(folder) as store:
            store.db.executemany(
                "INSERT OR IGNORE INTO pages(page,status) VALUES (?, 'pending')",
                [(n,) for n in range(1, probe["pages"] + 1)],
            )
            store.db.commit()
    return folder, metadata


class JobStore:
    def __init__(self, folder: Path):
        self.folder = folder
        (folder / "pages").mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(folder / "job.sqlite")
        self.db.row_factory = sqlite3.Row
        self.db.execute("""CREATE TABLE IF NOT EXISTS pages(
            page INTEGER PRIMARY KEY, status TEXT NOT NULL, artifact_hash TEXT,
            assets TEXT, error TEXT, attempts INTEGER NOT NULL DEFAULT 0)""")
        self.db.commit()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.db.close()

    def cached(self, number: int) -> PageResult | None:
        row = self.db.execute("SELECT * FROM pages WHERE page=?", (number,)).fetchone()
        path = self.folder / "pages" / f"{number:04d}.json"
        if not row or row["status"] in {"pending", "running", "failed"} or not path.is_file():
            return None
        if row["artifact_hash"] != file_hash(path):
            return None
        for name, expected in json.loads(row["assets"] or "{}").items():
            asset = (self.folder / "crops" / name).resolve()
            if not asset.is_relative_to((self.folder / "crops").resolve()):
                return None
            if not asset.is_file() or file_hash(asset) != expected:
                return None
        try:
            result = PageResult.model_validate(read_json(path))
            return result if result.number == number else None
        except (ValueError, TypeError):
            return None

    def start(self, number: int):
        self.db.execute(
            "UPDATE pages SET status='running', error=NULL, attempts=attempts+1 WHERE page=?", (number,)
        )
        self.db.commit()

    def save(self, result: PageResult):
        if any(b.page != result.number for b in result.blocks):
            raise ValueError("Block belongs to the wrong page")
        if len({b.id for b in result.blocks}) != len(result.blocks):
            raise ValueError("Duplicate block IDs")
        for b in result.blocks:
            if b.bbox.x1 > result.width + 0.01 or b.bbox.y1 > result.height + 0.01:
                raise ValueError("Block lies outside the displayed page")
        assets = {}
        for block in result.blocks:
            if block.asset:
                asset = (self.folder / "crops" / block.asset).resolve()
                if not asset.is_relative_to((self.folder / "crops").resolve()):
                    raise ValueError("Asset escapes crop directory")
                assets[block.asset] = file_hash(asset)
        path = self.folder / "pages" / f"{result.number:04d}.json"
        atomic_json(path, result.model_dump())
        self.db.execute(
            """UPDATE pages SET status=?,artifact_hash=?,assets=?,error=NULL WHERE page=?""",
            (result.status, file_hash(path), json.dumps(assets), result.number),
        )
        self.db.commit()

    def fail(self, number: int, error: Exception):
        self.db.execute("UPDATE pages SET status='failed',error=? WHERE page=?", (str(error), number))
        self.db.commit()

    def counts(self) -> dict:
        return dict(self.db.execute("SELECT status,COUNT(*) FROM pages GROUP BY status").fetchall())
