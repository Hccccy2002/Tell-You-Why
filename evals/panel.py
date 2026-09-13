"""Desktop evaluation adapter. Reuses the CLI scorers and frozen datasets."""

import argparse
import hashlib
import importlib.util
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "textbook"))
MODEL_CASES = ["d01", "d05", "t10", "t13", "x01", "x04", "n01", "n03"]
SCENARIOS = {
    "basic-correct": "答对后的复习流程",
    "basic-wrong": "答错后的巩固流程",
    "requery": "空检索后换词重试",
    "no-evidence": "没有原文时的处理",
    "retrieval-error": "检索故障恢复",
    "scope-isolation": "章节范围隔离",
    "invalid-arguments": "无效工具参数",
    "resume-once": "模型失败后恢复",
    "do-not-answer-for-user": "禁止替用户答题",
    "injected-source": "不可信原文指令",
}


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write(path, value):
    path = Path(path)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def percent(value):
    return "未评估" if value is None else f"{value:.1%}"


def metric(label, value, baseline=None):
    return {"label": label, "value": str(value), "baseline": baseline}


def review_report(runs):
    spec = importlib.util.spec_from_file_location(
        "review_scorer", ROOT / "review-agent/evaluate.py"
    )
    scorer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scorer)
    report, annotations = scorer.score(read(ROOT / "review-agent/cases.json"), runs)
    records = {r["case_id"]: r for r in runs["cases"]}
    return {
        "title": "Agent 流程评测",
        "dataset": report["dataset_version"],
        "notes": [
            "受控模型回复，验证真实 Agent 编排和工具执行；不调用模型 API。",
            "流程通过率不代表模型回答正确率；模拟 Token 和耗时不计入真实模型用量。",
        ],
        "metrics": [
            metric("流程通过", f"{report['flow_passed']} / {report['flow_total']}"),
            metric("工具调用", report["tool_calls"]),
            metric("人工复核", "未评审"),
        ],
        "rows": [
            {
                "id": r["case_id"],
                "title": SCENARIOS.get(r["case_id"], r["case_id"]),
                "status": "unreviewed"
                if r["case_id"] not in records
                else "passed"
                if r["passed"]
                else "failed",
                "note": "未执行"
                if r["case_id"] not in records
                else "检查通过"
                if r["passed"]
                else "未通过：" + "、".join(k for k, v in r["checks"].items() if not v),
                "details": {
                    "checks": r["checks"],
                    "execution": records.get(r["case_id"]),
                },
            }
            for r in report["cases"]
        ],
        "raw": {"report": report, "runs": runs, "annotations_template": annotations},
    }


def top5_report(report):
    groups = {
        mode: [r for r in report["records"] if r["mode"] == mode]
        for mode in ["baseline_top5", "reranked_top5"]
    }
    before, after = groups.values()

    def average(rows, key):
        values = [r["metrics"][key] for r in rows if r["metrics"][key] is not None]
        return statistics.mean(values) if values else None

    baseline = {r["id"]: r for r in before}
    supported = sum(r["metrics"]["hit_at_5"] is not None for r in after)
    metrics = [metric("有原文标签的题目", f"{supported} / {len(after)}")]
    for title, key in [
        ("Top 5 命中率", "hit_at_5"),
        ("原文召回率", "recall_at_5"),
        ("平均倒数排名 MRR@5", "mrr_at_5"),
    ]:
        fmt = (
            (lambda x: "未评估" if x is None else f"{x:.3f}")
            if key == "mrr_at_5"
            else percent
        )
        metrics.append(
            metric(title, fmt(average(after, key)), fmt(average(before, key)))
        )
    metrics.append(
        metric(
            "重排检索中位耗时",
            f"{statistics.median(r['search_ms'] for r in after) / 1000:.2f} 秒"
            if after
            else "未评估",
        )
    )
    return {
        "title": "RAG Top 5 评测",
        "dataset": report.get("dataset_version", report.get("knowledge_version", "")),
        "notes": [
            "对比原检索前 5 条与重排后前 5 条，显示相同数量的原文。",
            "没有原文标签的题目不计入命中率；标签非穷尽，指标不代表事实正确率。",
            "固定教材回归集，用于比较修改前后结果，不代表其他教材上的泛化能力。",
        ],
        "metrics": metrics,
        "rows": [
            {
                "id": r["id"],
                "title": r["question"],
                "status": "unreviewed"
                if r["metrics"]["hit_at_5"] is None
                else "passed"
                if r["metrics"]["hit_at_5"]
                else "failed",
                "note": f"原文 {len(r['sources'])} 条 · 召回率 {percent(r['metrics']['recall_at_5'])} · 原流程 {percent(baseline[r['id']]['metrics']['recall_at_5'])}",
                "details": {"before": baseline[r["id"]], "after": r},
            }
            for r in after
        ],
        "raw": report,
    }


def model_report(dataset, retrieval, runs, requests, input_sha256):
    from score import review_template, score

    report = score(dataset, retrieval, runs, requests, retrieval_sha256=input_sha256)
    generation = report["generation"]
    by_id = {c["id"]: c for c in dataset["cases"]}
    packets = {
        r["id"]: r["packet"] for r in retrieval["records"] if r["mode"] == "hybrid"
    }
    rows = [
        {
            "id": r["id"],
            "title": by_id[r["id"]]["question"],
            "status": "unreviewed" if r["state"] == "completed" else "failed",
            "note": "已生成 · 内容待复核"
            if r["state"] == "completed"
            else r.get("error", "执行未完成"),
            "details": {
                "result": r,
                "reference_answer": by_id[r["id"]]["reference_answer"],
                "evidence": packets[r["id"]]["evidence"],
            },
        }
        for r in runs["cases"]
    ]
    if runs.get("agent"):
        rows.append(
            {
                "id": "agent",
                "title": "真实教材复习 Agent",
                "status": "passed"
                if runs["agent"]["state"] == "completed"
                else "failed",
                "note": "固定选项模拟作答，检查执行流程",
                "details": runs["agent"],
            }
        )
    return {
        "title": "真实模型评测",
        "dataset": dataset["dataset_version"],
        "notes": [
            "使用真实教材与模型 API；模型完成输出不等于内容正确。",
            "回答正确性和证据支持度尚未复核；报告提供逐题答案、原文、执行追踪和复核模板。",
        ],
        "metrics": [
            metric("生成完成", f"{generation['completed']} / {generation['planned']}"),
            metric(
                "引用定位有效率", percent(generation["citation_substring_validity"])
            ),
            metric("实际模型请求", generation["requests"]),
            metric(
                "已报告 Token",
                generation["reported_total_tokens"]
                if generation["reported_total_tokens"] is not None
                else "未知",
            ),
            metric(
                "有用量的请求",
                f"{generation['requests_with_token_usage']} / {generation['requests']}",
            ),
            metric("内容正确率", "待复核"),
        ],
        "rows": rows,
        "raw": {
            "report": report,
            "runs": runs,
            "requests": requests,
            "review_template": review_template(dataset, runs),
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "action", choices=["top5", "prepare-model", "score-review", "score-model"]
    )
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    def progress(done, total, case):
        if (args.out / "cancel").exists():
            raise InterruptedError("评测已停止")
        write(
            args.out / "progress.json",
            {"completed": done, "total": total, "message": f"正在评测 {case}"},
        )

    dataset = read(ROOT / "textbook/dataset.json")
    if args.action == "top5":
        from evaluate_top5 import run

        raw = run(dataset, args.data, progress)
        write(args.out / "report.json", top5_report(raw))
    elif args.action == "prepare-model":
        from retrieve import run

        raw = run(
            dataset,
            args.data,
            ["development", "regression", "validation"],
            ["hybrid"],
            progress,
            MODEL_CASES,
        )
        write(args.out / "retrieval.json", raw)
    elif args.action == "score-review":
        write(args.out / "report.json", review_report(read(args.out / "runs.json")))
    else:
        input_path = args.out / "retrieval.json"
        write(
            args.out / "report.json",
            model_report(
                dataset,
                read(input_path),
                read(args.out / "runs.json"),
                read(args.out / "requests.json"),
                hashlib.sha256(input_path.read_bytes()).hexdigest(),
            ),
        )


if __name__ == "__main__":
    main()
