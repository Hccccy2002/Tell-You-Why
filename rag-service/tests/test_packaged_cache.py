from pathlib import Path
import os

from tellwhy_kb.models import local_environment


def test_installed_weights_do_not_receive_cache_writes(tmp_path, monkeypatch):
    weights = tmp_path / "read-only-install" / "models"
    cache = tmp_path / "user-data" / "cache"
    monkeypatch.setenv("TELLWHY_MODEL_CACHE", str(cache))
    for name in ["PADDLE_PDX_CACHE_HOME", "HF_HOME", "MODELSCOPE_CACHE", "XDG_CACHE_HOME"]:
        monkeypatch.setenv(name, "previous-value")
    local_environment(weights)
    for name in ["PADDLE_PDX_CACHE_HOME", "HF_HOME", "MODELSCOPE_CACHE", "XDG_CACHE_HOME"]:
        assert Path(os.environ[name]).is_relative_to(cache)
        assert not Path(os.environ[name]).is_relative_to(weights)
    assert not weights.exists()
