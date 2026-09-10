"""Download pinned, explicitly licensed textbook source files; never execute them."""

import concurrent.futures
import datetime
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent
SOURCES = {
    "foxsen/archbase": {
        "commit": "dfc1ea3ffc22b5f09c51cc3af80b25c3ce6ccd6c",
        "files": [
            "LICENSE", "README.md", "index.Rmd", "04-preface.Rmd", "book.bib",
            "11-introduction.Rmd", "12-isa.Rmd", "13-privileged-isa.Rmd",
            "15-organization.Rmd", "16-bus.Rmd", "17-boot.Rmd",
            "18-microarch.Rmd", "19-pipeline.Rmd", "21-multicore.Rmd",
            "22-perf-evaluation.Rmd",
        ],
    },
    "krahets/hello-algo": {
        "commit": "28c1e74c1d3594fca7cc41b78a6bd18b700c5ce9",
        "files": [
            "LICENSE", "README.md",
            "docs/chapter_data_structure/basic_data_types.md",
            "docs/chapter_data_structure/number_encoding.md",
            "docs/chapter_data_structure/character_encoding.md",
            "docs/chapter_array_and_linkedlist/ram_and_cache.md",
        ],
    },
}


def fetch_one(item):
    repo, spec, path = item
    commit = spec["commit"]
    url = f"https://raw.githubusercontent.com/{repo}/{commit}/{urllib.parse.quote(path)}"
    target = ROOT / "raw" / repo.replace("/", "--") / path
    tree = json.loads((ROOT / "provenance" / f"{repo.replace('/', '--')}-tree.json").read_text(encoding="utf-8-sig"))
    expected = next(entry["sha"] for entry in tree["tree"] if entry["path"] == path)
    if target.exists():
        data = target.read_bytes()
        mode = "cached"
    else:
        request = urllib.request.Request(url, headers={"User-Agent": "TellYouWhy-RAG-Corpus/0.1"})
        with urllib.request.urlopen(request, timeout=40) as response:
            data = response.read()
        mode = "downloaded"
    git_blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    if git_blob != expected:
        raise ValueError(f"Git blob hash mismatch: {repo}/{path}")
    data.decode("utf-8-sig")
    target.parent.mkdir(parents=True, exist_ok=True)
    if mode == "downloaded":
        target.write_bytes(data)
    return {
        "repo": repo, "commit": commit, "path": path,
        "raw_url": url, "source_url": f"https://github.com/{repo}/blob/{commit}/{path}",
        "local_path": target.relative_to(ROOT).as_posix(),
        "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
        "git_blob_sha1": git_blob, "verified_against_git_tree": True,
    }


def main():
    jobs = [(repo, spec, path) for repo, spec in SOURCES.items() for path in spec["files"]]
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        rows = list(pool.map(fetch_one, jobs))
    result = {
        "retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "files": sorted(rows, key=lambda row: (row["repo"], row["path"])),
    }
    (ROOT / "provenance" / "downloads.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"verified_files": len(rows), "bytes": sum(row["bytes"] for row in rows)}))


if __name__ == "__main__":
    main()
