import copy
import unittest

from evaluate import score


class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.dataset = {"version": "v1", "reference": {"chapter": "存储器"}, "cases": [
            {"id": "basic", "scenario": "normal", "selected": 1, "min_searches": 1}]}
        self.runs = {"dataset_version": "v1", "prompt_version": "p1", "created_at": "today",
                     "mode": "scripted", "cases": [{"case_id": "basic", "elapsed_ms": 10,
                     "tool_results": [{"name": name, "ok": True} for name in [
                         "get_learning_progress", "search_textbook", "read_source",
                         "save_review_question", "record_quiz_result"]],
                     "run": {"id": "run1", "state": "completed", "scope": {"kb": "book", "version": "v1"},
                             "sources": [{"id": "S1", "chapter_path": ["存储器"]}],
                             "questions": [{"id": "q1", "question": "作用？", "options": ["a", "b"],
                                            "source_ids": ["S1"], "selected_index": 1,
                                            "correct_index": 0, "correct": False}]}}]}

    def test_flow_does_not_claim_accuracy(self):
        report, _ = score(self.dataset, self.runs)
        self.assertEqual(report["flow_passed"], 1)
        self.assertIsNone(report["factual_accuracy"])
        self.assertIsNone(report["grounding_accuracy"])

    def test_missing_case_fails(self):
        self.runs["cases"] = []
        report, _ = score(self.dataset, self.runs)
        self.assertEqual(report["flow_passed"], 0)

    def test_fabricated_citation_and_answer_fail(self):
        question = self.runs["cases"][0]["run"]["questions"][0]
        question["source_ids"] = ["fake"]
        question["selected_index"] = 0
        report, _ = score(self.dataset, self.runs)
        self.assertFalse(report["cases"][0]["checks"]["source_ids_valid"])
        self.assertFalse(report["cases"][0]["checks"]["actual_answers"])

    def test_human_labels_and_coverage(self):
        _, template = score(self.dataset, self.runs)
        label = template["items"][0]
        label.update(rater="reviewer", factual_correct=False, grounded=True)
        report, _ = score(self.dataset, self.runs, template)
        self.assertEqual(report["factual_accuracy"], 0)
        self.assertEqual(report["grounding_accuracy"], 1)
        self.assertEqual(report["factual_reviewed"], 1)
        template["items"][0]["fingerprint"] = "stale"
        with self.assertRaisesRegex(ValueError, "Stale"):
            score(self.dataset, self.runs, template)

    def test_unmatched_annotations_rejected(self):
        _, template = score(self.dataset, self.runs)
        template["items"][0]["run_id"] = "another-run"
        with self.assertRaisesRegex(ValueError, "match"):
            score(self.dataset, self.runs, template)

    def test_scope_and_duplicate_outputs_fail(self):
        run = self.runs["cases"][0]["run"]
        run["sources"][0]["chapter_path"] = ["其他章节"]
        run["questions"].append(copy.deepcopy(run["questions"][0]))
        report, _ = score(self.dataset, self.runs)
        self.assertFalse(report["cases"][0]["checks"]["scope_valid"])
        self.assertFalse(report["cases"][0]["checks"]["unique_questions"])


if __name__ == "__main__":
    unittest.main()
