//! A transparent practice schedule based only on submitted quiz results.
use crate::{
    db::{Database, DbError},
    review_agent::{ReviewQuestion, ReviewRun, ReviewScope},
};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewMemory {
    pub id: String,
    pub kb: String,
    pub version: String,
    pub chapter_path: Vec<String>,
    pub topic: String,
    pub question: String,
    pub attempts: i64,
    pub correct_count: i64,
    pub lapses: i64,
    pub streak: i64,
    pub last_correct: bool,
    pub last_reviewed_at: i64,
    pub due_at: i64,
}

impl ReviewMemory {
    pub fn in_scope(&self, scope: &ReviewScope) -> bool {
        self.kb == scope.kb
            && self.version == scope.version
            && self.chapter_path.starts_with(&scope.chapter_path)
    }
    fn public(&self, now: i64) -> Value {
        json!({"id":self.id,"topic":self.topic,"question":self.question,"chapter_path":self.chapter_path,
            "attempts":self.attempts,"correct_count":self.correct_count,"lapses":self.lapses,"streak":self.streak,
            "last_correct":self.last_correct,"last_reviewed_at":DateTime::from_timestamp(self.last_reviewed_at,0).map(|v|v.to_rfc3339()),
            "due_at":DateTime::from_timestamp(self.due_at,0).map(|v|v.to_rfc3339()),"is_due":self.due_at<=now})
    }
}

pub fn next_interval_seconds(correct: bool, streak: i64) -> i64 {
    if !correct {
        return 10 * 60;
    }
    let days = [1, 3, 7, 14, 30, 60];
    days[streak.saturating_sub(1).clamp(0, 5) as usize] * 86400
}

pub fn question_path(run: &ReviewRun, question: &ReviewQuestion) -> Vec<String> {
    let paths: Vec<Vec<String>> = run
        .sources
        .iter()
        .filter(|s| question.source_ids.iter().any(|id| s["id"] == *id))
        .filter_map(|s| serde_json::from_value(s["chapter_path"].clone()).ok())
        .collect();
    let Some(first) = paths.first() else {
        return run.scope.chapter_path.clone();
    };
    let length = (0..first.len())
        .take_while(|&i| paths.iter().all(|p| p.get(i) == first.get(i)))
        .count();
    first[..length].to_vec()
}

fn new_id(run: &ReviewRun, q: &ReviewQuestion) -> String {
    let topic: String = q
        .topic
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let key = json!([
        run.scope.kb,
        run.scope.version,
        question_path(run, q),
        q.card_id,
        topic
    ]);
    format!("{:x}", Sha256::digest(key.to_string().as_bytes()))
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewMemory> {
    let path: String = r.get(3)?;
    let chapter_path = serde_json::from_str(&path).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(ReviewMemory {
        id: r.get(0)?,
        kb: r.get(1)?,
        version: r.get(2)?,
        chapter_path,
        topic: r.get(4)?,
        question: r.get(5)?,
        attempts: r.get(6)?,
        correct_count: r.get(7)?,
        lapses: r.get(8)?,
        streak: r.get(9)?,
        last_correct: r.get(10)?,
        last_reviewed_at: r.get(11)?,
        due_at: r.get(12)?,
    })
}

// This runs in the same transaction as the tool checkpoint and linked card state.
pub fn save_grade(
    tx: &Transaction<'_>,
    run: &ReviewRun,
    q: &ReviewQuestion,
    now: i64,
) -> Result<bool, DbError> {
    let correct = q
        .correct
        .ok_or_else(|| DbError::Validation("尚未记录实际答案".into()))?;
    let selected = q
        .selected_index
        .ok_or_else(|| DbError::Validation("用户尚未提交答案".into()))?;
    if selected >= q.options.len()
        || q.correct_index >= q.options.len()
        || correct != (selected == q.correct_index)
    {
        return Err(DbError::Validation("答题记录与实际选择不一致".into()));
    }
    if tx
        .query_row(
            "SELECT 1 FROM review_memory_events WHERE run_id=? AND question_id=?",
            params![run.id, q.id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(false);
    }
    let id = q.memory_id.clone().unwrap_or_else(|| new_id(run, q));
    let existing = tx
        .query_row("SELECT * FROM review_memory WHERE id=?", [&id], row)
        .optional()?;
    if existing.as_ref().is_some_and(|m| !m.in_scope(&run.scope)) {
        return Err(DbError::Validation("知识点超出当前教材版本或章节".into()));
    }
    let old_streak = existing.as_ref().map_or(0, |m| m.streak);
    let streak = if correct {
        old_streak.saturating_add(1)
    } else {
        0
    };
    let path = existing
        .as_ref()
        .map_or_else(|| question_path(run, q), |m| m.chapter_path.clone());
    tx.execute("INSERT INTO review_memory VALUES(?,?,?,?,?,?,1,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET topic=excluded.topic,question=excluded.question,attempts=review_memory.attempts+1,correct_count=review_memory.correct_count+excluded.correct_count,lapses=review_memory.lapses+excluded.lapses,streak=excluded.streak,last_correct=excluded.last_correct,last_reviewed_at=excluded.last_reviewed_at,due_at=excluded.due_at",
        params![id,run.scope.kb,run.scope.version,serde_json::to_string(&path)?,q.topic,q.question,i64::from(correct),i64::from(!correct),streak,correct,now,now+next_interval_seconds(correct,streak)])?;
    tx.execute(
        "INSERT INTO review_memory_events VALUES(?,?,?,?,?)",
        params![run.id, q.id, id, now, correct],
    )?;
    Ok(true)
}

pub fn backfill(tx: &Transaction<'_>) -> Result<(), DbError> {
    let records = tx
        .prepare("SELECT record,updated_at FROM review_runs ORDER BY updated_at,rowid")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut grades = Vec::new();
    for (raw, updated) in records {
        let run: ReviewRun = serde_json::from_str(&raw)?;
        for q in run
            .questions
            .iter()
            .filter(|q| q.correct.is_some() && q.selected_index.is_some())
        {
            let timestamp = run
                .trace
                .iter()
                .find(|e| {
                    e.name == "record_quiz_result"
                        && e.status == "succeeded"
                        && e.details["result"]["question_id"] == q.id
                })
                .and_then(|e| e.finished_at.as_ref())
                .unwrap_or(&updated);
            let now = DateTime::parse_from_rfc3339(timestamp)
                .map_err(|_| DbError::Validation("历史答题时间无效".into()))?
                .timestamp();
            grades.push((now, run.clone(), q.clone()));
        }
    }
    grades.sort_by_key(|(now, run, q)| (*now, run.id.clone(), q.id.clone()));
    for (now, run, q) in grades {
        save_grade(tx, &run, &q, now)?;
    }
    Ok(())
}

impl Database {
    pub fn review_memory_get(&self, id: &str) -> Result<Option<ReviewMemory>, DbError> {
        Ok(self
            .connect()?
            .query_row("SELECT * FROM review_memory WHERE id=?", [id], row)
            .optional()?)
    }
    pub fn review_memory_overview(
        &self,
        scope: &ReviewScope,
        now: DateTime<Utc>,
    ) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT * FROM review_memory WHERE kb=? AND version=? ORDER BY due_at,id")?;
        let mut items = stmt
            .query_map(params![scope.kb, scope.version], row)?
            .collect::<Result<Vec<_>, _>>()?;
        items.retain(|m| m.in_scope(scope));
        let due_count = items.iter().filter(|m| m.due_at <= now.timestamp()).count();
        let weak_count = items.iter().filter(|m| !m.last_correct).count();
        let total = items.len();
        // Due items first; within the due group prioritize the most recent mistakes.
        items.sort_by_key(|m| {
            (
                m.due_at > now.timestamp(),
                m.last_correct,
                m.due_at,
                m.id.clone(),
            )
        });
        Ok(
            json!({"total":total,"due_count":due_count,"weak_count":weak_count,"as_of":now.to_rfc3339(),
            "items":items.iter().take(12).map(|m|m.public(now.timestamp())).collect::<Vec<_>>(),
            "note":"按实际选择与题目参考答案比对安排复习，不代表已核验的掌握度。答错10分钟后，连续答对按1、3、7、14、30、60天安排。"}),
        )
    }
}
