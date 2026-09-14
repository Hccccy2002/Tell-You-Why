use crate::{
    db::{Database, DbError},
    review_agent::{ReviewQuestion, ReviewRun},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

impl Database {
    pub(crate) fn review_insert(&self, run: &ReviewRun) -> Result<(), DbError> {
        self.connect()?.execute("INSERT INTO review_runs(id,kb,state,record,created_at,updated_at) VALUES (?,?,?,?,?,?)",
            params![run.id,run.scope.kb,run.state,serde_json::to_string(run)?,run.created_at,run.created_at])?;
        Ok(())
    }

    pub(crate) fn review_load(&self, id: &str) -> Result<ReviewRun, DbError> {
        let (record, state, cancel): (String, String, bool) = self.connect()?.query_row(
            "SELECT record,state,cancel_requested FROM review_runs WHERE id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let mut run: ReviewRun = serde_json::from_str(&record)?;
        run.state = state;
        run.cancel_requested = cancel;
        Ok(run)
    }

    pub(crate) fn review_latest(&self, kb: &str) -> Result<Option<ReviewRun>, DbError> {
        let id: Option<String> = self
            .connect()?
            .query_row(
                "SELECT id FROM review_runs WHERE kb=? ORDER BY created_at DESC,rowid DESC LIMIT 1",
                [kb],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| self.review_load(&id)).transpose()
    }

    pub(crate) fn review_history(&self, kb: &str) -> Result<Vec<Value>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM review_runs WHERE kb=? ORDER BY created_at DESC,rowid DESC LIMIT 20",
        )?;
        let ids = stmt
            .query_map([kb], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| self.review_load(&id).map(|run| run.summary()))
            .collect()
    }

    pub(crate) fn review_claim(&self, id: &str) -> Result<ReviewRun, DbError> {
        if self.connect()?.execute("UPDATE review_runs SET state='running',cancel_requested=0 WHERE id=? AND state IN ('ready','paused','failed')", [id])? != 1 {
            return Err(DbError::Validation("任务正在执行或等待答题，请刷新复习进度".into()));
        }
        self.review_load(id)
    }

    // Tool effects and their checkpoint commit together, so replay cannot grade twice.
    pub(crate) fn review_save(
        &self,
        run: &ReviewRun,
        graded: Option<&ReviewQuestion>,
    ) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (state, cancel, previous): (String, bool, String) = tx.query_row(
            "SELECT state,cancel_requested,record FROM review_runs WHERE id=?",
            [&run.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if state != "running" {
            return Err(DbError::Validation("复习任务状态已更新".into()));
        }
        if cancel && run.state != "paused" {
            return Err(DbError::Validation("已请求暂停复习".into()));
        }
        if let Some(quiz) = graded {
            let saved: ReviewRun = serde_json::from_str(&previous)?;
            if !saved.questions.iter().any(|q| {
                q.id == quiz.id
                    && q.selected_index.is_some()
                    && q.selected_index == quiz.selected_index
                    && q.correct_index == quiz.correct_index
            }) {
                return Err(DbError::Validation("缺少已提交的实际答题记录".into()));
            }
            let inserted =
                crate::review_memory::save_grade(&tx, run, quiz, chrono::Utc::now().timestamp())?;
            if let Some(card_id) = quiz.card_id.as_ref().filter(|_| inserted) {
                tx.execute("UPDATE rag_learning_state SET status=CASE WHEN ? THEN CASE WHEN status='new' THEN 'learning' ELSE status END ELSE 'review' END,revision=revision+1,updated_at=? WHERE card_id=? AND EXISTS(SELECT 1 FROM rag_cards c WHERE c.id=card_id AND c.kb=?)",
                    params![quiz.correct,chrono::Utc::now().to_rfc3339(),card_id,run.scope.kb])?;
            }
        }
        tx.execute(
            "UPDATE review_runs SET state=?,record=?,updated_at=? WHERE id=?",
            params![
                run.state,
                serde_json::to_string(run)?,
                chrono::Utc::now().to_rfc3339(),
                run.id
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn review_cancel(&self, id: &str) -> Result<(), DbError> {
        self.connect()?.execute(
            "UPDATE review_runs SET cancel_requested=1 WHERE id=? AND state='running'",
            [id],
        )?;
        Ok(())
    }

    pub(crate) fn review_recover(&self) -> Result<(), DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id FROM review_runs WHERE state='running'")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            let mut run = self.review_load(&id)?;
            run.trace_close_open("interrupted");
            run.trace_state("recovered");
            run.state = "paused".into();
            run.error = Some("上次执行中断，已恢复保存的步骤，可继续复习".into());
            run.control.stop = Some(crate::harness::policy::StopReason::new(
                "interrupted",
                "上次执行中断，已恢复保存的步骤，可继续复习",
                true,
            ));
            self.review_save(&run, None)?;
        }
        Ok(())
    }

    pub(crate) fn review_answer(
        &self,
        id: &str,
        question_id: &str,
        selected: usize,
    ) -> Result<ReviewRun, DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (record, state): (String, String) = tx.query_row(
            "SELECT record,state FROM review_runs WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut run: ReviewRun = serde_json::from_str(&record)?;
        run.state = state;
        let quiz = run
            .questions
            .iter_mut()
            .find(|q| q.id == question_id)
            .ok_or_else(|| DbError::Validation("题目不属于本次复习".into()))?;
        if let Some(previous) = quiz.selected_index {
            if previous == selected {
                return Ok(run);
            }
            return Err(DbError::Validation(
                "本题已经提交，不能覆盖实际答题记录".into(),
            ));
        }
        if run.state != "waiting_answer" || selected >= quiz.options.len() {
            return Err(DbError::Validation("当前不能提交该选项".into()));
        }
        quiz.selected_index = Some(selected);
        run.messages.push(json!({"role":"user","content":json!({"submitted_question_id":question_id,"selected_index":selected,"selected_option":quiz.options[selected]}).to_string()}));
        let seq = run.trace_begin(
            "user",
            "answer_submitted",
            json!({"question_id":question_id,"selected_index":selected}),
        );
        run.trace_finish(seq, "succeeded");
        run.state = "ready".into();
        run.error = None;
        run.completion.last_report = None;
        tx.execute("UPDATE review_runs SET state='ready',record=?,cancel_requested=0,updated_at=? WHERE id=?",params![serde_json::to_string(&run)?,chrono::Utc::now().to_rfc3339(),id])?;
        tx.commit()?;
        Ok(run)
    }

    pub(crate) fn review_progress(&self, run: &ReviewRun) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT c.record,s.status FROM rag_cards c JOIN rag_learning_state s ON s.card_id=c.id WHERE c.kb=? ORDER BY CASE s.status WHEN 'review' THEN 0 WHEN 'learning' THEN 1 ELSE 2 END,c.created_at DESC")?;
        let rows = stmt.query_map([&run.scope.kb], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut cards = Vec::new();
        for row in rows {
            let (record, status) = row?;
            let card: Value = serde_json::from_str(&record)?;
            if card["packet"]["version"] == run.scope.version
                && crate::learning::in_scope(&card, &run.scope.chapter_path)
            {
                cards.push(json!({"card_id":card["id"],"question":card["result"]["question"],"status":status}));
                if cards.len() >= 12 {
                    break;
                }
            }
        }
        let mut stmt = conn.prepare(
            "SELECT record FROM review_runs WHERE kb=? ORDER BY created_at DESC LIMIT 10",
        )?;
        let records = stmt.query_map([&run.scope.kb], |r| r.get::<_, String>(0))?;
        let mut results = vec![];
        for record in records {
            let old: ReviewRun = serde_json::from_str(&record?)?;
            if old.scope.version == run.scope.version
                && old.scope.chapter_path.starts_with(&run.scope.chapter_path)
            {
                results.extend(
                    old.questions
                        .into_iter()
                        .filter(|q| q.correct.is_some())
                        .map(
                            |q| json!({"topic":q.topic,"question":q.question,"correct":q.correct}),
                        ),
                );
            }
        }
        Ok(
            json!({"cards":cards,"recent_quiz_results":results,"review_memory":self.review_memory_overview(&run.scope,chrono::Utc::now())?,"note":"没有记录表示尚未评估，不能推断用户掌握程度。"}),
        )
    }
}
