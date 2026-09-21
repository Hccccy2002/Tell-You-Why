use crate::{
    db::{Database, DbError},
    study::{StudyDoubtTarget, StudyQuestion, StudySession},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

impl Database {
    pub(crate) fn study_open_doubts(&self) -> Result<Vec<Value>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT d.id,s.record,d.question_id FROM study_doubts d JOIN study_sessions s ON s.id=d.session_id WHERE d.status='unresolved' ORDER BY d.updated_at DESC,d.rowid DESC LIMIT 3")?;
        let records = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|(id, record, question_id)| {
                let run: StudySession = serde_json::from_str(&record)?;
                let q = run
                    .questions
                    .iter()
                    .find(|q| q.id == question_id)
                    .ok_or_else(|| DbError::Validation("疑问记录缺少原始问题".into()))?;
                Ok(json!({"id":id,"question":q.question,"topic":run.topic,"session_id":run.id,"reason":q.doubt_reason()}))
            })
            .collect()
    }

    pub(crate) fn study_question_feedback(
        &self,
        id: &str,
        question_id: &str,
        feedback: &str,
    ) -> Result<StudySession, DbError> {
        if !["understood", "unresolved"].contains(&feedback) {
            return Err(DbError::Validation("不支持的疑问反馈".into()));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut run: StudySession = serde_json::from_str(&record)?;
        let index = run
            .questions
            .iter()
            .position(|q| q.id == question_id)
            .ok_or_else(|| DbError::Validation("问题不属于本次学习".into()))?;
        let mut question = run.questions[index].clone();
        let answer = question
            .answer
            .clone()
            .ok_or_else(|| DbError::Validation("请等待回答完成后再反馈".into()))?;
        if question.feedback.as_deref() == Some(feedback) {
            return Ok(run);
        }
        if run.state == "running" || run.pending_question().is_some() {
            return Err(DbError::Validation(
                "请先等待当前回答完成或结束本次学习".into(),
            ));
        }
        let doubt_id = question
            .doubt_id
            .clone()
            .unwrap_or_else(|| format!("{}:{}", run.id, question.id));
        if run.questions[index + 1..]
            .iter()
            .any(|q| q.doubt_id.as_deref() == Some(&doubt_id))
        {
            return Err(DbError::Validation(
                "这个疑问已有后续回答，请在最新回答处反馈".into(),
            ));
        }
        let current: Option<(String, String)> = tx
            .query_row(
                "SELECT session_id,question_id FROM study_doubts WHERE id=?",
                [&doubt_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if current.is_some_and(|(session, q)| session != id || q != question_id) {
            return Err(DbError::Validation(
                "这个疑问已有后续回答，请在最新回答处反馈".into(),
            ));
        }
        run.questions[index].feedback = Some(feedback.into());
        run.questions[index].doubt_id = Some(doubt_id.clone());
        run.messages.push(json!({"role":"user","content":json!({"kind":"question_feedback","question_id":question_id,"question":question.question,"feedback":feedback,"note":"这是对解释的主观反馈，不是知识掌握成绩。"}).to_string()}));
        let mut last_question_id = question_id.to_string();
        let can_follow_up = feedback == "unresolved"
            && run.can_ask()
            && run.steps.last().is_some_and(|s| s.id == question.step_id);
        if can_follow_up {
            question.previous_answers.push(answer);
            if question.previous_answers.len() > 3 {
                question.previous_answers.remove(0);
            }
            question.id = uuid::Uuid::new_v4().to_string();
            question.created_at = chrono::Utc::now().to_rfc3339();
            question.answer = None;
            question.search = Some(crate::study_search::StudySearchRun::new(
                question
                    .search
                    .as_ref()
                    .map(|s| s.options.clone())
                    .unwrap_or(self.search_options()?),
                false,
            ));
            question.feedback = None;
            question.reply_to_question_id = None;
            question.doubt_id = Some(doubt_id.clone());
            last_question_id = question.id.clone();
            run.messages.push(json!({"role":"user","content":json!({"kind":"question_unresolved","question":question,"feedback":"用户明确反馈还没懂，请结合此前回答换一种方式解释。"}).to_string()}));
            run.questions.push(question);
            run.state = "ready".into();
            run.error = None;
        }
        run.revision += 1;
        tx.execute(
            "UPDATE study_sessions SET state=?,revision=?,record=?,updated_at=? WHERE id=?",
            params![
                run.state,
                run.revision,
                serde_json::to_string(&run)?,
                chrono::Utc::now().to_rfc3339(),
                run.id
            ],
        )?;
        tx.execute("INSERT INTO study_doubts VALUES (?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET session_id=excluded.session_id,question_id=excluded.question_id,status=excluded.status,updated_at=excluded.updated_at", params![doubt_id,id,last_question_id,feedback,chrono::Utc::now().to_rfc3339()])?;
        tx.commit()?;
        Ok(run)
    }

    pub(crate) fn study_insert_doubt(
        &self,
        run: &mut StudySession,
        doubt_id: &str,
    ) -> Result<(), DbError> {
        if !self.settings()?.personalization_enabled {
            return Err(DbError::Validation(
                "个性化已关闭，暂不使用过往疑问开始学习".into(),
            ));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE state <> 'completed'",
            [],
            |r| r.get(0),
        )?;
        if active > 0 {
            return Err(DbError::Validation("请先继续或结束上一次学习".into()));
        }
        let (record, question_id): (String,String) = tx.query_row("SELECT s.record,d.question_id FROM study_doubts d JOIN study_sessions s ON s.id=d.session_id WHERE d.id=? AND d.status='unresolved'", [doubt_id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(|| DbError::Validation("这个疑问已解决或记录已清除，请返回小窗刷新".into()))?;
        let old: StudySession = serde_json::from_str(&record)?;
        let old_question = old
            .questions
            .iter()
            .find(|q| q.id == question_id)
            .ok_or_else(|| DbError::Validation("找不到疑问的原始内容".into()))?;
        let mut context = old
            .steps
            .iter()
            .find(|s| s.id == old_question.step_id)
            .cloned()
            .ok_or_else(|| DbError::Validation("找不到疑问对应的讲解".into()))?;
        if let Some(quiz) = context.quiz.take() {
            // Only an actually submitted quiz can have a saved question.
            let selected = quiz
                .selected
                .and_then(|i| quiz.options.get(i))
                .ok_or_else(|| DbError::Validation("缺少原练习的实际作答".into()))?;
            context.text = format!(
                "{}\n你当时选择：{}\n参考解析：{}",
                context.text, selected, quiz.explanation
            );
        }
        context.id = uuid::Uuid::new_v4().to_string();
        context.kind = "concept".into();
        context.feedback = None;
        let mut previous_answers = old_question.previous_answers.clone();
        if let Some(answer) = &old_question.answer {
            previous_answers.push(answer.clone());
        }
        while previous_answers.len() > 3 {
            previous_answers.remove(0);
        }
        let question = StudyQuestion {
            id: uuid::Uuid::new_v4().to_string(),
            step_id: context.id.clone(),
            question: old_question.question.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            answer: None,
            feedback: None,
            doubt_id: Some(doubt_id.into()),
            previous_answers,
            reply_to_question_id: None,
            clarification_replies: old_question.clarification_replies.clone(),
            search: Some(crate::study_search::StudySearchRun::new(
                self.search_options()?,
                false,
            )),
        };
        run.topic = old.topic;
        run.goal = format!(
            "接着弄明白：{}",
            question.question.chars().take(180).collect::<String>()
        );
        run.source_card = old.source_card;
        run.source_expanded = old.source_expanded;
        run.doubt_target = Some(StudyDoubtTarget {
            id: doubt_id.into(),
            question: question.question.clone(),
            reason: question.doubt_reason(),
        });
        run.messages.push(json!({"role":"user","content":json!({"kind":"resume_unresolved_question","context":context,"question":question}).to_string()}));
        run.steps.push(context);
        run.questions.push(question.clone());
        tx.execute(
            "INSERT INTO study_sessions VALUES (?,?,?,?,?,?)",
            params![
                run.id,
                run.state,
                run.revision,
                serde_json::to_string(run)?,
                run.created_at,
                run.created_at
            ],
        )?;
        tx.execute(
            "UPDATE study_doubts SET session_id=?,question_id=?,updated_at=? WHERE id=?",
            params![run.id, question.id, run.created_at, doubt_id],
        )?;
        tx.commit()?;
        Ok(())
    }
}
