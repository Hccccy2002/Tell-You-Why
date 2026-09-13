"""Non-exhaustive source labels: these metrics are coverage, not factual accuracy."""

import statistics


def coverage(case, results, packet):
    gold = {e["block_id"] for e in case["evidence"]}
    ranks = [{loc["block_id"] for loc in r["locations"]} for r in results]
    found = set().union(*ranks) if ranks else set()
    evidence = {e["block_id"] for e in packet["evidence"]}
    return {
        "hit_at_5": bool(gold & found) if gold else None,
        "all_at_5": gold <= found if gold else None,
        "recall_at_5": len(gold & found) / len(gold) if gold else None,
        "mrr_at_5": next((1 / i for i, ids in enumerate(ranks, 1) if ids & gold), 0)
        if gold
        else None,
        "evidence_recall": len(gold & evidence) / len(gold) if gold else None,
        "all_evidence": gold <= evidence if gold else None,
        "missing_blocks": sorted(gold - evidence),
    }


def summarize(records):
    result = {}
    for mode in sorted({r["mode"] for r in records}):
        for split in sorted({r["split"] for r in records}):
            rows = [r for r in records if r["mode"] == mode and r["split"] == split]
            if not rows:
                continue
            metrics = {}
            for name in (
                "hit_at_5",
                "all_at_5",
                "recall_at_5",
                "mrr_at_5",
                "evidence_recall",
                "all_evidence",
            ):
                values = [
                    r["metrics"][name] for r in rows if r["metrics"][name] is not None
                ]
                metrics[name] = statistics.mean(values) if values else None
            times = sorted(r["search_ms"] for r in rows)
            result[f"{mode}/{split}"] = {
                **metrics,
                "cases": len(rows),
                "supported_cases": sum(
                    r["metrics"]["hit_at_5"] is not None for r in rows
                ),
                "search_median_ms": statistics.median(times),
                "search_p95_ms": times[min(len(times) - 1, int(len(times) * 0.95))],
            }
    return result
