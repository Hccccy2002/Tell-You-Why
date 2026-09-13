import copy
import hashlib
import json
import unittest
from pathlib import Path

from dataset import digest, validate


class DatasetTests(unittest.TestCase):
    def setUp(self):
        self.dataset = json.loads(
            Path(__file__).with_name("dataset.json").read_text(encoding="utf-8")
        )

    def test_frozen_labels(self):
        lock = json.loads(Path(__file__).with_name("dataset-lock.json").read_text())
        self.assertEqual(digest(self.dataset), lock["sha256"])
        report = validate(self.dataset)
        self.assertEqual(report["case_count"], 40)
        self.assertEqual(report["human_reviewed"], 0)
        self.assertEqual(
            {c["split"] for c in self.dataset["cases"]},
            {"development", "validation", "regression"},
        )

    def test_duplicate_question_and_id(self):
        for field in ("id", "question"):
            changed = copy.deepcopy(self.dataset)
            changed["cases"][1][field] = changed["cases"][0][field]
            with self.assertRaises(ValueError):
                validate(changed)

    def test_multi_and_negative_requirements(self):
        for kind in ("multi", "absent", "unsupported_visual"):
            changed = copy.deepcopy(self.dataset)
            changed["cases"][0]["kind"] = kind
            with self.assertRaises(ValueError):
                validate(changed)

    def test_no_fake_human_review(self):
        self.dataset["cases"][0]["review"]["method"] = "human"
        with self.assertRaisesRegex(ValueError, "attribution"):
            validate(self.dataset)

    def test_source_validation_and_excluded_blocks(self):
        case = self.dataset["cases"][0]
        self.dataset["cases"] = [case]
        evidence = case["evidence"][0]
        text = evidence["quote"]
        evidence["text_sha256"] = hashlib.sha256(text.encode()).hexdigest()
        block = {"text": text, "page": evidence["page"], "eligible": False}
        blocks = {evidence["block_id"]: block}
        self.assertEqual(len(validate(self.dataset, blocks)["source_gaps"]), 1)
        for update in (
            {"page": 999},
            {"text": "unrelated"},
            {"text": text + "changed"},
        ):
            with self.assertRaises(ValueError):
                validate(self.dataset, {evidence["block_id"]: dict(block, **update)})
        with self.assertRaises(ValueError):
            validate(self.dataset, {})


if __name__ == "__main__":
    unittest.main()
