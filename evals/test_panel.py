import unittest
from pathlib import Path

from panel import review_report, top5_report

ROOT = Path(__file__).resolve().parent


class PanelTests(unittest.TestCase):
    def test_review_uses_real_scorer_and_retains_failure_details(self):
        runs = {
            "dataset_version": "review-eval-v1",
            "prompt_version": "test",
            "mode": "scripted",
            "created_at": "2026-09-13",
            "cases": [
                {
                    "case_id": "basic-correct",
                    "elapsed_ms": 1,
                    "run": {
                        "id": "r1",
                        "state": "completed",
                        "scope": {"kb": "book", "version": "v1"},
                        "sources": [],
                        "questions": [
                            {
                                "id": "q1",
                                "question": "存储器的作用？",
                                "options": ["程序和数据", "图片"],
                                "selected_index": 0,
                                "correct_index": 0,
                                "correct": True,
                                "source_ids": [],
                            }
                        ],
                    },
                    "tool_results": [
                        {"name": name, "ok": True}
                        for name in [
                            "get_learning_progress",
                            "search_textbook",
                            "read_source",
                            "save_review_question",
                            "record_quiz_result",
                        ]
                    ],
                }
            ],
        }
        report = review_report(runs)
        self.assertEqual(report["metrics"][0]["value"], "1 / 10")
        runs["cases"][0]["run"]["state"] = "failed"
        report = review_report(runs)
        self.assertEqual(report["metrics"][0]["value"], "0 / 10")
        self.assertEqual(report["rows"][0]["status"], "failed")
        self.assertFalse(report["rows"][0]["details"]["checks"]["completed"])
        self.assertIsNone(report["raw"]["report"]["factual_accuracy"])

    def test_top5_reports_denominator_and_does_not_call_unlabelled_success(self):
        raw = {
            "records": [
                {
                    "id": str(i),
                    "question": "测试题",
                    "mode": mode,
                    "sources": [],
                    "search_ms": 100,
                    "metrics": {
                        "hit_at_5": hit,
                        "recall_at_5": None if hit is None else float(hit),
                        "mrr_at_5": None if hit is None else float(hit),
                    },
                }
                for mode, hits in [
                    ("baseline_top5", [True, False, None]),
                    ("reranked_top5", [True, True, None]),
                ]
                for i, hit in enumerate(hits)
            ]
        }
        report = top5_report(raw)
        self.assertEqual(report["metrics"][0]["value"], "2 / 3")
        self.assertEqual(report["metrics"][1]["value"], "100.0%")
        self.assertEqual(report["metrics"][1]["baseline"], "50.0%")
        self.assertEqual(sum(r["status"] == "unreviewed" for r in report["rows"]), 1)
        self.assertEqual(len(report["rows"]), 3)


if __name__ == "__main__":
    unittest.main()
