//! Deterministic completion checks. These do not establish factual correctness.
use crate::{db::Database, review_agent::ReviewRun};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaskContract {
    pub min_questions: usize,
    pub max_questions: usize,
    pub require_sources: bool,
    pub max_repairs: usize,
}
impl Default for TaskContract {
    fn default() -> Self {
        Self {
            min_questions: 1,
            max_questions: 3,
            require_sources: false,
            max_repairs: 2,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CompletionState {
    pub contract: TaskContract,
    pub repair_attempts: usize,
    pub last_report: Option<CompletionReport>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    AwaitingAnswer,
    Completed,
    NeedsRepair,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionCheck {
    pub code: String,
    pub label: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionReport {
    pub version: String,
    pub outcome: Outcome,
    pub checks: Vec<CompletionCheck>,
    pub verified_submissions: usize,
    pub content_quality: String,
}

pub struct Validation {
    pub report: CompletionReport,
    pub guidance: Option<String>,
}

pub fn validate(db: &Database, run: &ReviewRun) -> Result<Validation, String> {
    let mut conn = db.connect().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let record: String = tx
        .query_row(
            "SELECT record FROM review_runs WHERE id=?",
            [&run.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let saved: ReviewRun = serde_json::from_str(&record).map_err(|e| e.to_string())?;
    let events = tx
        .prepare("SELECT question_id,correct,memory_id FROM review_memory_events WHERE run_id=?")
        .map_err(|e| e.to_string())?
        .query_map([&run.id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let contract = &run.completion.contract;
    let mut checks = vec![];
    let mut check = |code: &str, label: &str, passed: bool, pending: bool| {
        checks.push(CompletionCheck {
            code: code.into(),
            label: label.into(),
            status: if passed {
                "passed"
            } else if pending {
                "pending"
            } else {
                "failed"
            }
            .into(),
        });
    };
    check(
        "contract_valid",
        "任务完成条件有效",
        (1..=3).contains(&contract.min_questions)
            && (contract.min_questions..=3).contains(&contract.max_questions)
            && contract.max_repairs <= 3,
        false,
    );
    let mut ids = HashSet::new();
    check("question_structure", "题目与选项结构有效", run.questions.iter().all(|q| {
        !q.id.is_empty() && ids.insert(&q.id) && !q.question.trim().is_empty()
            && (2..=4).contains(&q.options.len()) && q.correct_index < q.options.len()
            && q.options.iter().all(|s| !s.trim().is_empty())
            && q.options.iter().map(|s| s.trim()).collect::<HashSet<_>>().len() == q.options.len()
            && !matches!(q.selected_index, Some(i) if i >= q.options.len())
            && !matches!(q.correct, Some(correct) if q.selected_index.map(|i| i == q.correct_index) != Some(correct))
    }), false);
    check(
        "question_limit",
        "题目数量未超出约定",
        run.questions.len() <= contract.max_questions,
        false,
    );
    let pending_answers = run
        .questions
        .iter()
        .filter(|q| q.selected_index.is_none())
        .count();
    check(
        "one_pending_question",
        "一次只等待一道题的回答",
        pending_answers <= 1,
        false,
    );
    check(
        "source_scope",
        "引用编号、页码和章节有效",
        run.questions.iter().all(|q| {
            q.source_ids.iter().all(|id| {
                run.sources.iter().any(|s| {
                    s["id"] == *id
                        && s["page"].as_u64().is_some_and(|p| p > 0)
                        && s["block_id"].as_str().is_some_and(|id| !id.is_empty())
                        && serde_json::from_value::<Vec<String>>(s["chapter_path"].clone())
                            .is_ok_and(|p| p.starts_with(&run.scope.chapter_path))
                })
            })
        }),
        false,
    );
    check(
        "required_sources",
        "符合教材依据模式",
        !contract.require_sources || run.questions.iter().all(|q| !q.source_ids.is_empty()),
        false,
    );
    check(
        "durable_submission",
        "答题状态与保存的用户提交一致",
        run.questions.iter().all(|q| {
            saved.questions.iter().any(|s| {
                s.id == q.id
                    && s.selected_index == q.selected_index
                    && s.correct == q.correct
                    && s.correct_index == q.correct_index
                    && s.options == q.options
            })
        }),
        false,
    );
    let verified_submissions = run
        .questions
        .iter()
        .filter(|q| {
            q.correct.is_some()
                && events.iter().any(|(id, correct, memory)| {
                    id == &q.id
                        && Some(*correct) == q.correct
                        && !matches!(&q.memory_id, Some(expected) if expected != memory)
                })
        })
        .count();
    check(
        "grade_transaction",
        "已判分题目均有唯一入库事件",
        verified_submissions == run.questions.iter().filter(|q| q.correct.is_some()).count()
            && events.len() == verified_submissions,
        false,
    );
    check(
        "tools_settled",
        "没有未完成的工具调用",
        run.pending.is_empty(),
        false,
    );
    let hard_failure = checks.iter().any(|c| c.status == "failed");
    let unrecorded = run
        .questions
        .iter()
        .any(|q| q.selected_index.is_some() && q.correct.is_none());
    let enough_questions = run.questions.len() >= contract.min_questions;
    let targets_covered = run.due_memory_ids.iter().all(|id| {
        run.questions
            .iter()
            .any(|q| q.memory_id.as_ref() == Some(id))
    });
    for (code, label, passed) in [
        (
            "required_questions",
            "已覆盖约定的题目数量",
            enough_questions,
        ),
        ("due_targets", "已覆盖指定到期知识点", targets_covered),
        (
            "answers_recorded",
            "所有题目均已提交并记录",
            !run.questions.is_empty() && run.questions.iter().all(|q| q.correct.is_some()),
        ),
    ] {
        checks.push(CompletionCheck {
            code: code.into(),
            label: label.into(),
            status: if passed { "passed" } else { "pending" }.into(),
        });
    }
    let (outcome, guidance) = if hard_failure {
        (Outcome::Rejected, None)
    } else if unrecorded {
        (
            Outcome::NeedsRepair,
            Some(
                "用户已提交的答案尚未记录，请先调用 record_quiz_result，不要重复出题或代答。"
                    .into(),
            ),
        )
    } else if pending_answers == 1 {
        (Outcome::AwaitingAnswer, None)
    } else if !enough_questions || !targets_covered {
        (Outcome::NeedsRepair, Some(format!("任务尚未完成：需要 {}–{} 道题并覆盖指定到期知识点。请查阅资料后调用 save_review_question，已有知识点带回 memory_id，然后等待用户实际作答。", contract.min_questions, contract.max_questions)))
    } else {
        (Outcome::Completed, None)
    };
    Ok(Validation {
        report: CompletionReport {
            version: "review-completion-v1".into(),
            outcome,
            checks,
            verified_submissions,
            content_quality: "not_assessed".into(),
        },
        guidance,
    })
}
