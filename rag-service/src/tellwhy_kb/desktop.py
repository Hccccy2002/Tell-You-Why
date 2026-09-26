"""Local desktop adapter. One JSON request on stdin, one framed reply on stdout.

Read operations deliberately avoid importing inference runtimes. Import workers are
separate processes; their checkpoint and OS lock survive a hidden/reopened window.
"""

from __future__ import annotations

import base64
import io
import json
import shutil
import sqlite3
import sys
import time
from pathlib import Path

from . import PIPELINE_VERSION
from .util import atomic_json, file_hash, read_json, safe_name

REPLY_PREFIX = "TELLWHY_DESKTOP:"
ACTIVE_STATES = {"running", "cancelling"}
DELETE_WAIT_SECONDS = 30 * 60


def inside(root: Path, relative: str) -> Path:
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("资料路径超出了知识库目录")
    return path


def readonly(path: Path):
    return sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True, timeout=5)


def is_locked(path: Path) -> bool:
    from .jobs import exclusive_lock

    if not path.exists():
        return False
    try:
        with exclusive_lock(path):
            return False
    except RuntimeError:
        return True


class DesktopLibrary:
    def __init__(self, data_root: Path, models_root: Path):
        self.data = data_root.resolve()
        self.models = models_root.resolve()
        self.root = self.data / "knowledge-bases"

    def kb_root(self, kb: str) -> Path:
        return inside(self.root, safe_name(kb))

    def job_folder(self, kb: str, job: str) -> Path:
        folder = inside(self.kb_root(kb), "work/" + safe_name(job))
        if not (folder / "job.json").is_file():
            raise ValueError("导入任务不存在")
        return folder

    def kb_lock(self, kb: str) -> Path:
        return inside(self.root, ".locks/" + safe_name(kb) + ".lock")

    def published(self, kb: str, verify_data: bool = False):
        root = self.kb_root(kb)
        pointer = read_json(root / "active.json")
        directory = inside(root, "versions/" + safe_name(pointer["version"]))
        if file_hash(directory / "manifest.json") != pointer["manifest_sha256"]:
            raise ValueError("知识库清单校验失败，请重新导入")
        manifest = read_json(directory / "manifest.json")
        if verify_data:
            for relative in ("knowledge.sqlite", "embeddings.npy"):
                if file_hash(directory / relative) != manifest["files"][relative]:
                    raise ValueError("知识库索引校验失败，请重新导入")
            source = inside(root, manifest["source"]["path"])
            if file_hash(source) != manifest["source"]["sha256"]:
                raise ValueError("原始 PDF 校验失败")
        return directory, manifest

    def job_status(self, folder: Path):
        meta = read_json(folder / "job.json")
        state_file = folder / "desktop-state.json"
        state = read_json(state_file) if state_file.exists() else {}
        stage_file = folder / "stage.json"
        stage = read_json(stage_file).get("stage", "extracting") if stage_file.exists() else "extracting"
        with readonly(folder / "job.sqlite") as db:
            counts = dict(db.execute("SELECT status,COUNT(*) FROM pages GROUP BY status"))
            errors = [
                {"page": page, "error": error}
                for page, error in db.execute("SELECT page,error FROM pages WHERE status='failed' LIMIT 3")
            ]
        running = is_locked(folder / "desktop.lock") or is_locked(folder / "writer.lock")
        status = state.get("status", "paused")
        if running:
            status = "cancelling" if (folder / "cancel.request").exists() else "running"
        elif status in ACTIVE_STATES:
            status = "paused"
        elif not state and stage == "published":
            status = "completed"
        return {
            "id": meta["id"],
            "status": status,
            "stage": state.get("stage", stage) if running else stage,
            "total": meta["probe"]["pages"],
            "completed": sum(counts.get(k, 0) for k in ("success", "needs_review", "blank", "excluded")),
            "counts": counts,
            "error": state.get("error"),
            "page_errors": errors,
        }

    def catalog(self):
        items, errors = [], []
        for root in sorted(self.root.glob("*")):
            if not root.is_dir() or root.name.startswith("."):
                continue
            try:
                root = self.kb_root(root.name)
                manifest = self.published(root.name)[1] if (root / "active.json").exists() else None
                jobs = sorted(root.glob("work/*/job.json"), key=lambda p: p.stat().st_mtime, reverse=True)
                if not manifest and not jobs:
                    continue
                statuses = [(self.job_status(p.parent), read_json(p)) for p in jobs]
                current = next((s for s in statuses if s[0]["status"] in ACTIVE_STATES), None)
                current = current or next(
                    (s for s in statuses if (root / "work" / s[0]["id"] / "desktop-state.json").exists()),
                    None,
                )
                current = current or next(
                    (s for s in statuses if manifest and s[0]["id"] == manifest["job_id"]), None
                )
                current = current or (statuses[0] if statuses else None)
                source = manifest["source"] if manifest else None
                meta = current[1] if current else None
                items.append(
                    {
                        "id": root.name,
                        "filename": source.get("filename", root.name + ".pdf")
                        if source
                        else meta["source_name"],
                        "pages": source["pages"] if source else meta["probe"]["pages"],
                        "status": manifest["status"] if manifest else "not_ready",
                        "version": manifest["version"] if manifest else None,
                        "chunks": manifest["counts"]["chunks"] if manifest else 0,
                        "coverage": manifest.get("coverage") if manifest else None,
                        "job": current[0] if current else None,
                    }
                )
            except (OSError, ValueError, KeyError, sqlite3.Error) as exc:
                errors.append({"id": root.name, "error": str(exc)})
        return {
            "items": items,
            "errors": errors,
            "models_ready": (self.models / "model-manifest.json").is_file(),
            "import_running": is_locked(self.data / "desktop-import.lock")
            or any(i["job"] and i["job"]["status"] in ACTIVE_STATES for i in items),
        }

    def inspect(self, pdf: str):
        from .ingest.probe import inspect_pdf

        probe = inspect_pdf(Path(pdf))
        return {key: probe[key] for key in ("filename", "pages", "bytes", "sha256")}

    def prepare(self, pdf: str, first_page: int = 1, ocr_mode: str = "always"):
        from .ingest.probe import inspect_pdf
        from .jobs import initialize_job
        from .models import verify_models
        from .schemas import IngestConfig

        if self.catalog()["import_running"]:
            raise ValueError("已有 PDF 正在处理，请先等待完成或暂停当前任务")
        probe = inspect_pdf(Path(pdf))
        if type(first_page) is not int or not 1 <= first_page <= probe["pages"]:
            raise ValueError("正文起始页必须在 PDF 页数范围内")
        config = IngestConfig(
            exclude_before_page=first_page,
            ocr_mode=ocr_mode,
            native_text_trusted=ocr_mode == "auto",
        )
        requested_config = config.model_dump()
        # Re-selecting a known PDF opens its existing knowledge base without replacing
        # accepted chapter overrides or re-running a full textbook's OCR.
        for root in sorted(self.root.glob("*")):
            if (root / "sources" / (probe["sha256"] + ".pdf")).is_file():
                if (root / "active.json").exists():
                    _, manifest = self.published(root.name, verify_data=True)
                    if (
                        manifest["source"]["sha256"] == probe["sha256"]
                        and manifest.get("config") == requested_config
                        and manifest.get("pipeline") == PIPELINE_VERSION
                    ):
                        return {"kb": root.name, "job": manifest["job_id"], "reused": True}
                jobs = sorted(root.glob("work/*/job.json"), key=lambda p: p.stat().st_mtime, reverse=True)
                for path in jobs:
                    meta = read_json(path)
                    if (
                        meta["source_sha256"] == probe["sha256"]
                        and meta.get("config") == requested_config
                        and meta.get("pipeline") == PIPELINE_VERSION
                    ):
                        return {"kb": root.name, "job": meta["id"], "reused": False}
        models = verify_models(self.models)
        kb = "pdf-" + probe["sha256"][:20]
        folder, meta = initialize_job(self.data, kb, probe, config, models)
        atomic_json(folder / "desktop-state.json", {"status": "paused", "stage": "extracting"})
        return {"kb": kb, "job": meta["id"], "reused": False}

    def run_import(self, kb: str, job: str):
        from .jobs import exclusive_lock, runtime_versions
        from .models import verify_models

        root = self.kb_root(kb)
        if (root / "delete.request").exists():
            raise ValueError("这份 PDF 正在删除")
        folder = self.job_folder(kb, job)
        meta = read_json(folder / "job.json")
        with (
            exclusive_lock(self.data / "desktop-import.lock"),
            exclusive_lock(self.kb_lock(kb)),
            exclusive_lock(folder / "desktop.lock"),
        ):
            if (root / "delete.request").exists():
                raise ValueError("这份 PDF 正在删除")
            state = {"status": "running", "stage": "verifying", "updated_at": time.time()}

            def update(**values):
                state.update(values, updated_at=time.time())
                atomic_json(folder / "desktop-state.json", state)

            update()
            try:
                if verify_models(self.models) != meta["models"] or runtime_versions() != meta["runtime"]:
                    raise ValueError("模型或 Python 运行环境已改变，无法继续旧任务")
                from .pipeline import extract_pages
                from .build import build_job

                # Cleared by the launcher before starting, never here: an immediate
                # pause during model loading must not be discarded.
                update(stage="extracting")
                counts = extract_pages(folder, self.models)
                if (folder / "cancel.request").exists():
                    update(status="paused")
                    return {"status": "paused"}
                if counts.get("failed", 0) or counts.get("pending", 0) or counts.get("running", 0):
                    raise ValueError("部分页面识别失败，已保存成功页面，可点击继续处理重试")
                update(stage="indexing")
                result = build_job(folder, self.models)
                update(status="completed", stage="published", error=None)
                return result
            except Exception as exc:
                update(status="failed", error=str(exc))
                raise

    def launch_ready(self, kb: str, job: str):
        folder = self.job_folder(kb, job)
        if (self.kb_root(kb) / "delete.request").exists():
            raise ValueError("这份 PDF 正在删除")
        if self.catalog()["import_running"]:
            raise ValueError("已有 PDF 正在处理，请等待完成或暂停当前任务")
        (folder / "cancel.request").unlink(missing_ok=True)
        return {"kb": kb, "job": job}

    def cancel(self, kb: str, job: str):
        folder = self.job_folder(kb, job)
        (folder / "cancel.request").write_text("pause requested", encoding="utf-8")
        return {"status": "cancelling"}

    def delete(self, kb: str):
        from .jobs import exclusive_lock

        root = self.kb_root(kb)
        if not root.is_dir():
            return {"deleted": False}
        marker = root / "delete.request"
        marker.write_text("delete requested", encoding="utf-8")
        deadline = time.monotonic() + DELETE_WAIT_SECONDS
        try:
            while root.exists():
                locked = False
                for job_file in root.glob("work/*/job.json"):
                    folder = job_file.parent
                    if is_locked(folder / "desktop.lock") or is_locked(folder / "writer.lock"):
                        locked = True
                        (folder / "cancel.request").write_text("delete requested", encoding="utf-8")
                if locked:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("等待 PDF 处理任务停止超时，请稍后重试删除")
                    time.sleep(0.2)
                    continue
                try:
                    with exclusive_lock(self.kb_lock(kb)):
                        while root.exists():
                            try:
                                shutil.rmtree(root)
                            except OSError:
                                if time.monotonic() >= deadline:
                                    raise TimeoutError("等待 PDF 文件释放超时，请稍后重试删除")
                                if root.exists():
                                    marker.write_text("delete requested", encoding="utf-8")
                                time.sleep(0.2)
                        return {"deleted": True}
                except RuntimeError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("等待 PDF 处理任务停止超时，请稍后重试删除")
                    marker.write_text("delete requested", encoding="utf-8")
                    time.sleep(0.2)
            return {"deleted": True}
        except Exception:
            marker.unlink(missing_ok=True)
            raise

    def chapters(self, kb: str):
        directory, _ = self.published(kb, verify_data=True)
        with readonly(directory / "knowledge.sqlite") as db:
            chapters = [json.loads(r[0]) for r in db.execute("SELECT data FROM chapters ORDER BY rowid")]
        return [
            {
                "id": c["id"],
                "title": c["title"],
                "depth": c["level"],
                "kind": c["kind"],
                "start_page": c["start"][0] if c.get("start") else None,
            }
            for c in chapters
        ]

    def browse(self, kb: str, chapter: str | None = None, offset: int = 0):
        if type(offset) is not int or offset < 0:
            raise ValueError("无效的分页位置")
        directory, _ = self.published(kb, verify_data=True)
        params = []
        where = "eligible=1"
        if chapter:
            where += """ AND section_id IN (
                WITH RECURSIVE selected(id) AS (
                    SELECT id FROM chapters WHERE id=? UNION
                    SELECT child.id FROM chapters child JOIN selected ON child.parent_id=selected.id
                ) SELECT id FROM selected)"""
            params.append(chapter)
        with readonly(directory / "knowledge.sqlite") as db:
            if chapter and not db.execute("SELECT 1 FROM chapters WHERE id=?", (chapter,)).fetchone():
                raise ValueError("章节不存在")
            total = db.execute("SELECT COUNT(*) FROM chunks WHERE " + where, params).fetchone()[0]
            rows = db.execute(
                "SELECT data FROM chunks WHERE " + where + " ORDER BY rowid LIMIT 10 OFFSET ?",
                params + [offset],
            ).fetchall()
        return {"total": total, "offset": offset, "items": [json.loads(r[0]) for r in rows]}

    def search(self, kb: str, query: str, mode: str = "hybrid", chapter: str | None = None):
        if not isinstance(query, str) or not query.strip() or len(query) > 1000:
            raise ValueError("请输入 1–1000 字的检索内容")
        directory, manifest = self.published(kb, verify_data=True)
        encoder = None
        if mode != "keyword":
            for relative, checksum in manifest["models"]["embedding"]["files"].items():
                if file_hash(inside(self.models / "embedding", relative)) != checksum:
                    raise ValueError("检索模型校验失败")
            from .indexing.embedding import Encoder

            encoder = Encoder(self.models)
        from .search import SearchIndex

        with SearchIndex(directory, encoder) as index:
            return index.search(query, top_k=5, mode=mode, chapter=chapter)

    def evidence(self, kb: str, query: str, version: str, chapter: str | None = None, mode: str = "hybrid"):
        if not isinstance(query, str) or not query.strip() or len(query) > 1000:
            raise ValueError("请输入 1–1000 字的问题或学习主题")
        if mode not in {"hybrid", "keyword"}:
            raise ValueError("无效的检索方式")
        directory, manifest = self.published(kb, verify_data=True)
        if manifest["version"] != version:
            raise ValueError("资料版本已更新，请刷新后重新检索")
        encoder = None
        if mode == "hybrid":
            for relative, checksum in manifest["models"]["embedding"]["files"].items():
                if file_hash(inside(self.models / "embedding", relative)) != checksum:
                    raise ValueError("检索模型校验失败")
            from .indexing.embedding import Encoder

            encoder = Encoder(self.models)
        from .search import SearchIndex
        from .evidence import assemble

        with SearchIndex(directory, encoder) as index:
            return assemble(index, manifest, kb, query.strip(), chapter, mode)

    def learning_version(self, kb: str, version: str):
        _, manifest = self.published(kb, verify_data=True)
        if manifest["version"] != version:
            raise ValueError("资料版本已更新，请重新预览")
        return {"version": version}

    def _related_source_version(
        self, kb: str, version: str, chapter: str | None, source_sha256: str | None
    ):
        root = self.kb_root(kb)
        directory = inside(root, "versions/" + safe_name(version))
        replaced = not directory.is_dir()
        if replaced:
            try:
                directory, manifest = self.published(kb, verify_data=True)
            except (OSError, ValueError, KeyError, sqlite3.Error) as exc:
                raise ValueError("原资料版本已删除，且当前没有可用的同一 PDF") from exc
        else:
            manifest = read_json(directory / "manifest.json")
            if manifest["version"] != version:
                raise ValueError("原文版本不一致")
        actual_sha256 = manifest["source"]["sha256"]
        if source_sha256 and actual_sha256 != source_sha256:
            raise ValueError("当前知识库已不是生成这张学习卡时使用的 PDF")
        if replaced and not source_sha256:
            generated_id = "pdf-" + actual_sha256[:20]
            if kb != generated_id:
                raise ValueError("原资料版本已删除，无法确认当前 PDF 与原资料一致")
        if replaced and chapter:
            with readonly(directory / "knowledge.sqlite") as db:
                exists = db.execute(
                    "SELECT 1 FROM chapters WHERE id=? OR title=?", (chapter, chapter)
                ).fetchone()
            if not exists:
                chapter = None
        return directory, manifest, chapter

    def related_sources(
        self,
        kb: str,
        version: str,
        query: str,
        chapter: str | None = None,
        source_sha256: str | None = None,
    ):
        """Top 5 for the finished question; never replace its original citation snapshot."""
        if not isinstance(query, str) or not query.strip() or len(query) > 1000:
            raise ValueError("请输入 1–1000 字的问题")
        directory, manifest, chapter = self._related_source_version(
            kb, version, chapter, source_sha256
        )
        for relative in ("knowledge.sqlite", "embeddings.npy"):
            if file_hash(directory / relative) != manifest["files"][relative]:
                raise ValueError("知识库索引校验失败，请重新导入")
        source = inside(self.kb_root(kb), manifest["source"]["path"])
        if file_hash(source) != manifest["source"]["sha256"]:
            raise ValueError("原始 PDF 校验失败")
        for relative, checksum in manifest["models"]["embedding"]["files"].items():
            if file_hash(inside(self.models / "embedding", relative)) != checksum:
                raise ValueError("检索模型校验失败")
        from .evidence import assemble
        from .indexing.embedding import Encoder
        from .indexing.reranker import Reranker
        from .search import SearchIndex

        reranker = Reranker(self.models)
        with SearchIndex(directory, Encoder(self.models)) as index:
            return assemble(index, manifest, kb, query.strip(), chapter, reranker=reranker)

    def learning_units(self, kb: str, version: str, chapter: str | None = None):
        from .learning import units

        directory, manifest = self.published(kb, verify_data=True)
        if manifest["version"] != version:
            raise ValueError("资料版本已更新，请刷新后重新选择")
        with readonly(directory / "knowledge.sqlite") as db:
            items = units(db, manifest, kb, chapter)
            chapters = {r[0]: json.loads(r[1]) for r in db.execute("SELECT id,data FROM chapters")}
            path, current, seen = [], chapter, set()
            while current in chapters and current not in seen:
                seen.add(current)
                path.insert(0, chapters[current]["title"])
                current = chapters[current].get("parent_id")
            return {
                "kb": kb,
                "version": version,
                "filename": manifest["source"].get("filename", kb + ".pdf"),
                "source_sha256": manifest["source"]["sha256"],
                "chapter_path": path,
                "items": items,
            }

    def learning_evidence(self, kb: str, version: str, unit_id: str, chapter: str | None = None):
        from .evidence import assemble
        from .learning import units
        from .search import SearchIndex

        directory, manifest = self.published(kb, verify_data=True)
        if manifest["version"] != version:
            raise ValueError("资料版本已更新，请重新预览")
        with SearchIndex(directory) as index:
            candidates = units(index.db, manifest, kb, chapter)
            unit = next((u for u in candidates if u["id"] == unit_id), None)
            if not unit:
                raise ValueError("主素材不属于所选范围或已失效")
            # The selected full blocks are mandatory; keyword search only supplements them.
            return assemble(index, manifest, kb, unit["query"], chapter, "keyword", primary_unit=unit)

    def page(self, kb: str, page: int, version: str | None = None):
        if version is None:
            directory, manifest = self.published(kb)
        else:
            directory = inside(self.kb_root(kb), "versions/" + safe_name(version))
            manifest = read_json(directory / "manifest.json")
            if manifest["version"] != version:
                raise ValueError("原文版本不一致")
        if type(page) is not int or not 1 <= page <= manifest["source"]["pages"]:
            raise ValueError("页码超出 PDF 范围")
        source = inside(self.kb_root(kb), manifest["source"]["path"])
        if file_hash(source) != manifest["source"]["sha256"]:
            raise ValueError("原始 PDF 校验失败")
        from .ingest.extract import PdfSource

        with PdfSource(source) as pdf, pdf.render(page, 120) as rendered:
            rendered.thumbnail((1400, 1800))
            buffer = io.BytesIO()
            rendered.save(buffer, format="PNG")
        with readonly(directory / "knowledge.sqlite") as db:
            row = db.execute("SELECT data FROM pages WHERE page=?", (page,)).fetchone()
        return {
            "page": page,
            "printed_label": json.loads(row[0]).get("printed_label") if row else None,
            "image": "data:image/png;base64," + base64.b64encode(buffer.getvalue()).decode("ascii"),
        }

    def dispatch(self, request: dict):
        op = request.get("op")
        actions = {
            "catalog": self.catalog,
            "inspect": self.inspect,
            "prepare": self.prepare,
            "run": self.run_import,
            "launch_ready": self.launch_ready,
            "cancel": self.cancel,
            "delete": self.delete,
            "chapters": self.chapters,
            "browse": self.browse,
            "search": self.search,
            "page": self.page,
            "evidence": self.evidence,
            "related_sources": self.related_sources,
            "learning_units": self.learning_units,
            "learning_version": self.learning_version,
            "learning_evidence": self.learning_evidence,
        }
        if op not in actions:
            raise ValueError("不支持的知识库操作")
        return actions[op](**{k: v for k, v in request.items() if k != "op"})


def main():
    try:
        payload = json.load(sys.stdin)
        library = DesktopLibrary(Path(payload["data_root"]), Path(payload["models_root"]))
        result = {"ok": True, "data": library.dispatch(payload["request"])}
    except Exception as exc:
        result = {"ok": False, "error": str(exc)}
    print(REPLY_PREFIX + json.dumps(result, ensure_ascii=False, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
