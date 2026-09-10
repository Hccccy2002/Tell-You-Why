import pytest
from tellwhy_kb.cli import parser, find_job


def test_cli_stage_flags_and_job_lookup_are_explicit(tmp_path):
    args = parser().parse_args(
        ["ingest", "--pdf", "book.pdf", "--kb", "book", "--pages", "1,4-6", "--until", "structure"]
    )
    assert args.until == "structure" and args.workers == 1
    with pytest.raises(ValueError):
        find_job(tmp_path, "../../secret")
    with pytest.raises(ValueError, match="not found"):
        find_job(tmp_path, "missing")
