"""Report actual coverage and execution, with fingerprint-bound optional quality reviews."""

import argparse
import hashlib
import json
import statistics
from pathlib import Path

from dataset import digest


def review_template(dataset, runs):
    cases = {c["id"]: c for c in dataset["cases"]}
    return {
        "dataset_sha256": digest(dataset),
        "rows": [
            {
                "id": r["id"],
                "result_sha256": digest(r),
                "question": cases[r["id"]]["question"],
                "reference_answer": cases[r["id"]]["reference_answer"],
                "method": None,
                "reviewer": None,
                "correct": None,
                "complete": None,
                "grounded": None,
                **(
                    {"behavior_appropriate": None}
                    if dataset.get("schema_version") == 2
                    else {}
                ),
                "notes": "",
            }
            for r in runs["cases"]
        ],
    }


def score(
    dataset,
    retrieval,
    runs=None,
    requests=None,
    annotations=None,
    retrieval_sha256=None,
):
    if retrieval["dataset_sha256"] != digest(dataset):
        raise ValueError("Retrieval labels changed")
    result = {
        "dataset_sha256": digest(dataset),
        "retrieval": retrieval["metrics"],
        "source_gaps": retrieval["source_gaps"],
        "generation": None,
        "agent": None,
        "quality_reviews": {"human": None, "ai": None},
        "experiment": {
            key: retrieval.get(key)
            for key in (
                "source_sha256",
                "knowledge_version",
                "embedding_index_sha256",
                "embedding_model_manifest_sha256",
            )
        },
    }
    if runs is None:
        return result
    if runs["binding"]["dataset_sha256"] != digest(dataset):
        raise ValueError("Generation labels changed")
    if (
        retrieval_sha256 is not None
        and runs["binding"].get("input_sha256") != retrieval_sha256
    ):
        raise ValueError("Generation used a different retrieval experiment")
    result["experiment"]["generation_binding"] = runs["binding"]
    ids = runs["binding"]["ids"]
    rows = {r["id"]: r for r in runs["cases"]}
    if len(rows) != len(runs["cases"]) or not set(rows) <= set(ids):
        raise ValueError("Duplicate or unexpected generation cases")
    completed = [r for r in rows.values() if r["state"] == "completed"]
    answered = [r for r in completed if r["draft"]["status"] == "answered"]
    # Citation existence is structural, not semantic entailment.
    packets = {
        r["id"]: r["packet"] for r in retrieval["records"] if r["mode"] == "hybrid"
    }
    citations = []
    for row in answered:
        evidence = {e["id"]: e["text"] for e in packets[row["id"]]["evidence"]}
        for claim in row["draft"]["answer"] + row["draft"]["explanation"]:
            citations.extend(
                bool(c.get("quote"))
                and c["quote"] in evidence.get(c["evidence_id"], "")
                for c in claim["citations"]
            )
    requests = requests or []
    usage = [
        r["usage"]["total_tokens"]
        for r in requests
        if isinstance(r.get("usage", {}).get("total_tokens"), int)
    ]
    times = [r["duration_ms"] for r in rows.values() if "duration_ms" in r]
    result["generation"] = {
        "planned": len(ids),
        "recorded": len(rows),
        "completed": len(completed),
        "answered": len(answered),
        "insufficient": len(completed) - len(answered),
        "failed": sum(r["state"] == "failed" for r in rows.values()),
        "pending_or_interrupted": len(ids)
        - sum(r["state"] in {"completed", "failed"} for r in rows.values()),
        "citation_substring_validity": statistics.mean(citations)
        if citations
        else None,
        "duration_median_ms": statistics.median(times) if times else None,
        "requests": len(requests),
        "requests_with_token_usage": len(usage),
        "reported_total_tokens": sum(usage) if usage else None,
    }
    if runs.get("agent"):
        agent = runs["agent"]
        questions = agent.get("run", {}).get("questions", [])
        result["agent"] = {
            "state": agent["state"],
            "completed": agent["state"] == "completed",
            "saved_questions": len(questions),
            "graded_questions": sum(q.get("correct") is not None for q in questions),
            "duration_ms": agent.get("duration_ms"),
            "model_requests": agent.get("trace", {}).get("model_requests"),
            "note": "脚本选择固定选项，完成率不代表真实用户掌握或题目正确率。",
        }
    if annotations is not None:
        if annotations["dataset_sha256"] != digest(dataset):
            raise ValueError("Review dataset changed")
        seen, accepted = set(), {"human": [], "ai": []}
        for annotation in annotations["rows"]:
            cid = annotation["id"]
            if (
                cid in seen
                or cid not in rows
                or annotation["result_sha256"] != digest(rows[cid])
            ):
                raise ValueError("Stale, duplicate or unknown review")
            seen.add(cid)
            method = annotation["method"]
            if method is None:
                continue
            if rows[cid]["state"] != "completed":
                raise ValueError("Cannot review an unfinished or failed generation")
            if (
                method not in accepted
                or not annotation["reviewer"]
                or not annotation["notes"].strip()
            ):
                raise ValueError("Review requires method, reviewer and rationale")
            if (
                type(annotation["correct"]) is not bool
                or type(annotation["grounded"]) is not bool
            ):
                raise ValueError("Review needs explicit boolean judgments")
            if (
                annotation.get("complete") is not None
                and type(annotation["complete"]) is not bool
            ):
                raise ValueError("Completeness review must be a boolean")
            if dataset.get("schema_version") == 2 and (
                type(annotation.get("complete")) is not bool
                or type(annotation.get("behavior_appropriate")) is not bool
            ):
                raise ValueError("Human benchmark requires all four explicit judgments")
            accepted[method].append(annotation)
        for method, reviews in accepted.items():
            if reviews:
                completeness = [
                    r["complete"] for r in reviews if type(r.get("complete")) is bool
                ]
                result["quality_reviews"][method] = {
                    "reviewed": len(reviews),
                    "planned": len(ids),
                    "correct": statistics.mean(r["correct"] for r in reviews),
                    "grounded": statistics.mean(r["grounded"] for r in reviews),
                    "complete": statistics.mean(completeness) if completeness else None,
                    "complete_reviewed": len(completeness),
                }
                if dataset.get("schema_version") == 2:
                    cases = {c["id"]: c for c in dataset["cases"]}
                    refusal_reviews = [
                        r
                        for r in reviews
                        if cases[r["id"]]["expected_behavior"] == "refuse"
                    ]
                    groups = []
                    eligible = [r["id"] for r in completed]
                    for field in ("kind", "split"):
                        for name in sorted({cases[cid][field] for cid in eligible}):
                            members = [
                                r for r in reviews if cases[r["id"]][field] == name
                            ]
                            passed = sum(
                                all(
                                    r[key]
                                    for key in (
                                        "correct",
                                        "complete",
                                        "grounded",
                                        "behavior_appropriate",
                                    )
                                )
                                for r in members
                            )
                            groups.append(
                                {
                                    "dimension": field,
                                    "name": name,
                                    "reviewed": len(members),
                                    "total": sum(
                                        cases[cid][field] == name for cid in eligible
                                    ),
                                    "passed": passed,
                                    "pass_rate": passed / len(members)
                                    if members
                                    else None,
                                }
                            )
                    result["quality_reviews"][method].update(
                        behavior_reviewed=len(reviews),
                        behavior_appropriate=statistics.mean(
                            r["behavior_appropriate"] for r in reviews
                        ),
                        refusal_reviewed=len(refusal_reviews),
                        appropriate_refusal=statistics.mean(
                            r["behavior_appropriate"] for r in refusal_reviews
                        )
                        if refusal_reviews
                        else None,
                        groups=groups,
                    )
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dataset", type=Path, default=Path(__file__).with_name("dataset.json")
    )
    parser.add_argument("--retrieval", type=Path, required=True)
    parser.add_argument("--runs", type=Path)
    parser.add_argument("--annotations", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    read = lambda p: json.loads(p.read_text(encoding="utf-8")) if p else None
    dataset, retrieval, runs = read(args.dataset), read(args.retrieval), read(args.runs)
    requests = read(args.runs.with_name("requests.json")) if args.runs else None
    report = score(
        dataset,
        retrieval,
        runs,
        requests,
        read(args.annotations),
        hashlib.sha256(args.retrieval.read_bytes()).hexdigest(),
    )
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    lines = [
        "# 真实教材评测报告",
        "",
        "引用匹配率只校验定位；人工/AI 复核分开计数，缺失指标保持 null。",
        "",
        "| 模式/分组 | 有证据题 | Hit@5 | 证据召回率 |",
        "|---|---:|---:|---:|",
    ]
    for group, values in report["retrieval"].items():

        def percent(x):
            return f"{x:.1%}" if x is not None else "未评估"

        lines.append(
            f"| {group} | {values['supported_cases']} | {percent(values['hit_at_5'])} | {percent(values['evidence_recall'])} |"
        )
    lines += [
        "",
        "执行及复核结果：",
        "",
        "```json",
        json.dumps(
            {
                k: v
                for k, v in report.items()
                if k in {"generation", "agent", "quality_reviews"}
            },
            ensure_ascii=False,
            indent=2,
        ),
        "```",
        "",
    ]
    (args.out / "report.md").write_text("\n".join(lines), encoding="utf-8")
    template = args.out / "review-template.json"
    if runs and not template.exists():
        template.write_text(
            json.dumps(review_template(dataset, runs), ensure_ascii=False, indent=2)
            + "\n",
            encoding="utf-8",
        )
    print(json.dumps(report["generation"]))


if __name__ == "__main__":
    main()
