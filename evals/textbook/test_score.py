import copy
import unittest

from dataset import digest
from score import review_template, score


class ScoreTests(unittest.TestCase):
    def setUp(self):
        self.dataset = {
            "cases": [{"id": "a", "question": "q", "reference_answer": "r"}]
        }
        self.retrieval = {
            "dataset_sha256": digest(self.dataset),
            "source_gaps": [],
            "metrics": {},
            "records": [
                {
                    "id": "a",
                    "mode": "hybrid",
                    "packet": {"evidence": [{"id": "E1", "text": "source"}]},
                }
            ],
        }
        self.runs = {
            "binding": {"dataset_sha256": digest(self.dataset), "ids": ["a", "b"]},
            "cases": [
                {
                    "id": "a",
                    "state": "completed",
                    "draft": {
                        "status": "answered",
                        "answer": [
                            {
                                "text": "incorrect but well-cited",
                                "citations": [{"evidence_id": "E1", "quote": "source"}],
                            }
                        ],
                        "explanation": [],
                    },
                }
            ],
        }

    def test_no_review_is_not_perfect_quality_and_missing_is_pending(self):
        result = score(self.dataset, self.retrieval, self.runs)
        self.assertIsNone(result["quality_reviews"]["human"])
        self.assertIsNone(result["generation"]["reported_total_tokens"])
        self.assertEqual(result["generation"]["pending_or_interrupted"], 1)
        self.assertEqual(result["generation"]["citation_substring_validity"], 1)

    def test_bound_reviews_and_separate_methods(self):
        review = review_template(self.dataset, self.runs)
        row = review["rows"][0]
        row.update(
            method="ai",
            reviewer="test judge",
            correct=False,
            grounded=False,
            notes="Unsupported claim",
        )
        result = score(self.dataset, self.retrieval, self.runs, annotations=review)
        self.assertEqual(result["quality_reviews"]["ai"]["correct"], 0)
        self.assertIsNone(result["quality_reviews"]["human"])
        changed = copy.deepcopy(self.runs)
        changed["cases"][0]["state"] = "failed"
        with self.assertRaises(ValueError):
            score(self.dataset, self.retrieval, changed, annotations=review)

    def test_duplicate_and_unattributed_reviews_rejected(self):
        review = review_template(self.dataset, self.runs)
        review["rows"] *= 2
        with self.assertRaises(ValueError):
            score(self.dataset, self.retrieval, self.runs, annotations=review)
        review = review_template(self.dataset, self.runs)
        review["rows"][0]["method"] = "human"
        with self.assertRaises(ValueError):
            score(self.dataset, self.retrieval, self.runs, annotations=review)

    def test_stale_dataset_rejected(self):
        self.retrieval["dataset_sha256"] = "old"
        with self.assertRaises(ValueError):
            score(self.dataset, self.retrieval)

    def test_same_labels_do_not_allow_a_different_retrieval_run(self):
        self.runs["binding"]["input_sha256"] = "original"
        with self.assertRaisesRegex(ValueError, "different retrieval"):
            score(self.dataset, self.retrieval, self.runs, retrieval_sha256="changed")

    def test_missing_quote_is_invalid(self):
        self.runs["cases"][0]["draft"]["answer"][0]["citations"][0]["quote"] = ""
        self.assertEqual(
            score(self.dataset, self.retrieval, self.runs)["generation"][
                "citation_substring_validity"
            ],
            0,
        )

    def test_desktop_human_review_format_keeps_drafts_and_ai_separate(self):
        annotations = review_template(self.dataset, self.runs)
        annotations.update(schema_version=1, report_sha256="desktop-report-hash")
        row = annotations["rows"][0]
        row.update(
            method=None,
            reviewer="本机用户",
            correct=True,
            complete=None,
            grounded=True,
            notes="尚未判断完整性",
            revision=1,
            updated_at="2026-09-13T00:00:00Z",
        )
        result = score(self.dataset, self.retrieval, self.runs, annotations=annotations)
        self.assertIsNone(result["quality_reviews"]["human"])
        row.update(method="human", complete=False)
        result = score(self.dataset, self.retrieval, self.runs, annotations=annotations)
        self.assertEqual(
            result["quality_reviews"]["human"],
            {
                "reviewed": 1,
                "planned": 2,
                "correct": 1,
                "grounded": 1,
                "complete": 0,
                "complete_reviewed": 1,
            },
        )
        self.assertIsNone(result["quality_reviews"]["ai"])
        row["complete"] = "false"
        with self.assertRaisesRegex(ValueError, "Completeness"):
            score(self.dataset, self.retrieval, self.runs, annotations=annotations)

    def test_legacy_reviews_do_not_invent_completeness_scores(self):
        annotations = review_template(self.dataset, self.runs)
        row = annotations["rows"][0]
        row.pop("complete")
        row.update(
            method="human",
            reviewer="reviewer",
            correct=True,
            grounded=True,
            notes="旧版复核",
        )
        result = score(self.dataset, self.retrieval, self.runs, annotations=annotations)
        self.assertEqual(result["quality_reviews"]["human"]["correct"], 1)
        self.assertIsNone(result["quality_reviews"]["human"]["complete"])
        self.assertEqual(result["quality_reviews"]["human"]["complete_reviewed"], 0)
