use crate::{
    db::{Database, DbError},
    study::{StudyReviewTarget, StudySession},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

pub(crate) struct QuestionOptions<'a> {
    pub reply_to: Option<&'a str>,
    pub force_search: bool,
}

impl Database {
    pub(crate) fn study_save_highlight(
        &self,
        id: &str,
        kind: &str,
        source_id: &str,
    ) -> Result<String, DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let run: StudySession = serde_json::from_str(&record)?;
        let (title, text, reference) = match kind {
            "step" => {
                let step = run
                    .steps
                    .iter()
                    .find(|s| s.id == source_id && s.quiz.is_none())
                    .ok_or_else(|| DbError::Validation("只能保存本次学习中已展示的讲解".into()))?;
                (&step.title, &step.text, &step.card_id)
            }
            "question" => {
                let question = run
                    .questions
                    .iter()
                    .find(|q| q.id == source_id)
                    .ok_or_else(|| DbError::Validation("问题不属于本次学习".into()))?;
                let answer = question
                    .answer
                    .as_ref()
                    .ok_or_else(|| DbError::Validation("请等待回答完成后再保存".into()))?;
                (&question.question, &answer.text, &answer.card_id)
            }
            _ => return Err(DbError::Validation("无法保存这种内容".into())),
        };
        let card_id = run
            .source_card
            .as_ref()
            .map(|c| &c.card_id)
            .or(reference.as_ref());
        let highlight_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT OR IGNORE INTO study_highlights VALUES (?,?,?,?,?,?,?,?)",
            params![
                highlight_id,
                id,
                kind,
                source_id,
                card_id,
                title,
                text,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        let saved = tx.query_row(
            "SELECT id FROM study_highlights WHERE session_id=? AND source_kind=? AND source_id=?",
            params![id, kind, source_id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(saved)
    }

    pub(crate) fn study_highlights(
        &self,
        session_id: Option<&str>,
        card_id: Option<&str>,
    ) -> Result<Vec<Value>, DbError> {
        if session_id.is_some() == card_id.is_some() {
            return Err(DbError::Validation("请选择一次学习或一张卡片".into()));
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id,session_id,source_kind,source_id,title,text,created_at FROM study_highlights WHERE (?1 IS NOT NULL AND session_id=?1) OR (?2 IS NOT NULL AND card_id=?2) ORDER BY created_at DESC,rowid DESC")?;
        let mut rows = stmt.query_map(params![session_id,card_id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"session_id":r.get::<_,String>(1)?,"source_kind":r.get::<_,String>(2)?,"source_id":r.get::<_,String>(3)?,"title":r.get::<_,String>(4)?,"text":r.get::<_,String>(5)?,"created_at":r.get::<_,String>(6)?})))?.collect::<Result<Vec<_>, _>>()?;
        // Answers are immutable; retain their canonical evidence association even
        // after re-explanation, cache expiry, deleted source cards or restart.
        let mut sessions = std::collections::HashMap::new();
        for row in &mut rows {
            if row["source_kind"] != "question" {
                continue;
            }
            let id = row["session_id"].as_str().unwrap();
            if !sessions.contains_key(id) {
                sessions.insert(id.to_owned(), self.study_load(id)?);
            }
            let source = row["source_id"].as_str().unwrap();
            let search = sessions[id]
                .questions
                .iter()
                .find(|q| q.id == source)
                .and_then(|q| q.answer.as_ref())
                .and_then(|a| a.search.as_ref());
            row["search"] = serde_json::to_value(search)?;
        }
        Ok(rows)
    }

    pub(crate) fn study_remove_highlight(&self, id: &str) -> Result<(), DbError> {
        self.connect()?
            .execute("DELETE FROM study_highlights WHERE id=?", [id])?;
        Ok(())
    }

    pub(crate) fn study_card_sessions(&self, card_id: &str) -> Result<Vec<Value>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT record FROM study_sessions WHERE json_extract(record,'$.source_card.card_id')=? ORDER BY updated_at DESC,rowid DESC LIMIT 20")?;
        let records = stmt
            .query_map([card_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        records.into_iter().map(|record| {
            let run: StudySession = serde_json::from_str(&record)?;
            Ok(json!({"id":run.id,"goal":run.goal,"state":run.state,"created_at":run.created_at,"step_count":run.steps.len()}))
        }).collect()
    }

    pub(crate) fn study_source_card(
        &self,
        id: &str,
    ) -> Result<Option<crate::models::KnowledgeCard>, DbError> {
        let run = self.study_load(id)?;
        let Some(source) = run.source_card else {
            return Ok(None);
        };
        match self.card_by_id(&source.card_id) {
            Ok(card) => Ok(Some(card)),
            Err(DbError::NoCards) => Ok(None),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    pub(crate) fn study_ask(
        &self,
        id: &str,
        step_id: &str,
        question: &str,
        request_id: &str,
    ) -> Result<StudySession, DbError> {
        self.study_ask_with_reply(id, step_id, question, request_id, None)
    }

    #[cfg(test)]
    pub(crate) fn study_ask_with_reply(
        &self,
        id: &str,
        step_id: &str,
        question: &str,
        request_id: &str,
        reply_to_question_id: Option<&str>,
    ) -> Result<StudySession, DbError> {
        self.study_ask_with_options(
            id,
            step_id,
            question,
            request_id,
            QuestionOptions {
                reply_to: reply_to_question_id,
                force_search: false,
            },
        )
    }

    pub(crate) fn study_ask_with_options(
        &self,
        id: &str,
        step_id: &str,
        question: &str,
        request_id: &str,
        options: QuestionOptions<'_>,
    ) -> Result<StudySession, DbError> {
        let reply_to_question_id = options.reply_to;
        let question = question.trim();
        if question.is_empty()
            || question.chars().count() > 300
            || question
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
            || uuid::Uuid::parse_str(request_id).is_err()
        {
            return Err(DbError::Validation(
                "请输入1–300字的问题，并使用有效请求编号".into(),
            ));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut run: StudySession = serde_json::from_str(&record)?;
        if let Some(saved) = run.questions.iter().find(|q| q.id == request_id) {
            let submitted = if saved.reply_to_question_id.is_some() {
                saved.clarification_replies.last().map(|c| c.reply.as_str())
            } else {
                Some(saved.question.as_str())
            };
            if saved.step_id == step_id
                && saved.reply_to_question_id.as_deref() == reply_to_question_id
                && submitted == Some(question)
                && saved.search.as_ref().is_some_and(|s| s.force_refresh) == options.force_search
            {
                return Ok(run);
            }
            return Err(DbError::Validation(
                "该请求已保存，不能覆盖为另一个问题".into(),
            ));
        }
        if !run.can_ask() || run.steps.last().map_or(true, |s| s.id != step_id) {
            return Err(DbError::Validation(
                "当前不能提问：请等待回答完成；练习需先提交答案，每次短学习最多4个问题".into(),
            ));
        }
        let title = run.steps.last().unwrap().title.clone();
        let mut next_question = crate::study::StudyQuestion {
            id: request_id.into(),
            step_id: step_id.into(),
            question: question.into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            answer: None,
            feedback: None,
            doubt_id: None,
            previous_answers: vec![],
            reply_to_question_id: reply_to_question_id.map(str::to_owned),
            clarification_replies: vec![],
            search: Some(crate::study_search::StudySearchRun::new(
                self.search_options()?,
                options.force_search,
            )),
        };
        if let Some(parent_id) = reply_to_question_id {
            let index = run
                .questions
                .iter()
                .position(|q| q.id == parent_id)
                .ok_or_else(|| DbError::Validation("找不到对应的澄清问题".into()))?;
            let parent = &run.questions[index];
            let answer = parent
                .answer
                .as_ref()
                .filter(|answer| answer.kind == "clarification")
                .ok_or_else(|| DbError::Validation("只能补充已完成的澄清问题".into()))?;
            let doubt_id = parent
                .doubt_id
                .clone()
                .unwrap_or_else(|| format!("{}:{}", run.id, parent.id));
            if parent.step_id != step_id
                || parent.feedback.as_deref() == Some("understood")
                || run.questions[index + 1..]
                    .iter()
                    .any(|q| q.doubt_id.as_deref() == Some(&doubt_id))
            {
                return Err(DbError::Validation(
                    "请在当前步骤的最新澄清问题处补充".into(),
                ));
            }
            let current: Option<(String, String)> = tx
                .query_row(
                    "SELECT session_id,question_id FROM study_doubts WHERE id=?",
                    [&doubt_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if current.is_some_and(|(session, q)| session != id || q != parent_id) {
                return Err(DbError::Validation(
                    "这个疑问已有后续回答，请返回最新回答补充".into(),
                ));
            }
            next_question.question = parent.question.clone();
            next_question.doubt_id = Some(doubt_id.clone());
            next_question.previous_answers = parent.previous_answers.clone();
            next_question.previous_answers.push(answer.clone());
            if next_question.previous_answers.len() > 3 {
                next_question.previous_answers.remove(0);
            }
            next_question.clarification_replies = parent.clarification_replies.clone();
            next_question
                .clarification_replies
                .push(crate::study::StudyClarification {
                    prompt: answer.text.clone(),
                    reply: question.into(),
                });
            if next_question.clarification_replies.len() > 3 {
                next_question.clarification_replies.remove(0);
            }
            run.questions[index].doubt_id = Some(doubt_id.clone());
            // A clarification alone is not feedback. Move an existing doubt, but
            // do not invent an unresolved one before the user says "还没懂".
            tx.execute(
                "UPDATE study_doubts SET session_id=?,question_id=?,updated_at=? WHERE id=?",
                params![id, request_id, chrono::Utc::now().to_rfc3339(), doubt_id],
            )?;
        }
        run.messages.push(json!({"role":"user","content":json!({"kind":if reply_to_question_id.is_some() { "clarification_reply" } else { "study_question" },"request_id":request_id,"step_id":step_id,"step_title":title,"question":next_question}).to_string()}));
        run.questions.push(next_question);
        run.state = "ready".into();
        run.error = None;
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
        tx.commit()?;
        Ok(run)
    }

    pub(crate) fn study_home(&self) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let active: Option<(String, String)> = conn.query_row(
            "SELECT record,updated_at FROM study_sessions WHERE state <> 'completed' ORDER BY updated_at DESC,rowid DESC LIMIT 1",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).optional()?;
        let active = active
            .map(|(record, updated)| -> Result<Value, DbError> {
                let run: StudySession = serde_json::from_str(&record)?;
                Ok(
                    json!({"id":run.id,"goal":run.goal,"topic":run.topic,"state":run.state,
                "step_count":run.steps.len(),"last_title":if run.goal_mode {Some(&run.goal)} else {run.steps.last().map(|s|&s.title)},
                "updated_at":updated}),
                )
            })
            .transpose()?;
        let enabled = self.settings()?.personalization_enabled;
        let now = chrono::Utc::now().timestamp();
        let (practice_count, due_count, next_due): (i64, i64, Option<i64>) = if enabled {
            conn.query_row("SELECT COUNT(*),COALESCE(SUM(due_at<=?1),0),MIN(CASE WHEN due_at>?1 THEN due_at END) FROM study_memory", [now], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?
        } else {
            (0, 0, None)
        };
        let mut due = vec![];
        if enabled {
            let mut stmt = conn.prepare("SELECT concept_key,topic,title,attempts,last_correct,due_at FROM study_memory WHERE due_at<=? ORDER BY due_at,concept_key LIMIT 3")?;
            due = stmt.query_map([now], |r| Ok(json!({"concept_key":r.get::<_,String>(0)?,"topic":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"attempts":r.get::<_,i64>(3)?,"last_correct":r.get::<_,bool>(4)?,"due_at":r.get::<_,i64>(5)?})))?.collect::<Result<Vec<_>,_>>()?;
        }
        Ok(
            json!({"active":active,"goals":self.study_pending_goals()?,"due":due,"due_count":due_count,"practice_count":practice_count,"next_due_at":next_due,"personalization_enabled":enabled,"doubts":if enabled { self.study_open_doubts()? } else { vec![] }}),
        )
    }

    pub(crate) fn study_review_target(&self, key: &str) -> Result<StudyReviewTarget, DbError> {
        if !self.settings()?.personalization_enabled {
            return Err(DbError::Validation(
                "个性化已关闭，暂不读取过往练习来安排巩固".into(),
            ));
        }
        let conn = self.connect()?;
        let memory: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT topic,title,due_at FROM study_memory WHERE concept_key=?",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (topic, title, due) = memory
            .ok_or_else(|| DbError::Validation("这条练习记录已不存在，请返回小窗刷新".into()))?;
        if due > chrono::Utc::now().timestamp() {
            return Err(DbError::Validation(
                "这条知识已安排在稍后巩固，请返回小窗刷新".into(),
            ));
        }
        // Resolve an actual submitted quiz, including records created before stable IDs existed.
        let mut stmt = conn.prepare("SELECT j.value FROM study_sessions s,json_each(s.record,'$.steps') j WHERE json_extract(j.value,'$.quiz.selected') IS NOT NULL AND (json_extract(j.value,'$.concept_key')=?1 OR json_extract(j.value,'$.card_id')=?1 OR (json_extract(s.record,'$.topic')=?2 AND json_extract(j.value,'$.title')=?3)) ORDER BY s.updated_at DESC,s.rowid DESC,j.key DESC")?;
        let rows = stmt.query_map(params![key, topic, title], |r| r.get::<_, String>(0))?;
        for row in rows {
            let step: crate::study::StudyStep = serde_json::from_str(&row?)?;
            if step.memory_key(&topic) != key {
                continue;
            }
            if let Some(quiz) = step.quiz.filter(|q| {
                q.selected.is_some_and(|a| a < q.options.len()) && q.correct_index < q.options.len()
            }) {
                return Ok(StudyReviewTarget {
                    concept_key: key.into(),
                    topic,
                    title,
                    previous_question: step.text,
                    previous_quiz: quiz,
                });
            }
        }
        Err(DbError::Validation(
            "找不到这条练习的原始作答，暂时无法安排巩固".into(),
        ))
    }

    pub(crate) fn study_history(&self) -> Result<Vec<Value>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT record FROM study_sessions ORDER BY updated_at DESC,rowid DESC LIMIT 20",
        )?;
        let records = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|record| Ok(serde_json::from_str::<StudySession>(&record)?.public()))
            .collect()
    }

    pub(crate) fn study_reset(&self) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let running: i64 = tx.query_row(
            "SELECT COUNT(*) FROM study_sessions WHERE state='running'",
            [],
            |r| r.get(0),
        )?;
        if running > 0 {
            return Err(DbError::Validation("请先暂停正在进行的学习".into()));
        }
        tx.execute("DELETE FROM study_memory", [])?;
        tx.execute("DELETE FROM study_highlights", [])?;
        tx.execute("DELETE FROM study_doubts", [])?;
        tx.execute("DELETE FROM study_sessions", [])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn study_search_cards(
        &self,
        query: &str,
    ) -> Result<Vec<crate::models::KnowledgeCard>, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT c.id FROM cards c LEFT JOIN card_user_state s ON s.card_id=c.id WHERE COALESCE(s.deleted,0)=0 AND COALESCE(s.disliked,0)=0 AND (?='' OR instr(lower(c.question || ' ' || c.topic_label || ' ' || c.tags_json),lower(?))>0) ORDER BY COALESCE(s.favorite,0) DESC,c.created_at DESC LIMIT 8")?;
        let ids = stmt
            .query_map(params![query, query], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter().map(|id| self.card_by_id(&id)).collect()
    }

    pub(crate) fn study_context(&self, topic: &str) -> Result<Value, DbError> {
        let personalized = self.settings()?.personalization_enabled;
        let memories = if personalized {
            self.study_memories(topic)?
        } else {
            json!([])
        };
        let interests: Vec<_> = if personalized {
            self.topics()?
                .into_iter()
                .filter(|t| t.selected && t.enabled)
                .map(|t| t.label)
                .collect()
        } else {
            vec![]
        };
        let mut recent = vec![];
        if personalized {
            let conn = self.connect()?;
            let mut stmt =
                conn.prepare("SELECT record FROM study_sessions ORDER BY updated_at DESC LIMIT 8")?;
            for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
                let session: StudySession = serde_json::from_str(&row?)?;
                if session.topic == topic {
                    recent.extend(
                        session
                            .steps
                            .iter()
                            .filter(|s| s.feedback.is_some())
                            .map(|s| json!({"title":s.title,"feedback":s.feedback})),
                    );
                }
            }
        }
        recent.truncate(12);
        Ok(
            json!({"personalization_enabled":personalized,"interests":interests,"practice_records":memories,"recent_feedback":recent,"note":"阅读、收藏或一次答对不等于掌握。练习记录依据实际提交和该题参考答案，不代表外部核验。"}),
        )
    }

    pub(crate) fn study_insert(&self, session: &StudySession) -> Result<(), DbError> {
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
        tx.execute(
            "INSERT INTO study_sessions VALUES (?, ?, ?, ?, ?, ?)",
            params![
                session.id,
                session.state,
                session.revision,
                serde_json::to_string(session)?,
                session.created_at,
                session.created_at
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn study_load(&self, id: &str) -> Result<StudySession, DbError> {
        let record: String = self.connect()?.query_row(
            "SELECT record FROM study_sessions WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&record)?)
    }

    pub(crate) fn study_latest(&self) -> Result<Option<StudySession>, DbError> {
        let record: Option<String> = self
            .connect()?
            .query_row(
                "SELECT record FROM study_sessions ORDER BY updated_at DESC,rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        record
            .map(|s| serde_json::from_str(&s).map_err(DbError::from))
            .transpose()
    }

    // Every write checks the revision; late network responses cannot undo a pause/end.
    pub(crate) fn study_save(&self, session: &mut StudySession) -> Result<(), DbError> {
        let before = session.revision;
        let mut next = session.clone();
        next.revision += 1;
        let changed = self.connect()?.execute("UPDATE study_sessions SET state=?,revision=?,record=?,updated_at=? WHERE id=? AND revision=?", params![next.state,next.revision,serde_json::to_string(&next)?,chrono::Utc::now().to_rfc3339(),next.id,before])?;
        if changed != 1 {
            return Err(DbError::Validation("学习进度已更新，请刷新后继续".into()));
        }
        *session = next;
        Ok(())
    }

    pub(crate) fn study_claim(&self, id: &str) -> Result<StudySession, DbError> {
        let mut session = self.study_load(id)?;
        if !["ready", "paused", "failed"].contains(&session.state.as_str())
            || session.public()["can_resume"] != true
        {
            return Err(DbError::Validation(
                "当前学习不能继续，请刷新进度或结束本次学习".into(),
            ));
        }
        session.state = "running".into();
        session.error = None;
        session.control.stop = None;
        self.study_save(&mut session)?;
        Ok(session)
    }

    pub(crate) fn study_pause(&self, id: &str, finish: bool) -> Result<StudySession, DbError> {
        // Retry only optimistic local writes; this never repeats a model request.
        for _ in 0..4 {
            let mut session = self.study_load(id)?;
            if session.state == "completed" {
                return Ok(session);
            }
            if !finish && session.state != "running" {
                return Ok(session);
            }
            session.state = if finish { "completed" } else { "paused" }.into();
            if let Some(search) = session
                .questions
                .last_mut()
                .filter(|q| q.answer.is_none())
                .and_then(|q| q.search.as_mut())
            {
                if search.is_active() {
                    search.stage = if finish { "cancelled" } else { "paused" }.into();
                }
            }
            session.error = None;
            if finish {
                // Goal sessions may be reopened with their original budget and
                // tool transcript. Preserve a checkpoint to keep tool replies paired.
                if !session.goal_mode {
                    session.pending = None;
                }
                if session.next_topic.is_none() {
                    session.next_topic = Some(session.goal.clone());
                }
            }
            match self.study_save(&mut session) {
                Ok(()) => return Ok(session),
                Err(DbError::Validation(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(DbError::Validation("学习进度正在更新，请再试一次".into()))
    }

    pub(crate) fn study_recover(&self) -> Result<(), DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id FROM study_sessions WHERE state='running'")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            self.study_pause(&id, false)?;
        }
        Ok(())
    }

    pub(crate) fn study_feedback(
        &self,
        id: &str,
        step_id: &str,
        feedback: &str,
        selected: Option<usize>,
    ) -> Result<StudySession, DbError> {
        if ![
            "continue",
            "confused",
            "easy",
            "example",
            "skip",
            "answer",
            "understood",
        ]
        .contains(&feedback)
        {
            return Err(DbError::Validation("不支持的学习反馈".into()));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: String =
            tx.query_row("SELECT record FROM study_sessions WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut session: StudySession = serde_json::from_str(&record)?;
        if feedback == "understood" && !session.goal_mode {
            return Err(DbError::Validation("仅目标学习支持这个讲解反馈".into()));
        }
        let current_step = session.steps.last().map(|s| s.id.clone());
        let step = session
            .steps
            .iter_mut()
            .find(|s| s.id == step_id)
            .ok_or_else(|| DbError::Validation("内容不属于本次学习".into()))?;
        if let Some(previous) = &step.feedback {
            if previous == feedback && step.quiz.as_ref().and_then(|q| q.selected) == selected {
                return Ok(session);
            }
            return Err(DbError::Validation("这一步已提交反馈，不能重复覆盖".into()));
        }
        if session.state != "waiting" {
            return Err(DbError::Validation("请等待当前步骤完成".into()));
        }
        if session.goal_mode
            && (current_step.as_deref() != Some(step_id)
                || session.questions.last().is_some_and(|q| q.answer.is_none()))
        {
            return Err(DbError::Validation(
                "请先完成当前问答，再对当前步骤反馈".into(),
            ));
        }
        let mut result = json!({"step_id":step.id,"title":step.title,"feedback":feedback});
        if feedback == "answer" {
            let quiz = step
                .quiz
                .as_mut()
                .ok_or_else(|| DbError::Validation("当前内容不是练习题".into()))?;
            let answer = selected
                .filter(|&s| s < quiz.options.len())
                .ok_or_else(|| DbError::Validation("请选择有效答案".into()))?;
            quiz.selected = Some(answer);
            let correct = answer == quiz.correct_index;
            result["selected"] = json!(answer);
            result["correct"] = json!(correct);
            result["explanation"] = json!(quiz.explanation);
            let concept_key = step
                .concept_key
                .clone()
                .or_else(|| step.card_id.clone())
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        session.topic,
                        step.title
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .flat_map(char::to_lowercase)
                            .collect::<String>()
                    )
                });
            let streak: i64 = tx
                .query_row(
                    "SELECT streak FROM study_memory WHERE concept_key=?",
                    [&concept_key],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(0);
            let streak = if correct { streak + 1 } else { 0 };
            let due = chrono::Utc::now().timestamp()
                + crate::review_memory::next_interval_seconds(correct, streak);
            let title = session
                .review_target
                .as_ref()
                .map_or(step.title.as_str(), |r| r.title.as_str());
            tx.execute("INSERT INTO study_memory VALUES (?,?,?,1,?,?,?,?,?) ON CONFLICT(concept_key) DO UPDATE SET title=excluded.title,attempts=attempts+1,correct_count=correct_count+excluded.correct_count,streak=excluded.streak,due_at=excluded.due_at,last_correct=excluded.last_correct,updated_at=excluded.updated_at",params![concept_key,session.topic,title,i64::from(correct),streak,due,correct,chrono::Utc::now().to_rfc3339()])?;
        } else if selected.is_some()
            || (step.quiz.is_some() && feedback != "skip")
            || (step.quiz.is_none() && feedback == "skip")
        {
            return Err(DbError::Validation("反馈与当前内容不匹配".into()));
        }
        step.feedback = Some(feedback.into());
        session
            .messages
            .push(json!({"role":"user","content":result.to_string()}));
        session.state = "ready".into();
        session.error = None;
        session.revision += 1;
        tx.execute(
            "UPDATE study_sessions SET state=?,revision=?,record=?,updated_at=? WHERE id=?",
            params![
                session.state,
                session.revision,
                serde_json::to_string(&session)?,
                chrono::Utc::now().to_rfc3339(),
                session.id
            ],
        )?;
        tx.commit()?;
        Ok(session)
    }

    pub(crate) fn study_memories(&self, topic: &str) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT title,attempts,correct_count,due_at,last_correct FROM study_memory WHERE topic=? ORDER BY due_at LIMIT 12")?;
        let values = stmt.query_map([topic], |r| {
            let due: i64 = r.get(3)?;
            Ok(json!({"title":r.get::<_,String>(0)?,"attempts":r.get::<_,i64>(1)?,"correct_count":r.get::<_,i64>(2)?,"due_at":due,"is_due":due<=chrono::Utc::now().timestamp(),"last_correct":r.get::<_,bool>(4)?}))
        })?.collect::<Result<Vec<_>,_>>()?;
        Ok(json!(values))
    }
}
