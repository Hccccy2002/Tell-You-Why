import unittest

from metrics import coverage, summarize


class MetricsTests(unittest.TestCase):
    def test_multi_source_and_duplicate_hits(self):
        case = {"evidence": [{"block_id": "a"}, {"block_id": "b"}]}
        result = [{"locations": [{"block_id": "a"}, {"block_id": "a"}]}]
        score = coverage(case, result, {"evidence": [{"block_id": "b"}]})
        self.assertTrue(score["hit_at_5"])
        self.assertFalse(score["all_at_5"])
        self.assertEqual(score["recall_at_5"], 0.5)
        self.assertEqual(score["evidence_recall"], 0.5)
        self.assertEqual(score["missing_blocks"], ["a"])

    def test_negative_is_not_a_successful_recall(self):
        score = coverage({"evidence": []}, [], {"evidence": []})
        self.assertIsNone(score["hit_at_5"])
        report = summarize(
            [
                {
                    "mode": "hybrid",
                    "split": "development",
                    "search_ms": 3,
                    "metrics": score,
                }
            ]
        )
        self.assertIsNone(report["hybrid/development"]["recall_at_5"])
        self.assertEqual(report["hybrid/development"]["supported_cases"], 0)

    def test_rank_miss_and_empty_results(self):
        case = {"evidence": [{"block_id": "b"}]}
        rows = [{"locations": [{"block_id": b}]} for b in ("a", "b")]
        self.assertEqual(coverage(case, rows, {"evidence": []})["mrr_at_5"], 0.5)
        self.assertEqual(coverage(case, [], {"evidence": []})["mrr_at_5"], 0)
