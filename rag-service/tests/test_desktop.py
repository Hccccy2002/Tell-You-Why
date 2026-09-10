import json
import subprocess
import sys

import pytest

from tellwhy_kb.desktop import DesktopLibrary, REPLY_PREFIX
from tellwhy_kb.jobs import JobStore, exclusive_lock
from tellwhy_kb.storage.manifest import publish_version
from tellwhy_kb.util import atomic_json, read_json
from test_publication import sample


@pytest.fixture
def desktop(tmp_path):
    root = tmp_path / "knowledge-bases" / "test"
    root.mkdir(parents=True)
    args = list(sample(root))
    args[0]["source_name"] = "计算机 组成原理.pdf"
    args[2][0].update(level=0, kind="chapter", start=[1, 0])
    publish_version(root, "version1", *args)
    return DesktopLibrary(tmp_path, tmp_path / "models"), root, args


def test_real_publication_browse_search_source_and_dedup(desktop):
    library, root, args = desktop
    items = library.catalog()["items"]
    assert items[0]["filename"] == "计算机 组成原理.pdf"
    assert items[0]["chunks"] == 1
    assert library.chapters("test")[0]["start_page"] == 1
    assert library.browse("test", "c1")["items"][0]["text"] == "存储器存放程序和数据。"
    assert library.browse("test", offset=10)["items"] == []
    result = library.search("test", "存储器", "keyword", "c1")
    assert result["answerability"] == "not_assessed"
    assert result["results"][0]["locations"][0]["page"] == 1
    assert library.page("test", 1)["image"].startswith("data:image/png;base64,")
    source = root / "sources" / (args[0]["source_sha256"] + ".pdf")
    assert library.prepare(str(source))["reused"]
    assert len(list((root / "versions").iterdir())) == 1


def test_invalid_paths_queries_pages_and_corrupt_index_fail_closed(desktop):
    library, root, _ = desktop
    with pytest.raises(ValueError):
        library.browse("../test")
    for page in [0, 2, 1.5]:
        with pytest.raises(ValueError):
            library.page("test", page)
    for query in ["", "x" * 1001]:
        with pytest.raises(ValueError):
            library.search("test", query)
    with pytest.raises(ValueError):
        library.browse("test", "missing")
    with pytest.raises(ValueError):
        library.dispatch({"op": "shell"})
    (root / "versions/version1/embeddings.npy").write_bytes(b"corrupted")
    with pytest.raises(ValueError, match="校验失败"):
        library.browse("test")


def make_job(library, root, args):
    folder = root / "work/job1"
    folder.mkdir(parents=True)
    atomic_json(folder / "job.json", {**args[0], "probe": {"pages": 1}})
    with JobStore(folder) as store:
        store.db.execute("INSERT INTO pages(page,status) VALUES (1,'pending')")
        store.db.commit()
    return folder


def test_lock_and_stale_running_status_enable_safe_resume(desktop):
    library, root, args = desktop
    folder = make_job(library, root, args)
    atomic_json(folder / "desktop-state.json", {"status": "running", "stage": "extracting"})
    assert library.job_status(folder)["status"] == "paused"
    with exclusive_lock(folder / "desktop.lock"):
        assert library.catalog()["import_running"]
        assert library.job_status(folder)["status"] == "running"
        with pytest.raises(ValueError, match="已有 PDF"):
            library.launch_ready("test", "job1")
        library.cancel("test", "job1")
        assert library.job_status(folder)["status"] == "cancelling"
    library.launch_ready("test", "job1")
    assert not (folder / "cancel.request").exists()


def test_immediate_pause_during_startup_is_not_lost(desktop, monkeypatch):
    library, root, args = desktop
    folder = make_job(library, root, args)
    monkeypatch.setattr("tellwhy_kb.models.verify_models", lambda _: {})
    monkeypatch.setattr("tellwhy_kb.jobs.runtime_versions", lambda: {})
    monkeypatch.setattr("tellwhy_kb.pipeline.extract_pages", lambda *_: {"pending": 1})
    library.cancel("test", "job1")
    assert library.run_import("test", "job1")["status"] == "paused"
    assert read_json(folder / "desktop-state.json")["status"] == "paused"
    assert not library.catalog()["import_running"]


def test_failure_is_persisted_and_exclusive_worker_rejected(desktop, monkeypatch):
    library, root, args = desktop
    folder = make_job(library, root, args)
    with exclusive_lock(library.data / "desktop-import.lock"):
        with pytest.raises(RuntimeError, match="Another writer"):
            library.run_import("test", "job1")
    monkeypatch.setattr("tellwhy_kb.models.verify_models", lambda _: {"changed": True})
    with pytest.raises(ValueError, match="运行环境"):
        library.run_import("test", "job1")
    assert read_json(folder / "desktop-state.json")["status"] == "failed"
    assert not library.catalog()["import_running"]


def test_json_process_protocol_with_unicode_paths(desktop):
    library, _, _ = desktop
    completed = subprocess.run(
        [sys.executable, "-X", "utf8", "-m", "tellwhy_kb.desktop"],
        input=json.dumps(
            {"data_root": str(library.data), "models_root": str(library.models), "request": {"op": "catalog"}}
        ),
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    reply = json.loads(
        next(
            s.removeprefix(REPLY_PREFIX) for s in completed.stdout.splitlines() if s.startswith(REPLY_PREFIX)
        )
    )
    assert reply["ok"]
    assert reply["data"]["items"][0]["filename"] == "计算机 组成原理.pdf"


def test_broken_catalog_entry_does_not_hide_healthy_books(desktop):
    library, _, _ = desktop
    bad = library.root / "broken"
    atomic_json(bad / "active.json", {"version": "missing"})
    catalog = library.catalog()
    assert len(catalog["items"]) == 1
    assert catalog["errors"][0]["id"] == "broken"
