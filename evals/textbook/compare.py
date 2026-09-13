"""Compare fixed cohorts. Reject label, source, index or query-model drift."""

import argparse
import json
from pathlib import Path


def compare(before, after, mode="hybrid"):
    for pin in (
        "dataset_sha256",
        "source_sha256",
        "knowledge_version",
        "embedding_index_sha256",
        "embedding_model_manifest_sha256",
    ):
        if not before.get(pin) or before[pin] != after.get(pin):
            raise ValueError(f"Experiment pin changed: {pin}")

    def keyed(report):
        rows = [r for r in report["records"] if r["mode"] == mode]
        keyed_rows = {(r["id"], r["split"]): r for r in rows}
        if len(keyed_rows) != len(rows):
            raise ValueError("Duplicate comparison case")
        return keyed_rows

    old, new = keyed(before), keyed(after)
    if not old or old.keys() != new.keys():
        raise ValueError("Comparison cohorts differ")
    changes = []
    for key, row in old.items():
        for metric in ("recall_at_5", "evidence_recall"):
            left, right = row["metrics"][metric], new[key]["metrics"][metric]
            if (left is None) != (right is None):
                raise ValueError("Metric denominator changed")
            if left != right:
                changes.append(
                    {
                        "id": key[0],
                        "split": key[1],
                        "metric": metric,
                        "before": left,
                        "after": right,
                    }
                )
    return {
        "dataset_sha256": before["dataset_sha256"],
        "compared_cases": len(old),
        "mode": mode,
        "changes": changes,
        "regressions": [r for r in changes if r["after"] < r["before"]],
        "before": {
            k: v for k, v in before["metrics"].items() if k.startswith(mode + "/")
        },
        "after": {
            k: v for k, v in after["metrics"].items() if k.startswith(mode + "/")
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    report = compare(
        json.loads(args.before.read_text(encoding="utf-8")),
        json.loads(args.after.read_text(encoding="utf-8")),
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "compared_cases": report["compared_cases"],
                "changes": report["changes"],
                "regressions": report["regressions"],
            }
        )
    )
    if report["regressions"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
