"""Human-authored labels, immutable releases and an explicit holdout-use ledger.

No inference imports or API calls. Human attribution is a local declaration, not
identity authentication. The existing exposed dataset is never a blind holdout.
"""

import argparse
import copy
import hashlib
import json
import re
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path

from dataset import digest, load_source, normalized, validate


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def now():
    return datetime.now(timezone.utc).isoformat()


def seed():
    dataset = read(Path(__file__).with_name("dataset.json"))
    dataset.update(schema_version=2, dataset_version="human-benchmark-draft")
    dataset["annotation_policy"] = (
        "人工标注工作副本；仅由用户确认的题目可以封存。署名不等于身份认证。"
    )
    for case in dataset["cases"]:
        case.update(
            split="regression",
            origin="legacy_exposed",
            expected_behavior="refuse"
            if case["kind"] in {"absent", "unsupported_visual"}
            else "answer",
            acceptance_criteria=case["reference_answer"],
            review={
                "method": "unreviewed",
                "human_reviewer": None,
                "date": None,
                "notes": "",
            },
        )
    return {"schema_version": 1, "revision": 0, "dataset": dataset}


def bank(root):
    return read(root / "draft.json") if (root / "draft.json").exists() else seed()


def release(root, rid):
    if not isinstance(rid, str) or not re.fullmatch(r"[a-f0-9]{64}", rid):
        raise ValueError("无效的基准版本")
    dataset = read(root / "releases" / rid / "dataset.json")
    if digest(dataset) != rid:
        raise ValueError("封存题库指纹不匹配，不能用于评测")
    validate(dataset)
    return dataset


def ledger(root):
    return read(root / "uses.json") if (root / "uses.json").exists() else []


def sources(data):
    result = []
    for pointer_path in sorted((data / "knowledge-bases").glob("*/active.json")):
        try:
            kb = pointer_path.parent.name
            pointer = read(pointer_path)
            version = pointer["version"]
            if not re.fullmatch(r"[a-zA-Z0-9_-]+", kb) or not re.fullmatch(
                r"[a-zA-Z0-9_-]+", version
            ):
                continue
            path = pointer_path.parent / "versions" / version / "manifest.json"
            raw = path.read_bytes()
            if hashlib.sha256(raw).hexdigest() != pointer["manifest_sha256"]:
                continue
            manifest = json.loads(raw)
            result.append(
                {
                    "kb": kb,
                    "knowledge_version": version,
                    "source_sha256": manifest["source"]["sha256"],
                    "source_filename": manifest["source"].get("filename", kb + ".pdf"),
                    "source_pages": manifest["source"]["pages"],
                    "status": manifest["status"],
                }
            )
        except (OSError, ValueError, KeyError, TypeError):
            # Unpublished/broken indexes are not selectable; the PDF library exposes import errors.
            continue
    return result


def bind_source(root, data, request):
    current = bank(root)
    if request.get("revision") != current["revision"]:
        raise ValueError("题库已被修改，请重新载入后切换资料")
    source = next((s for s in sources(data) if s["kb"] == request.get("kb")), None)
    if source is None:
        raise ValueError("请先在 PDF 知识库完成资料导入，再选择可用的已发布版本")

    def key(dataset):
        return digest([dataset["kb"], dataset["knowledge_version"]])

    target = root / "drafts" / (key(source) + ".json")
    if key(source) == key(current["dataset"]):
        return document(root)
    if target.exists():
        candidate = read(target)
        load_source(candidate["dataset"], data)
    else:
        candidate = seed()
        dataset = candidate["dataset"]
        same_pdf = dataset["source_sha256"] == source["source_sha256"]
        dataset.update({k: v for k, v in source.items() if k != "status"})
        _, _, blocks = load_source(dataset, data)
        if not same_pdf:
            dataset["cases"] = []
        else:
            try:
                validate(dataset, blocks)
            except ValueError:
                dataset["cases"] = []
    write(root / "drafts" / (key(current["dataset"]) + ".json"), current)
    candidate["revision"] = current["revision"] + 1
    write(root / "draft.json", candidate)
    return document(root)


def document(root):
    result = bank(root)
    result["releases"] = []
    for path in sorted((root / "releases").glob("*/dataset.json"), reverse=True):
        dataset = release(root, path.parent.name)
        result["releases"].append(
            {
                "id": path.parent.name,
                "version": dataset["dataset_version"],
                "source_filename": dataset["source_filename"],
                "created_at": dataset["frozen_at"],
                "cases": [
                    {"id": c["id"], "split": c["split"], "question": c["question"]}
                    for c in dataset["cases"]
                ],
            }
        )
    result["releases"].sort(key=lambda r: r["created_at"], reverse=True)
    result["uses"] = ledger(root)
    return result


def save_case(root, request):
    current = bank(root)
    if request.get("revision") != current["revision"]:
        raise ValueError("题库已被修改，请重新载入后保存")
    case = copy.deepcopy(request["case"])
    if not re.fullmatch(r"[a-zA-Z0-9_-]{1,64}", case.get("id", "")):
        raise ValueError("题号仅支持 1–64 个字母、数字、下划线或连字符")
    cases = current["dataset"]["cases"]
    existing = next((c for c in cases if c["id"] == case["id"]), None)
    if not existing and len(cases) >= 500:
        raise ValueError("当前题库最多 500 题")
    case["origin"] = existing["origin"] if existing else "manual"
    exposed = {normalized(c["question"]) for c in seed()["dataset"]["cases"]}
    if case["split"] == "holdout" and (
        case["origin"] == "legacy_exposed" or normalized(case["question"]) in exposed
    ):
        raise ValueError("历史题目已经用于评测，不能改名后作为独立留出题")
    review = case.get("review", {})
    reviewer = str(review.get("human_reviewer") or "").strip()
    notes = str(review.get("notes") or "").strip()
    if len(reviewer) > 100 or len(notes) > 5000:
        raise ValueError("复核人最多 100 字，核对依据最多 5000 字")
    confirmed = request.get("confirm_human") is True
    if confirmed:
        if not reviewer or not notes:
            raise ValueError("确认标注需要填写复核人和核对依据")
        case["review"] = {
            "method": "human",
            "human_reviewer": reviewer,
            "notes": notes,
            "date": now(),
        }
    else:
        case["review"] = {
            "method": "unreviewed",
            "human_reviewer": reviewer or None,
            "notes": notes,
            "date": None,
        }
    for field in ("question", "reference_answer", "acceptance_criteria"):
        if (
            not isinstance(case.get(field), str)
            or len(case[field].strip()) > 10000
            or ((confirmed or field == "question") and not case[field].strip())
        ):
            raise ValueError("问题、参考答案和验收标准必须填写，且各不超过 10000 字")
        case[field] = case[field].strip()
    if not isinstance(case.get("evidence"), list) or len(case["evidence"]) > 30:
        raise ValueError("证据最多 30 条")
    if existing:
        cases[cases.index(existing)] = case
    else:
        cases.append(case)
    validate(current["dataset"], allow_drafts=True)
    current["revision"] += 1
    write(root / "draft.json", current)
    return document(root)


def freeze(root, data, request):
    current = bank(root)
    if request.get("revision") != current["revision"]:
        raise ValueError("题库已被修改，请重新载入后封存")
    dataset = copy.deepcopy(current["dataset"])
    dataset["cases"] = [c for c in dataset["cases"] if c["review"]["method"] == "human"]
    if not dataset["cases"]:
        raise ValueError("请先人工核对并确认至少一道题；未确认的题目不会封存")
    _, _, blocks = load_source(dataset, data)
    validate(dataset, blocks)
    dataset["frozen_at"] = now()
    dataset["dataset_version"] = (
        "human-"
        + datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
        + "-"
        + uuid.uuid4().hex[:6]
    )
    rid = digest(dataset)
    folder = root / "releases" / rid
    folder.mkdir(parents=True, exist_ok=False)
    write(folder / "dataset.json", dataset)
    return document(root)


def source_page(root, data, page):
    dataset = bank(root)["dataset"]
    if type(page) is not int or not 1 <= page <= dataset["source_pages"]:
        raise ValueError("无效的 PDF 物理页码")
    _, _, blocks = load_source(dataset, data)
    return {
        "page": page,
        "blocks": [
            {
                "block_id": bid,
                "page": page,
                "quote": b["text"],
                "text_sha256": hashlib.sha256(b["text"].encode()).hexdigest(),
                "eligible_at_annotation": bool(b["eligible"]),
            }
            for bid, b in blocks.items()
            if b["page"] == page and b["text"].strip()
        ],
    }


def prepare(root, data, output, request):
    """Bind one job to immutable labels; reserve holdout exposure before any execution."""
    dataset = release(root, request.get("release"))
    split = request.get("split")
    if split not in {"development", "regression", "holdout"}:
        raise ValueError("请选择开发、回归或留出分组")
    cases = [c for c in dataset["cases"] if c["split"] == split]
    ids = request.get("case_ids", [])
    if (
        not isinstance(ids, list)
        or len(ids) != len(set(ids))
        or any(not isinstance(i, str) for i in ids)
    ):
        raise ValueError("题号格式错误或重复")
    if ids:
        if not set(ids) <= {c["id"] for c in cases}:
            raise ValueError("题号不属于所选封存版本和分组")
        cases = [c for c in cases if c["id"] in ids]
    if not cases:
        raise ValueError("所选分组没有题目")
    if request.get("kind") == "model" and len(cases) > 8:
        raise ValueError("真实模型每轮最多 8 题，请填写本轮题号；每轮仍最多 40 次请求")
    if split == "holdout" and request.get("acknowledge_holdout") is not True:
        raise ValueError("运行留出题前，请确认方案已固定，本轮结果不用于调参")
    _, _, blocks = load_source(dataset, data)
    validate(dataset, blocks)
    uses = ledger(root)
    fingerprints = [digest(normalized(c["question"])) for c in cases]
    exposed = {q for use in uses for q in use["question_hashes"]}
    repeated = sum(q in exposed for q in fingerprints)
    selection = {
        "release": request["release"],
        "dataset_version": dataset["dataset_version"],
        "source_filename": dataset["source_filename"],
        "split": split,
        "case_ids": [c["id"] for c in cases],
        "kind": request["kind"],
        "used_before": repeated,
        "holdout_status": "reused"
        if repeated
        else "first_use_declared"
        if split == "holdout"
        else "not_holdout",
        "note": "首次使用仅按本机记录判断；题目独立性由作者声明，不证明未在其他地方见过。重复运行不能称为新的盲测。",
    }
    # Snapshot all labels to retain the frozen hash; runners select only these IDs.
    write(output / "dataset.json", dataset)
    write(output / "selection.json", selection)
    uses.append(
        {
            "job_id": output.name,
            "created_at": now(),
            **selection,
            "question_hashes": fingerprints,
        }
    )
    write(root / "uses.json", uses)
    return selection


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    try:
        request = json.loads(sys.stdin.read())
        action = request.get("action")
        if action == "read":
            result = document(args.root)
        elif action == "save":
            result = save_case(args.root, request)
        elif action == "freeze":
            result = freeze(args.root, args.data, request)
        elif action == "bind-source":
            result = bind_source(args.root, args.data, request)
        elif action == "source-page":
            result = source_page(args.root, args.data, request.get("page"))
        elif action == "export":
            result = release(args.root, request.get("release"))
        elif action == "prepare" and args.out:
            result = prepare(args.root, args.data, args.out, request)
        else:
            raise ValueError("不支持的题库操作")
        if action in {"read", "save", "freeze", "bind-source"}:
            result["sources"] = sources(args.data)
        print(json.dumps({"ok": True, "result": result}, ensure_ascii=False))
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
