from __future__ import annotations

import hashlib
import json
import os
import re
import tempfile
from pathlib import Path


def digest(value: object) -> str:
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True).encode()).hexdigest()


def file_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def atomic_json(path: Path, data: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=2, allow_nan=False)
            f.flush()
            os.fsync(f.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def safe_name(value: str) -> str:
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,63}", value):
        raise ValueError("ID must contain 1–64 lowercase letters, digits, hyphens or underscores")
    return value


def parse_pages(spec: str | None, count: int) -> list[int]:
    if not spec:
        return list(range(1, count + 1))
    pages = set()
    for part in spec.split(","):
        match = re.fullmatch(r"\s*(\d+)(?:-(\d+))?\s*", part)
        if not match:
            raise ValueError(f"Invalid page selection: {part}")
        a, b = int(match[1]), int(match[2] or match[1])
        if not 1 <= a <= b <= count:
            raise ValueError(f"Pages must be between 1 and {count}: {part}")
        pages.update(range(a, b + 1))
    return sorted(pages)


def event(stage: str, **fields) -> None:
    print(json.dumps({"stage": stage, **fields}, ensure_ascii=False, allow_nan=False), flush=True)
