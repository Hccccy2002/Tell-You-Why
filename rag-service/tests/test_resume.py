import pytest

from tellwhy_kb.jobs import JobStore, exclusive_lock, initialize_job
from tellwhy_kb.schemas import IngestConfig, PageResult
from tellwhy_kb.util import file_hash


def test_resume_invalidates_corrupt_checkpoint_and_deduplicates_import(tmp_path):
    source = tmp_path / "fake.pdf"
    source.write_bytes(b"test bytes")
    probe = {"path": str(source), "sha256": file_hash(source), "pages": 2, "bytes": 10}
    folder, meta = initialize_job(tmp_path, "test", probe, IngestConfig(), {}, runtime={})
    with JobStore(folder) as store:
        store.start(1)
        store.save(PageResult(number=1, width=100, height=100, status="blank", method="none"))
        assert store.cached(1).status == "blank"
        assert store.cached(2) is None
        assert store.counts() == {"blank": 1, "pending": 1}
    same, _ = initialize_job(tmp_path, "test", probe, IngestConfig(), {}, runtime={})
    assert same == folder
    with JobStore(folder) as store:
        assert store.cached(1) is not None
        (folder / "pages/0001.json").write_text("corrupt")
        assert store.cached(1) is None
    changed, _ = initialize_job(tmp_path, "test", probe, IngestConfig(dpi=300), {}, runtime={})
    assert changed != folder
    assert file_hash(source) == meta["source_sha256"]


def test_lock_rejects_concurrent_writer_and_releases_after_exception(tmp_path):
    path = tmp_path / "writer.lock"
    with pytest.raises(ValueError):
        with exclusive_lock(path):
            with pytest.raises(RuntimeError, match="Another writer"):
                with exclusive_lock(path):
                    pass
            raise ValueError("simulated interruption")
    with exclusive_lock(path):
        pass
