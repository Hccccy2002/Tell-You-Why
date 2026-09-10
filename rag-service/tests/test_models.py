import pytest
from tellwhy_kb.models import REPOSITORIES, PINNED_REVISIONS, verify_models
from tellwhy_kb.util import atomic_json, file_hash


def test_model_manifest_requires_all_resources_and_detects_tampering(tmp_path):
    manifest = {}
    for name, repository in REPOSITORIES.items():
        folder = tmp_path / name
        folder.mkdir()
        resource = folder / "weights.bin"
        resource.write_bytes(b"known weights")
        manifest[name] = {
            "repository": repository,
            "revision": PINNED_REVISIONS[name],
            "files": {"weights.bin": file_hash(resource)},
        }
    atomic_json(tmp_path / "model-manifest.json", manifest)
    assert verify_models(tmp_path) == manifest
    (tmp_path / "rec/weights.bin").write_bytes(b"wrong weights")
    with pytest.raises(ValueError, match="changed or missing"):
        verify_models(tmp_path)


def test_manifest_paths_cannot_escape_model_directory(tmp_path):
    manifest = {"det": {"revision": "test", "files": {"../../outside": "fake"}}}
    atomic_json(tmp_path / "model-manifest.json", manifest)
    with pytest.raises(ValueError, match="escapes"):
        verify_models(tmp_path)
