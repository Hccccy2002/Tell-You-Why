import copy
import unittest

from compare import compare


class CompareTests(unittest.TestCase):
    def setUp(self):
        self.before = {
            pin: "fixed"
            for pin in (
                "dataset_sha256",
                "source_sha256",
                "knowledge_version",
                "embedding_index_sha256",
                "embedding_model_manifest_sha256",
            )
        }
        self.before.update(
            metrics={},
            records=[
                {
                    "id": "a",
                    "split": "validation",
                    "mode": "hybrid",
                    "metrics": {"recall_at_5": 0.5, "evidence_recall": 0.5},
                }
            ],
        )

    def test_detects_regression_and_improvement(self):
        after = copy.deepcopy(self.before)
        after["records"][0]["metrics"].update(recall_at_5=0, evidence_recall=1)
        report = compare(self.before, after)
        self.assertEqual(len(report["changes"]), 2)
        self.assertEqual(len(report["regressions"]), 1)

    def test_rejects_dataset_drift_and_missing_cases(self):
        for changed in ({"dataset_sha256": "new"}, {"records": []}):
            with self.assertRaises(ValueError):
                compare(self.before, {**self.before, **changed})

    def test_never_changes_negative_denominator(self):
        after = copy.deepcopy(self.before)
        after["records"][0]["metrics"]["recall_at_5"] = None
        with self.assertRaises(ValueError):
            compare(self.before, after)
