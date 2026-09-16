import hashlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import benchmark as b


class BenchmarkTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "bank"
        self.data = Path(self.temp.name) / "data"
        self.draft = b.seed()
        self.draft["dataset"]["cases"] = []
        b.write(self.root / "draft.json", self.draft)
        self.blocks = {"block-1": {"text": "测试原文", "page": 9, "eligible": False}}
        self.source = patch.object(
            b, "load_source", return_value=(None, None, self.blocks)
        )
        self.source.start()
        self.addCleanup(self.source.stop)

    def case(self, cid="new1", split="development"):
        return {
            "id": cid,
            "question": "测试问题 " + cid,
            "split": split,
            "kind": "single",
            "reference_answer": "测试参考答案",
            "acceptance_criteria": "必须覆盖原文中的两个要点",
            "expected_behavior": "answer",
            "origin": "manual",
            "evidence": [
                {
                    "block_id": "block-1",
                    "page": 9,
                    "quote": "测试原文",
                    "text_sha256": hashlib.sha256("测试原文".encode()).hexdigest(),
                    "eligible_at_annotation": False,
                }
            ],
            "review": {
                "method": "human",
                "human_reviewer": "测试署名（不是真实人工评价）",
                "notes": "仅测试保存与校验",
                "date": None,
            },
        }

    def save(self, case, confirmed=True):
        return b.save_case(
            self.root,
            {
                "revision": b.bank(self.root)["revision"],
                "case": case,
                "confirm_human": confirmed,
            },
        )

    def freeze(self):
        return b.freeze(
            self.root, self.data, {"revision": b.bank(self.root)["revision"]}
        )["releases"][0]["id"]

    def prepare(self, rid, split="development", **extra):
        output = Path(self.temp.name) / ("run-" + str(len(b.ledger(self.root))))
        return b.prepare(
            self.root,
            self.data,
            output,
            {"release": rid, "split": split, "kind": "model", **extra},
        )

    def test_seed_is_unreviewed_and_cannot_relabel_historical_cases_as_holdout(self):
        seed = b.seed()
        self.assertEqual(len(seed["dataset"]["cases"]), 40)
        self.assertTrue(
            all(
                c["review"]["method"] == "unreviewed" and c["split"] == "regression"
                for c in seed["dataset"]["cases"]
            )
        )
        old = seed["dataset"]["cases"][0]
        old.update(id="renamed", split="holdout")
        with self.assertRaisesRegex(ValueError, "历史题目"):
            self.save(old, False)

    def test_confirmation_is_explicit_and_edit_invalidates_previous_review(self):
        case = self.case()
        self.save(case, False)
        self.assertEqual(
            b.bank(self.root)["dataset"]["cases"][0]["review"]["method"], "unreviewed"
        )
        with self.assertRaisesRegex(ValueError, "至少一道"):
            self.freeze()
        self.save(case)
        case["reference_answer"] = "修改后的参考答案"
        self.save(case, False)
        self.assertEqual(
            b.bank(self.root)["dataset"]["cases"][0]["review"]["method"], "unreviewed"
        )
        with self.assertRaisesRegex(ValueError, "已被修改"):
            b.save_case(self.root, {"revision": 0, "case": case})

    def test_incomplete_label_draft_preserves_reviewer_but_cannot_freeze(self):
        case = self.case()
        case.update(reference_answer="", acceptance_criteria="", evidence=[])
        self.save(case, False)
        saved = b.bank(self.root)["dataset"]["cases"][0]
        self.assertEqual(
            saved["review"]["human_reviewer"], case["review"]["human_reviewer"]
        )
        self.assertEqual(saved["evidence"], [])
        with self.assertRaises(ValueError):
            self.save(case, True)
        with self.assertRaises(ValueError):
            self.freeze()

    def test_freeze_only_confirmed_labels_and_changes_never_change_release(self):
        case = self.case()
        self.save(case)
        self.save(self.case("pending"), False)
        rid = self.freeze()
        self.assertEqual(len(b.release(self.root, rid)["cases"]), 1)
        case["reference_answer"] = "修改后的答案"
        self.save(case)
        self.assertEqual(
            b.release(self.root, rid)["cases"][0]["reference_answer"], "测试参考答案"
        )
        self.blocks["block-1"]["text"] = "变化的原文"
        with self.assertRaises(ValueError):
            self.freeze()

    def test_tampered_release_is_rejected(self):
        self.save(self.case())
        rid = self.freeze()
        path = self.root / "releases" / rid / "dataset.json"
        changed = b.read(path)
        changed["cases"][0]["reference_answer"] = "被改写"
        b.write(path, changed)
        with self.assertRaisesRegex(ValueError, "指纹"):
            self.prepare(rid)

    def test_holdout_is_explicit_and_reuse_tracked_across_versions(self):
        self.save(self.case())
        held = self.case("held", "holdout")
        self.save(held)
        rid = self.freeze()
        selection = self.prepare(rid)
        self.assertEqual(selection["case_ids"], ["new1"])
        with self.assertRaisesRegex(ValueError, "方案已固定"):
            self.prepare(rid, "holdout")
        first = self.prepare(rid, "holdout", acknowledge_holdout=True)
        self.assertEqual(first["holdout_status"], "first_use_declared")
        self.assertEqual(first["used_before"], 0)
        repeated = self.prepare(self.freeze(), "holdout", acknowledge_holdout=True)
        self.assertEqual(repeated["used_before"], 1)
        self.assertEqual(repeated["holdout_status"], "reused")

    def test_model_batches_are_explicit_and_never_silently_truncate(self):
        for i in range(9):
            self.save(self.case("q" + str(i)))
        rid = self.freeze()
        with self.assertRaisesRegex(ValueError, "最多 8 题"):
            self.prepare(rid)
        for ids in (["unknown"], ["q1", "q1"]):
            with self.assertRaises(ValueError):
                self.prepare(rid, case_ids=ids)
        self.assertEqual(
            self.prepare(rid, case_ids=["q7", "q8"])["case_ids"], ["q7", "q8"]
        )

    def test_missing_reviewer_and_duplicate_questions_rejected(self):
        case = self.case()
        case["review"]["human_reviewer"] = " "
        with self.assertRaises(ValueError):
            self.save(case)
        self.save(self.case())
        duplicate = self.case()
        duplicate["id"] = "duplicate"
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            self.save(duplicate)

    def test_switching_pdf_preserves_drafts_and_does_not_transplant_labels(self):
        self.save(self.case())
        original = b.bank(self.root)["dataset"]
        fields = (
            "kb",
            "knowledge_version",
            "source_sha256",
            "source_filename",
            "source_pages",
        )
        old_source = {key: original[key] for key in fields}
        other_source = dict(old_source, kb="other-pdf", source_sha256="a" * 64)
        with patch.object(b, "sources", return_value=[old_source, other_source]):
            other = b.bind_source(
                self.root, self.data, {"kb": "other-pdf", "revision": 1}
            )
            self.assertEqual(other["dataset"]["cases"], [])
            restored = b.bind_source(
                self.root, self.data, {"kb": original["kb"], "revision": 2}
            )
            self.assertEqual(restored["dataset"]["cases"][0]["id"], "new1")
            self.assertEqual(restored["revision"], 3)


if __name__ == "__main__":
    unittest.main()
