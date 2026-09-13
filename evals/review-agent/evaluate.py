"""Score review-agent runs without mistaking scripted orchestration for model quality."""
import argparse
import hashlib
import json
import statistics
from pathlib import Path


def fingerprint(question):
    payload = json.dumps(question, ensure_ascii=False, sort_keys=True).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def score(dataset, runs, annotations=None):
    if runs["dataset_version"] != dataset["version"]:
        raise ValueError("Dataset version mismatch")
    records = {item["case_id"]: item for item in runs["cases"]}
    if len(records) != len(runs["cases"]):
        raise ValueError("Duplicate case id")
    labels = {}
    for item in (annotations or {}).get("items", []):
        key = (item["case_id"], item["run_id"], item["question_id"])
        if key in labels:
            raise ValueError("Duplicate annotation")
        labels[key] = item
    rows, template, factual, grounded = [], [], [], []
    used_labels = set()
    for case in dataset["cases"]:
        record = records.get(case["id"], {})
        run = record.get("run", {})
        questions = run.get("questions", [])
        sources = {s["id"]: s for s in run.get("sources", [])}
        tools = record.get("tool_results", [])
        successful = {t["name"] for t in tools if t.get("ok") is True}
        required = {"get_learning_progress", "search_textbook", "save_review_question", "record_quiz_result"}
        if case["scenario"] != "empty_all":
            required.add("read_source")
        search_count = sum(t["name"] == "search_textbook" for t in tools)
        checks = {
            "completed": run.get("state") == "completed" and bool(questions),
            "tools_used": required <= successful and search_count >= case["min_searches"],
            "actual_answers": bool(questions) and all(
                q.get("selected_index") == case["selected"] % len(q["options"])
                and type(q.get("correct")) is bool
                and q["correct"] == (q["selected_index"] == q.get("correct_index"))
                for q in questions
            ),
            "source_ids_valid": all(set(q["source_ids"]) <= sources.keys() for q in questions),
            "scope_valid": run.get("scope", {}).get("kb") == "book"
            and run.get("scope", {}).get("version") == "v1"
            and all(s.get("chapter_path", [])[:1] == [dataset["reference"]["chapter"]]
                    for s in sources.values()),
            "unique_questions": len({q["id"] for q in questions}) == len(questions)
            and len({(q["question"].strip(), tuple(q["options"])) for q in questions}) == len(questions),
        }
        if case["scenario"] == "empty_all":
            checks["no_fabricated_citations"] = not sources and all(not q["source_ids"] for q in questions)
        rows.append({"case_id": case["id"], "passed": all(checks.values()), "checks": checks,
                     "state": run.get("state", "missing"), "elapsed_ms": record.get("elapsed_ms"),
                     "model_calls": run.get("model_calls", 0), "tool_calls": run.get("tool_calls", 0)})
        for question in questions:
            key = (case["id"], run["id"], question["id"])
            digest = fingerprint(question)
            template.append({"case_id": key[0], "run_id": key[1], "question_id": key[2],
                             "fingerprint": digest, "rater": "", "factual_correct": None,
                             "grounded": None, "notes": ""})
            label = labels.get(key)
            if label is None:
                continue
            used_labels.add(key)
            if label.get("fingerprint") != digest:
                raise ValueError("Stale annotation fingerprint")
            for field in ("factual_correct", "grounded"):
                if label.get(field) is not None and type(label[field]) is not bool:
                    raise ValueError("Annotation must be boolean or null")
            if (any(label.get(field) is not None for field in ("factual_correct", "grounded"))
                    and not label.get("rater", "").strip()):
                raise ValueError("A named reviewer is required")
            if label.get("factual_correct") is not None:
                factual.append(label["factual_correct"])
            if label.get("grounded") is not None:
                if not question["source_ids"]:
                    raise ValueError("Grounding is not applicable to an uncited question")
                grounded.append(label["grounded"])
    if used_labels != labels.keys():
        raise ValueError("Annotation does not match this run")
    latencies = sorted(row["elapsed_ms"] for row in rows if row["elapsed_ms"] is not None)
    total_questions = len(template)
    cited = sum(bool(q["source_ids"]) for item in runs["cases"] for q in item["run"]["questions"])
    usages = [item["trace"]["reported_total_tokens"] for item in runs["cases"]
              if item.get("trace", {}).get("reported_total_tokens") is not None]
    report = {
        "dataset_version": dataset["version"], "prompt_version": runs["prompt_version"],
        "mode": runs["mode"], "run_created_at": runs["created_at"],
        "limitations": "受控微型教材场景，非真实 PDF 检索基准。scripted 仅验证编排；人工标注指标只适用于已评审输出。",
        "flow_passed": sum(row["passed"] for row in rows), "flow_total": len(rows),
        "question_count": total_questions, "cited_question_count": cited,
        "factual_reviewed": len(factual), "factual_accuracy": statistics.mean(factual) if factual else None,
        "grounding_reviewed": len(grounded), "grounding_accuracy": statistics.mean(grounded) if grounded else None,
        "latency_median_ms": statistics.median(latencies) if latencies else None,
        "latency_p95_ms": latencies[max(0, (95 * len(latencies) + 99) // 100 - 1)] if latencies else None,
        "model_calls": sum(row["model_calls"] for row in rows),
        "tool_calls": sum(row["tool_calls"] for row in rows), "cases": rows,
        "models": sorted({item["run"].get("model", "unknown") for item in runs["cases"]}),
        "reported_total_tokens": sum(usages) if usages else None,
        "usage_reported_requests": sum(item.get("trace", {}).get("usage_reported_requests", 0) for item in runs["cases"]),
    }
    return report, {"reference": dataset["reference"], "items": template}


def markdown(report):
    lines = ["# 复习 Agent 评测报告", "", f"执行模式：`{report['mode']}`。",
             report["limitations"], "", f"数据集：{report['dataset_version']}；提示词：{report['prompt_version']}。",
             f"模型配置：{', '.join(report['models'])}（scripted 使用模拟回复）。",
             f"流程通过：{report['flow_passed']} / {report['flow_total']}。",
             f"题目：{report['question_count']}，带原文编号：{report['cited_question_count']}。", ""]
    for title, field, reviewed in [("事实正确率", "factual_accuracy", "factual_reviewed"),
                                   ("引用支持率", "grounding_accuracy", "grounding_reviewed")]:
        value = report[field]
        lines.append(f"- {title}：" + ("未评审" if value is None else f"{value:.1%}")
                     + f"（人工评审 {report[reviewed]} 题）")
    lines += ["", f"模型调用 {report['model_calls']} 次，工具调用 {report['tool_calls']} 次。",
              f"已报告用量：{report['reported_total_tokens']} tokens，覆盖 {report['usage_reported_requests']} 次请求（scripted 用量为模拟值）。",
              f"场景耗时中位数 {report['latency_median_ms']} ms，P95 {report['latency_p95_ms']} ms。",
              "耗时包含本地持久化、检索及答题模拟；scripted 耗时不代表线上模型延迟。", "",
              "| 场景 | 结果 | 未通过检查 |", "| --- | --- | --- |"]
    for row in report["cases"]:
        failures = ", ".join(k for k, v in row["checks"].items() if not v) or "—"
        lines.append(f"| {row['case_id']} | {'通过' if row['passed'] else '未通过'} | {failures} |")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=Path, required=True)
    parser.add_argument("--annotations", type=Path)
    parser.add_argument("--out", type=Path, default=Path("tmp/review-agent-eval"))
    args = parser.parse_args()
    def read(path):
        return json.loads(path.read_text(encoding="utf-8"))
    report, template = score(read(Path(__file__).with_name("cases.json")), read(args.runs),
                             read(args.annotations) if args.annotations else None)
    args.out.mkdir(parents=True, exist_ok=True)
    for name, value in [("report.json", report), ("annotations-template.json", template)]:
        (args.out / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (args.out / "report.md").write_text(markdown(report), encoding="utf-8")
    print(f"Flow: {report['flow_passed']}/{report['flow_total']}; mode={report['mode']}")
    return 0 if report["flow_passed"] == report["flow_total"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
