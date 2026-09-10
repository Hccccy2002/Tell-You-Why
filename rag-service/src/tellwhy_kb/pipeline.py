from __future__ import annotations

import os
from concurrent.futures import ProcessPoolExecutor, wait, FIRST_COMPLETED
from pathlib import Path

from .ingest.extract import PdfSource
from .ingest.ocr import OcrEngine
from .ingest.routing import native_page, use_native
from .jobs import JobStore, exclusive_lock
from .models import local_environment
from .schemas import IngestConfig
from .util import atomic_json, event, read_json

_engine = None
_source = None
_config = None
_assets = None


def worker_init(source: str, models: str, config: dict, assets: str):
    global _source, _engine, _config, _assets
    local_environment(Path(models))
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    _config = IngestConfig.model_validate(config)
    _source = PdfSource(Path(source))
    _source.__enter__()
    _assets = Path(assets)
    _engine = None
    if _config.ocr_mode == "always":
        _engine = OcrEngine(Path(models), _config)
    # Retain for lazy initialization when auto routing encounters an image page.
    _source.models_root = Path(models)


def process_page(number: int):
    global _engine
    native = _source.read(number)
    if use_native(native, _config):
        return native_page(native, number, _config)
    if _config.ocr_mode == "native":
        raise ValueError(f"Page {number} is not usable native text; enable OCR")
    if _engine is None:
        _engine = OcrEngine(_source.models_root, _config)
    return _engine.process(_source, number, _assets)


def extract_pages(folder: Path, models_root: Path, pages: list[int] | None = None, workers: int = 1) -> dict:
    if not 1 <= workers <= 4:
        raise ValueError("workers must be between 1 and 4")
    meta = read_json(folder / "job.json")
    selected = sorted(set(pages if pages is not None else range(1, meta["probe"]["pages"] + 1)))
    if not selected or selected[0] < 1 or selected[-1] > meta["probe"]["pages"]:
        raise ValueError("Invalid page selection")
    if workers * meta["config"]["cpu_threads"] > (os.cpu_count() or 1):
        raise ValueError("Worker threads exceed available logical CPUs")
    with exclusive_lock(folder / "writer.lock"), JobStore(folder) as store:
        pending = [n for n in selected if store.cached(n) is None]
        event("extraction_started", job=meta["id"], pending=len(pending), reused=len(selected) - len(pending))
        if not pending:
            return store.counts()
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=worker_init,
            initargs=(meta["source"], str(models_root), meta["config"], str(folder / "crops")),
        ) as pool:
            futures = {}
            remaining = iter(pending)

            def submit_next():
                if (folder / "cancel.request").exists():
                    return
                n = next(remaining, None)
                if n is not None:
                    store.start(n)
                    futures[pool.submit(process_page, n)] = n

            for _ in range(workers):
                submit_next()
            try:
                while futures:
                    finished, _ = wait(futures, return_when=FIRST_COMPLETED)
                    for future in finished:
                        n = futures.pop(future)
                        try:
                            result = future.result()
                            store.save(result)
                            event(
                                "page_finished", page=n, status=result.status, seconds=result.elapsed_seconds
                            )
                        except Exception as exc:
                            store.fail(n, exc)
                            event("page_failed", page=n, error=str(exc))
                        atomic_json(folder / "progress.json", {"last_page": n, "counts": store.counts()})
                        submit_next()
            except KeyboardInterrupt:
                # Only the currently executing pages finish; the entire book is never prequeued.
                for future in futures:
                    future.cancel()
                raise
        return store.counts()
