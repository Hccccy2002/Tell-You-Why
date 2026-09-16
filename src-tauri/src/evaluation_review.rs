//! Human judgments are stored separately from the immutable model report.
use crate::evaluation::{read_json, write_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReviewInput {
    pub reviewer: String,
    pub correct: Option<bool>,
    pub complete: Option<bool>,
    pub grounded: Option<bool>,
    #[serde(default)]
    pub behavior_appropriate: Option<bool>,
    pub notes: String,
}
impl ReviewInput {
    fn finished(&self) -> bool {
        self.correct.is_some()
            && self.complete.is_some()
            && self.grounded.is_some()
            && !self.notes.trim().is_empty()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveReview {
    pub id: String,
    pub case_id: String,
    pub report_sha256: String,
    pub expected_revision: u64,
    pub review: ReviewInput,
}

#[derive(Clone, Deserialize, Serialize)]
struct Annotation {
    id: String,
    result_sha256: String,
    method: Option<String>,
    #[serde(flatten)]
    review: ReviewInput,
    revision: u64,
    updated_at: String,
}

// `dataset_sha256` and `rows` also match the CLI textbook scorer's annotation format.
#[derive(Deserialize, Serialize)]
struct Annotations {
    schema_version: u32,
    report_sha256: String,
    dataset_sha256: String,
    rows: Vec<Annotation>,
}

fn digest(report: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(report).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn targets(report: &Value) -> Vec<(&str, &str)> {
    report["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            row["details"]["result"]["state"] == "completed"
                && row["details"]["result"]["draft"].is_object()
        })
        .filter_map(|row| {
            let id = row["id"].as_str()?;
            let template = report["raw"]["review_template"]["rows"]
                .as_array()?
                .iter()
                .find(|r| r["id"] == id)?;
            Some((id, template["result_sha256"].as_str()?))
        })
        .collect()
}

fn metadata<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == id))
        .map(|row| &row["details"]["evaluation"])
        .unwrap_or(&Value::Null)
}

fn finished(report: &Value, id: &str, input: &ReviewInput) -> bool {
    input.finished()
        && (metadata(report, id)["expected_behavior"].as_str().is_none()
            || input.behavior_appropriate.is_some())
}

fn load(dir: &Path, report: &Value) -> Result<Annotations, String> {
    let report_sha256 = digest(report)?;
    let dataset_sha256 = report["raw"]["review_template"]["dataset_sha256"]
        .as_str()
        .ok_or("报告缺少复核模板，请重新运行评测。")?
        .to_owned();
    let path = dir.join("human-review.json");
    if !path.exists() {
        return Ok(Annotations {
            schema_version: 1,
            report_sha256,
            dataset_sha256,
            rows: vec![],
        });
    }
    let annotations: Annotations = serde_json::from_value(read_json(&path)?)
        .map_err(|_| "人工复核文件格式错误；原评测报告仍可查看。")?;
    if annotations.schema_version != 1
        || annotations.report_sha256 != report_sha256
        || annotations.dataset_sha256 != dataset_sha256
    {
        return Err("报告内容已变化，已有复核不再适用；请重新运行评测后复核。".into());
    }
    let eligible = targets(report);
    let mut seen = HashSet::new();
    for row in &annotations.rows {
        if !seen.insert(&row.id)
            || !eligible.contains(&(row.id.as_str(), row.result_sha256.as_str()))
            || row.revision == 0
            || row.review.reviewer.trim().is_empty()
            || row.method.as_deref() != finished(report, &row.id, &row.review).then_some("human")
        {
            return Err("人工复核记录与当前题目不匹配；原评测报告仍可查看。".into());
        }
    }
    Ok(annotations)
}

pub fn save(dir: &Path, report: &Value, request: SaveReview) -> Result<(), String> {
    let mut annotations = load(dir, report)?;
    if request.report_sha256 != annotations.report_sha256 {
        return Err("报告已更新，请刷新后再保存复核。".into());
    }
    let eligible = targets(report);
    let (_, result_sha256) = eligible
        .iter()
        .find(|(id, _)| *id == request.case_id)
        .ok_or("只能复核已完成的模型问答，不能复核未执行的题目或流程记录。")?;
    let existing = annotations
        .rows
        .iter()
        .position(|r| r.id == request.case_id);
    let revision = existing.map(|i| annotations.rows[i].revision).unwrap_or(0);
    if request.expected_revision != revision {
        return Err("这条复核已被修改，请刷新报告后再保存，避免覆盖其他修改。".into());
    }
    let mut review = request.review;
    review.reviewer = review.reviewer.trim().to_owned();
    review.notes = review.notes.trim().to_owned();
    if review.reviewer.is_empty() || review.reviewer.chars().count() > 100 {
        return Err("请填写复核人（最多 100 字）。".into());
    }
    if review.notes.chars().count() > 5000 {
        return Err("复核备注最多 5000 字。".into());
    }
    let method = finished(report, &request.case_id, &review).then(|| "human".to_owned());
    let annotation = Annotation {
        id: request.case_id,
        result_sha256: (*result_sha256).to_owned(),
        method,
        review,
        revision: revision.checked_add(1).ok_or("复核版本号超出范围")?,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Some(index) = existing {
        annotations.rows[index] = annotation;
    } else {
        annotations.rows.push(annotation);
    }
    write_json(&dir.join("human-review.json"), &annotations)
}

/// Derive display/export values without changing report.json or model execution status.
pub fn decorate(dir: &Path, report: &mut Value) -> Result<(), String> {
    let annotations = load(dir, report)?;
    let eligible: Vec<(String, String)> = targets(report)
        .into_iter()
        .map(|(id, hash)| (id.to_owned(), hash.to_owned()))
        .collect();
    let finished: Vec<_> = annotations
        .rows
        .iter()
        .filter(|r| r.method.as_deref() == Some("human"))
        .collect();
    let reviewed = finished.len();
    let total = eligible.len();
    let planned = report["raw"]["report"]["generation"]["planned"].as_u64();
    let count = |f: fn(&ReviewInput) -> Option<bool>| {
        finished
            .iter()
            .filter(|r| f(&r.review) == Some(true))
            .count()
    };
    let correct = count(|r| r.correct);
    let complete = count(|r| r.complete);
    let grounded = count(|r| r.grounded);
    let rate = |n: usize| (reviewed > 0).then(|| n as f64 / reviewed as f64);
    let behavior: Vec<_> = finished
        .iter()
        .filter(|r| r.review.behavior_appropriate.is_some())
        .collect();
    let refusals: Vec<_> = behavior
        .iter()
        .filter(|r| metadata(report, &r.id)["expected_behavior"] == "refuse")
        .collect();
    let refusal_passed = refusals
        .iter()
        .filter(|r| r.review.behavior_appropriate == Some(true))
        .count();
    let refusal_rate =
        (!refusals.is_empty()).then(|| refusal_passed as f64 / refusals.len() as f64);
    let mut groups = vec![];
    for field in ["kind", "split"] {
        let names: std::collections::BTreeSet<_> = eligible
            .iter()
            .filter_map(|(id, _)| metadata(report, id)[field].as_str())
            .collect();
        for name in names {
            let members: Vec<_> = finished
                .iter()
                .filter(|r| metadata(report, &r.id)[field] == name)
                .collect();
            let passed = members
                .iter()
                .filter(|r| {
                    r.review.correct == Some(true)
                        && r.review.complete == Some(true)
                        && r.review.grounded == Some(true)
                        && r.review.behavior_appropriate != Some(false)
                })
                .count();
            groups.push(json!({"dimension":field,"name":name,"reviewed":members.len(),"passed":passed,
                "total":eligible.iter().filter(|(id, _)| metadata(report, id)[field] == name).count(),
                "pass_rate":(!members.is_empty()).then(|| passed as f64 / members.len() as f64)}));
        }
    }
    let display = |n: usize| match rate(n) {
        Some(value) => format!("{:.1}%", value * 100.0),
        None => "待复核".to_owned(),
    };
    let is_benchmark = report["benchmark"].is_object();
    if let Some(metrics) = report["metrics"].as_array_mut() {
        metrics.retain(|m| m["label"] != "内容正确率");
        metrics.push(
            json!({"label":"人工复核进度","value":format!("{reviewed} / {total}"),"baseline":null}),
        );
        for (label, count) in [
            ("事实正确率（人工）", correct),
            ("回答完整率（人工）", complete),
            ("证据支持率（人工）", grounded),
        ] {
            metrics.push(json!({"label":label,"value":display(count),"baseline":null,
                "detail":(reviewed > 0).then(|| format!("符合 {count} / {reviewed} 道"))}));
        }
        if is_benchmark {
            metrics.push(json!({"label":"应拒答题通过率（人工）","value":refusal_rate.map(|r| format!("{:.1}%",r*100.0)).unwrap_or("待复核".into()),"baseline":null,
                "detail":format!("行为符合 {refusal_passed} / {} 道已复核应拒答题",refusals.len())}));
        }
    }
    report["human_review"] = json!({
        "report_sha256":annotations.report_sha256,
        "reviewed":reviewed,"total":total,"drafts":annotations.rows.len()-reviewed,
        "error":null
    });
    report["human_review"]["groups"] = json!(groups);
    if let Some(rows) = report["rows"].as_array_mut() {
        for row in rows {
            let Some((id, hash)) = eligible.iter().find(|(id, _)| row["id"] == *id) else {
                continue;
            };
            let annotation = annotations.rows.iter().find(|r| &r.id == id);
            row["human_review"] = json!({"result_sha256":hash,"annotation":annotation});
            if let Some(annotation) = annotation {
                let completed = annotation.method.as_deref() == Some("human");
                let passed = completed
                    && annotation.review.correct == Some(true)
                    && annotation.review.complete == Some(true)
                    && annotation.review.grounded == Some(true)
                    && annotation.review.behavior_appropriate != Some(false);
                row["status"] = json!(if passed {
                    "passed"
                } else if completed {
                    "failed"
                } else {
                    "unreviewed"
                });
                row["note"] = json!(if completed {
                    "已人工复核 · 所有必填判断符合才标记通过"
                } else {
                    "复核草稿 · 尚未计入质量分数"
                });
            }
        }
    }
    if let Some(notes) = report["notes"].as_array_mut() {
        notes.retain(|n| {
            !n.as_str()
                .is_some_and(|s| s.starts_with("回答正确性和证据支持度尚未复核"))
        });
        notes.push(json!("人工质量分数仅统计完成所有必填判断并填写备注的问答；草稿和未复核题目不计分。Agent 条目仍表示执行流程状态。"));
    }
    report["raw"]["report"]["quality_reviews"]["human"] = if reviewed == 0 {
        Value::Null
    } else {
        json!({"reviewed":reviewed,"planned":planned,"correct":rate(correct),
            "complete":rate(complete),"complete_reviewed":reviewed,"grounded":rate(grounded)})
    };
    if reviewed > 0 && report["benchmark"].is_object() {
        let quality = &mut report["raw"]["report"]["quality_reviews"]["human"];
        quality["behavior_reviewed"] = json!(behavior.len());
        quality["behavior_appropriate"] = json!((!behavior.is_empty()).then(|| behavior
            .iter()
            .filter(|r| r.review.behavior_appropriate == Some(true))
            .count()
            as f64
            / behavior.len() as f64));
        quality["refusal_reviewed"] = json!(refusals.len());
        quality["appropriate_refusal"] = json!(refusal_rate);
        quality["groups"] = json!(groups);
    }
    report["raw"]["human_review"] = serde_json::to_value(annotations).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn report() -> Value {
        json!({
            "dataset":"test-v1", "notes":[],
            "metrics":[{"label":"生成完成","value":"3 / 4"},{"label":"内容正确率","value":"待复核"}],
            "rows":[
                {"id":"q1","title":"问题一","status":"unreviewed","details":{"result":{"state":"completed","draft":{"status":"answered"}},"evidence":[{"text":"原文"}]}},
                {"id":"q2","status":"unreviewed","details":{"result":{"state":"completed","draft":{"status":"insufficient","reason":"缺少证据"}}}},
                {"id":"q3","status":"failed","details":{"result":{"state":"failed"}}},
                {"id":"agent","status":"passed","details":{"state":"completed"}}
            ],
            "raw":{"report":{"generation":{"planned":4},"quality_reviews":{"human":null,"ai":{"correct":0.5}}},
                "review_template":{"dataset_sha256":"dataset-hash","rows":[
                    {"id":"q1","result_sha256":"result-1"},
                    {"id":"q2","result_sha256":"result-2"},
                    {"id":"q3","result_sha256":"result-3"}
                ]}}
        })
    }

    pub fn request(report: &Value) -> SaveReview {
        SaveReview {
            id: String::new(),
            case_id: "q1".into(),
            report_sha256: digest(report).unwrap(),
            expected_revision: 0,
            review: ReviewInput {
                reviewer: "测试复核人".into(),
                correct: Some(false),
                complete: Some(true),
                grounded: Some(true),
                behavior_appropriate: None,
                notes: "回答与原文矛盾".into(),
            },
        }
    }

    #[test]
    fn partial_judgments_are_persistent_drafts_not_quality_scores() {
        let dir = tempfile::tempdir().unwrap();
        let original = report();
        let mut draft = request(&original);
        draft.review.complete = None;
        save(dir.path(), &original, draft).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["human_review"]["total"], 2);
        assert_eq!(view["human_review"]["reviewed"], 0);
        assert_eq!(view["human_review"]["drafts"], 1);
        assert_eq!(view["rows"][0]["status"], "unreviewed");
        assert!(view["raw"]["report"]["quality_reviews"]["human"].is_null());
        assert_eq!(
            view["rows"][0]["human_review"]["annotation"]["correct"],
            false
        );
        let mut full = request(&original);
        full.expected_revision = 1;
        save(dir.path(), &original, full).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["human_review"]["reviewed"], 1);
        assert_eq!(view["rows"][0]["status"], "failed");
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["human"]["correct"],
            0.0
        );
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["human"]["complete"],
            1.0
        );
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["human"]["reviewed"],
            1
        );
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["human"]["planned"],
            4
        );
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["ai"]["correct"],
            0.5
        );
        assert!(view["rows"][2].get("human_review").is_none());
        assert!(view["rows"][3].get("human_review").is_none());
        let mut other = request(&original);
        other.case_id = "q2".into();
        other.review.correct = Some(true);
        save(dir.path(), &original, other).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["rows"][1]["status"], "passed");
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["human"]["correct"],
            0.5
        );
    }

    #[test]
    fn changed_answers_sources_and_dataset_cannot_inherit_reviews() {
        let dir = tempfile::tempdir().unwrap();
        let original = report();
        save(dir.path(), &original, request(&original)).unwrap();
        for pointer in [
            "/rows/0/title",
            "/rows/0/details/evidence/0/text",
            "/dataset",
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("已变化");
            assert!(decorate(dir.path(), &mut changed)
                .unwrap_err()
                .contains("已变化"));
            assert!(save(dir.path(), &changed, request(&changed)).is_err());
        }
    }

    #[test]
    fn benchmark_requires_behavior_judgment_and_separates_refusal_denominator() {
        let dir = tempfile::tempdir().unwrap();
        let mut original = report();
        original["benchmark"] = json!({"split":"holdout"});
        original["rows"][0]["details"]["evaluation"] =
            json!({"kind":"absent","split":"holdout","expected_behavior":"refuse"});
        original["rows"][1]["details"]["evaluation"] =
            json!({"kind":"single","split":"holdout","expected_behavior":"answer"});
        save(dir.path(), &original, request(&original)).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["human_review"]["reviewed"], 0);
        assert_eq!(view["human_review"]["drafts"], 1);
        let mut completed = request(&original);
        completed.expected_revision = 1;
        completed.review.behavior_appropriate = Some(false);
        save(dir.path(), &original, completed).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["human_review"]["reviewed"], 1);
        assert_eq!(view["human_review"]["total"], 2);
        assert_eq!(view["rows"][0]["status"], "failed");
        let quality = &view["raw"]["report"]["quality_reviews"]["human"];
        assert_eq!(quality["refusal_reviewed"], 1);
        assert_eq!(quality["appropriate_refusal"], 0.0);
        assert_eq!(quality["groups"][0]["reviewed"], 1);
        assert!(quality["groups"][1]["pass_rate"].is_null());
        assert_eq!(quality["groups"][2]["total"], 2);
        assert_eq!(
            view["raw"]["report"]["quality_reviews"]["ai"]["correct"],
            0.5
        );
    }

    #[test]
    fn rejects_stale_writes_invalid_targets_and_unattributed_reviews() {
        let dir = tempfile::tempdir().unwrap();
        let original = report();
        let mut stale = request(&original);
        stale.report_sha256 = "old".into();
        assert!(save(dir.path(), &original, stale).is_err());
        for id in ["q3", "agent", "unknown", "../job"] {
            let mut invalid = request(&original);
            invalid.case_id = id.into();
            assert!(save(dir.path(), &original, invalid).is_err());
        }
        let mut invalid = request(&original);
        invalid.review.reviewer = "  ".into();
        assert!(save(dir.path(), &original, invalid).is_err());
        save(dir.path(), &original, request(&original)).unwrap();
        assert!(save(dir.path(), &original, request(&original))
            .unwrap_err()
            .contains("已被修改"));
    }

    #[test]
    fn clearing_a_judgment_returns_the_case_to_pending() {
        let dir = tempfile::tempdir().unwrap();
        let original = report();
        save(dir.path(), &original, request(&original)).unwrap();
        let mut draft = request(&original);
        draft.expected_revision = 1;
        draft.review.notes = " ".into();
        save(dir.path(), &original, draft).unwrap();
        let mut view = original.clone();
        decorate(dir.path(), &mut view).unwrap();
        assert_eq!(view["human_review"]["reviewed"], 0);
        assert!(view["raw"]["report"]["quality_reviews"]["human"].is_null());
    }

    #[test]
    fn corrupt_or_misattributed_annotations_do_not_produce_scores() {
        let dir = tempfile::tempdir().unwrap();
        let original = report();
        save(dir.path(), &original, request(&original)).unwrap();
        let path = dir.path().join("human-review.json");
        let stored = read_json(&path).unwrap();
        let mut changed = stored.clone();
        changed["rows"][0]["method"] = json!("ai");
        write_json(&path, &changed).unwrap();
        assert!(decorate(dir.path(), &mut report()).is_err());
        changed = stored.clone();
        changed["rows"]
            .as_array_mut()
            .unwrap()
            .push(stored["rows"][0].clone());
        write_json(&path, &changed).unwrap();
        assert!(decorate(dir.path(), &mut report()).is_err());
        std::fs::write(&path, "broken").unwrap();
        assert!(decorate(dir.path(), &mut report()).is_err());
    }
}
