//! A bounded learning plan. Progress is derived from saved lessons and real feedback.
use crate::{
    db::{Database, DbError},
    study::{StudySession, StudyStep},
};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyObjective {
    pub id: String,
    pub title: String,
    pub criterion: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyPlan {
    pub objectives: Vec<StudyObjective>,
    pub checkin_prompt: String,
    // Some("") is an explicit skip, not an inferred answer.
    pub checkin_reply: Option<String>,
    pub lesson_targets: BTreeMap<String, String>,
    pub remediation_step_id: Option<String>,
    pub verification_step_id: Option<String>,
    pub verification_target: Option<String>,
}

impl StudySession {
    pub(crate) fn goal_taught(&self, objective: &str) -> bool {
        self.goal_plan.as_ref().is_some_and(|plan| {
            self.steps.iter().any(|step| {
                step.quiz.is_none()
                    && plan.lesson_targets.get(&step.id).map(String::as_str) == Some(objective)
            })
        })
    }

    pub(crate) fn goal_remediation_target(&self) -> Option<&str> {
        let plan = self.goal_plan.as_ref()?;
        if plan.remediation_step_id.is_some() {
            return None;
        }
        let last = self.steps.last()?;
        if [Some("confused"), Some("example")].contains(&last.feedback.as_deref()) {
            return plan.lesson_targets.get(&last.id).map(String::as_str);
        }
        if last
            .quiz
            .as_ref()
            .is_some_and(|q| q.selected.is_some_and(|s| s != q.correct_index))
        {
            return plan.verification_target.as_deref();
        }
        None
    }

    pub(crate) fn goal_finished(&self) -> bool {
        let Some(plan) = &self.goal_plan else {
            return false;
        };
        plan.checkin_reply.is_some()
            && plan.objectives.iter().all(|o| self.goal_taught(&o.id))
            && plan.verification_step_id.as_ref().is_some_and(|id| {
                self.steps
                    .iter()
                    .any(|s| &s.id == id && s.quiz.is_some() && s.feedback.is_some())
            })
            && self.goal_remediation_target().is_none()
            && self.steps.last().is_some_and(|s| s.feedback.is_some())
            && self.pending_question().is_none()
    }

    pub(crate) fn goal_has_budget(&self) -> bool {
        self.control.stop.as_ref().map_or(true, |s| s.resumable)
            && (self.model_calls < self.control.policy.max_model_calls || self.pending.is_some())
            && self.tool_calls < self.control.policy.max_tool_calls
            && self.control.charged_tokens < self.control.policy.max_token_charge
            && self.control.charged_active_ms < self.control.policy.max_active_ms
    }

    pub(crate) fn validate_goal_lesson(
        &self,
        objective: Option<&str>,
        kind: &str,
    ) -> Result<bool, String> {
        let plan = self.goal_ready()?;
        let objective = objective
            .filter(|id| plan.objectives.iter().any(|o| o.id == *id))
            .ok_or("请选择计划中的有效 objective_id")?;
        if let Some(target) = self.goal_remediation_target() {
            if objective != target || !["example", "prerequisite"].contains(&kind) {
                return Err("先针对刚才的子目标补基础或换例子，不要推进其它目标".into());
            }
            return Ok(true);
        }
        if self.goal_taught(objective) {
            return Err("该子目标已讲解；本轮最多一次补讲，请继续未讲解目标或提供验证".into());
        }
        Ok(false)
    }

    pub(crate) fn validate_goal_quiz(&self, objective: Option<&str>) -> Result<(), String> {
        let plan = self.goal_ready()?;
        if plan.verification_step_id.is_some() {
            return Err("本轮只提供一道可跳过的验证题，不重复出题".into());
        }
        if self.goal_remediation_target().is_some()
            || !plan.objectives.iter().all(|o| self.goal_taught(&o.id))
        {
            return Err("先讲解全部子目标，并完成用户要求的补讲，再提供验证".into());
        }
        if !objective.is_some_and(|id| plan.objectives.iter().any(|o| o.id == id)) {
            return Err("验证题必须指定计划中的 objective_id，只记录该子目标的本次结果".into());
        }
        Ok(())
    }

    fn goal_ready(&self) -> Result<&StudyPlan, String> {
        let plan = self
            .goal_plan
            .as_ref()
            .ok_or("先用 plan_learning 拆解学习目标")?;
        if plan.checkin_reply.is_none() {
            return Err("先等待用户补充卡点或明确跳过，不得代填".into());
        }
        if self.steps.last().is_some_and(|s| s.feedback.is_none()) {
            return Err("请等待用户对当前讲解或验证作出反馈".into());
        }
        Ok(plan)
    }

    pub(crate) fn goal_step_reason(&self, step: &StudyStep) -> Option<String> {
        let plan = self.goal_plan.as_ref()?;
        if plan.verification_step_id.as_deref() == Some(&step.id) {
            let objective = plan
                .objectives
                .iter()
                .find(|o| Some(&o.id) == plan.verification_target.as_ref())?;
            return Some(format!(
                "用一道可跳过的小题验证“{}”；结果只对应这个子目标。",
                objective.title
            ));
        }
        if plan.remediation_step_id.as_deref() == Some(&step.id) {
            let index = self.steps.iter().position(|s| s.id == step.id)?;
            let previous = index.checked_sub(1).and_then(|i| self.steps.get(i))?;
            let basis = match previous.feedback.as_deref() {
                Some("confused") => "你刚才反馈“还没懂”",
                Some("example") => "你刚才希望看一个例子",
                Some("answer") => "你刚才的答案与本题参考答案不一致",
                _ => return None,
            };
            return Some(format!("{basis}，先针对同一子目标补基础或换个例子。"));
        }
        let id = plan.lesson_targets.get(&step.id)?;
        let objective = plan.objectives.iter().find(|o| &o.id == id)?;
        Some(
            if plan.checkin_reply.as_ref().is_some_and(|s| !s.is_empty()) {
                format!("结合你补充的卡点，围绕“{}”展开。", objective.title)
            } else {
                format!("围绕本轮子目标“{}”展开。", objective.title)
            },
        )
    }

    pub(crate) fn goal_public(&self) -> Option<Value> {
        let plan = self.goal_plan.as_ref()?;
        let verification = plan
            .verification_step_id
            .as_ref()
            .and_then(|id| self.steps.iter().find(|s| &s.id == id));
        let objectives: Vec<_> = plan.objectives.iter().map(|o| {
            let lessons: Vec<_> = self.steps.iter().filter(|s| plan.lesson_targets.get(&s.id) == Some(&o.id)).collect();
            let latest = lessons.last();
            let verified = verification.filter(|_| plan.verification_target.as_ref() == Some(&o.id))
                .and_then(|s| s.quiz.as_ref()).filter(|q| q.selected.is_some());
            json!({"id":o.id,"title":o.title,"criterion":o.criterion,
                "taught":!lessons.is_empty(),
                "self_reported_understood":latest.is_some_and(|s| s.feedback.as_deref() == Some("understood")),
                "needs_help":latest.is_some_and(|s| s.feedback.as_deref() == Some("confused")),
                "verified":verified.is_some(),"correct":verified.map(|q| q.selected == Some(q.correct_index)),
                "lesson_step_ids":lessons.iter().map(|s| &s.id).collect::<Vec<_>>()})
        }).collect();
        Some(
            json!({"objectives":objectives,"checkin_prompt":plan.checkin_prompt,
            "checkin_reply":plan.checkin_reply,"remediation_used":plan.remediation_step_id.is_some(),
            "verification_status":match verification {
                Some(s) if s.feedback.as_deref() == Some("skip") => "skipped",
                Some(s) if s.quiz.as_ref().is_some_and(|q| q.selected.is_some()) => "submitted",
                Some(_) => "awaiting_answer", None => "not_offered",
            },"finished":self.goal_finished()}),
        )
    }
}

impl Database {
    pub(crate) fn study_goal_checkin(
        &self,
        id: &str,
        reply: &str,
    ) -> Result<StudySession, DbError> {
        let reply = reply.trim();
        if reply.chars().count() > 300
            || reply
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
        {
            return Err(DbError::Validation(
                "请用300字以内补充卡点，也可以跳过".into(),
            ));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut run: StudySession = serde_json::from_str(&record)?;
        let plan = run
            .goal_plan
            .as_mut()
            .ok_or_else(|| DbError::Validation("学习计划尚未生成".into()))?;
        if let Some(saved) = &plan.checkin_reply {
            if saved == reply {
                return Ok(run);
            }
            return Err(DbError::Validation(
                "卡点已提交，不能重复覆盖；可在讲解中继续提问".into(),
            ));
        }
        if run.state != "waiting" || !run.steps.is_empty() || !run.goal_mode {
            return Err(DbError::Validation("请等待学习计划生成后再补充".into()));
        }
        plan.checkin_reply = Some(reply.into());
        run.messages.push(json!({"role":"user","content":json!({"kind":"goal_checkin","reply":reply,"skipped":reply.is_empty(),"note":"仅描述当前卡点，不计为答题或掌握成绩"}).to_string()}));
        run.state = "ready".into();
        run.revision += 1;
        tx.execute(
            "UPDATE study_sessions SET state=?,revision=?,record=?,updated_at=? WHERE id=?",
            params![
                run.state,
                run.revision,
                serde_json::to_string(&run)?,
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )?;
        tx.commit()?;
        Ok(run)
    }

    pub(crate) fn study_reopen_goal(&self, id: &str) -> Result<StudySession, DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut run: StudySession = serde_json::from_str(&record)?;
        if !run.goal_mode || run.goal_finished() {
            return Err(DbError::Validation(
                "这个目标本轮已结束，请从首页开始新的学习目标".into(),
            ));
        }
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE state <> 'completed' AND id<>?",
            [id],
            |r| r.get(0),
        )?;
        if active > 0 {
            return Err(DbError::Validation("请先继续或结束上一次学习".into()));
        }
        if run.state != "completed" {
            return Ok(run);
        }
        if !run.goal_has_budget() {
            return Err(DbError::Validation(
                "这个目标的执行预算已用完，记录保留；请开始新的学习目标".into(),
            ));
        }
        run.state = if run.pending_question().is_some() || run.pending.is_some() {
            "ready"
        } else if run
            .goal_plan
            .as_ref()
            .is_some_and(|p| p.checkin_reply.is_none())
            || run.steps.last().is_some_and(|s| s.feedback.is_none())
        {
            "waiting"
        } else {
            "ready"
        }
        .into();
        run.error = None;
        run.next_topic = None;
        run.revision += 1;
        tx.execute(
            "UPDATE study_sessions SET state=?,revision=?,record=?,updated_at=? WHERE id=?",
            params![
                run.state,
                run.revision,
                serde_json::to_string(&run)?,
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )?;
        tx.commit()?;
        Ok(run)
    }

    pub(crate) fn study_pending_goals(&self) -> Result<Vec<Value>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT record FROM study_sessions WHERE state='completed' AND json_extract(record,'$.goal_mode')=1 ORDER BY updated_at DESC,rowid DESC")?;
        let mut goals = vec![];
        for record in stmt.query_map([], |r| r.get::<_, String>(0))? {
            let run: StudySession = serde_json::from_str(&record?)?;
            if !run.goal_finished() {
                goals.push(json!({"id":run.id,"title":run.goal}));
                if goals.len() == 3 {
                    break;
                }
            }
        }
        Ok(goals)
    }
}
