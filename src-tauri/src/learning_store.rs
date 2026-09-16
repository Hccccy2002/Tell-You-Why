use crate::db::{Database, DbError};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearningState {
    pub card_id: String,
    pub status: String,
    pub shown_count: u32,
    pub revealed_count: u32,
    pub first_shown_at: Option<String>,
    pub last_shown_at: Option<String>,
    pub last_revealed_at: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presentation {
    pub id: String,
    pub card_id: String,
    pub confirmed: bool,
    pub revealed: bool,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSession {
    pub id: String,
    pub kb: String,
    pub chapter: Option<String>,
    pub chapter_path: Vec<String>,
    pub filter: String,
    pub relaxed: bool,
    pub history: Vec<Presentation>,
    pub cursor: Option<usize>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningEvent {
    pub id: String,
    pub session_id: String,
    pub presentation_id: String,
    pub kind: String,
    pub status: Option<String>,
    pub expected_revision: u64,
    pub undo_event_id: Option<String>,
}

pub(crate) fn invalid(message: &str) -> DbError {
    DbError::Validation(message.into())
}

pub(crate) fn read_state(conn: &Connection, card: &str) -> Result<LearningState, DbError> {
    Ok(conn.query_row(
        "SELECT card_id,status,shown_count,revealed_count,first_shown_at,last_shown_at,last_revealed_at,revision FROM rag_learning_state WHERE card_id=?",
        [card], |r| Ok(LearningState { card_id:r.get(0)?, status:r.get(1)?, shown_count:r.get(2)?, revealed_count:r.get(3)?, first_shown_at:r.get(4)?, last_shown_at:r.get(5)?, last_revealed_at:r.get(6)?, revision:r.get(7)? }),
    )?)
}

pub(crate) fn read_session(conn: &Connection, id: &str) -> Result<LearningSession, DbError> {
    let text: Option<String> = conn
        .query_row(
            "SELECT record FROM rag_learning_sessions WHERE id=? AND active=1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&text.ok_or_else(|| invalid("学习会话已结束或记录已重置，请重新开始"))?)
        .map_err(Into::into)
}

pub(crate) fn save_session(
    conn: &Connection,
    session: &LearningSession,
    now: DateTime<Utc>,
) -> Result<(), DbError> {
    if conn.execute(
        "UPDATE rag_learning_sessions SET record=?,updated_at=? WHERE id=? AND active=1",
        params![
            serde_json::to_string(session)?,
            now.to_rfc3339(),
            session.id
        ],
    )? != 1
    {
        return Err(invalid("学习会话已失效"));
    }
    Ok(())
}

impl Database {
    pub fn learning_resume(&self, kb: &str) -> Result<Option<LearningSession>, DbError> {
        let text: Option<String> = self
            .connect()?
            .query_row(
                "SELECT record FROM rag_learning_sessions WHERE kb=? AND active=1",
                [kb],
                |r| r.get(0),
            )
            .optional()?;
        text.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }

    pub fn learning_card(&self, card: &str) -> Result<Value, DbError> {
        let text: String =
            self.connect()?
                .query_row("SELECT record FROM rag_cards WHERE id=?", [card], |r| {
                    r.get(0)
                })?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn learning_state(&self, card: &str) -> Result<LearningState, DbError> {
        read_state(&self.connect()?, card)
    }

    pub fn learning_record(
        &self,
        event: &LearningEvent,
        now: DateTime<Utc>,
    ) -> Result<LearningState, DbError> {
        if event.id.is_empty() || event.id.len() > 120 {
            return Err(invalid("无效的学习事件"));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = read_session(&tx, &event.session_id)?;
        let position = session
            .history
            .iter()
            .position(|p| p.id == event.presentation_id)
            .ok_or_else(|| invalid("卡片不属于本次学习会话"))?;
        let presentation = session.history[position].clone();
        let card_kb: String = tx.query_row(
            "SELECT kb FROM rag_cards WHERE id=?",
            [&presentation.card_id],
            |r| r.get(0),
        )?;
        if card_kb != session.kb {
            return Err(invalid("卡片不属于当前知识库"));
        }
        let request = serde_json::to_string(event)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT request FROM rag_learning_events WHERE id=?",
                [&event.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing != request {
                return Err(invalid("事件编号已被其他操作使用"));
            }
            return read_state(&tx, &presentation.card_id);
        }
        let mut state = read_state(&tx, &presentation.card_id)?;
        let before = state.status.clone();
        if matches!(event.kind.as_str(), "shown" | "revealed" | "skipped") {
            let recorded: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM rag_learning_events WHERE presentation_id=? AND kind=?)", params![presentation.id,event.kind], |r| r.get(0))?;
            if recorded {
                return Ok(state);
            }
        }
        match event.kind.as_str() {
            "shown" => {
                if session.cursor != Some(position)
                    || DateTime::parse_from_rfc3339(&presentation.expires_at)
                        .map_err(|_| invalid("预留时间无效"))?
                        < now
                {
                    return Err(invalid("卡片预留已过期，请重新抽取"));
                }
                session.history[position].confirmed = true;
                state.shown_count += 1;
                state.first_shown_at.get_or_insert(now.to_rfc3339());
                state.last_shown_at = Some(now.to_rfc3339());
            }
            "revealed" | "skipped" | "status" | "undo" => {
                if !presentation.confirmed || session.cursor != Some(position) {
                    return Err(invalid("请先展示当前卡片"));
                }
                match event.kind.as_str() {
                    "revealed" => {
                        session.history[position].revealed = true;
                        state.revealed_count += 1;
                        state.last_revealed_at = Some(now.to_rfc3339());
                        if state.status == "new" {
                            state.status = "learning".into();
                        }
                    }
                    "status" | "undo" => {
                        if state.revision != event.expected_revision {
                            return Err(invalid("学习状态已更新，请刷新后重试"));
                        }
                        if event.kind == "undo" {
                            let previous: Option<(String,u64,String)> = tx.query_row("SELECT before_status,state_revision,kind FROM rag_learning_events WHERE id=? AND card_id=?", params![event.undo_event_id,presentation.card_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
                            let (status, revision, kind) =
                                previous.ok_or_else(|| invalid("找不到可撤销的操作"))?;
                            if revision != state.revision || kind != "status" {
                                return Err(invalid("只能撤销仍然有效的最近一次状态修改"));
                            }
                            state.status = status;
                        } else {
                            let status = event.status.as_deref().unwrap_or("");
                            if !["learning", "review", "mastered"].contains(&status) {
                                return Err(invalid("无效的学习状态"));
                            }
                            state.status = status.into();
                        }
                    }
                    _ => {}
                }
            }
            _ => return Err(invalid("未知的学习事件")),
        }
        state.revision += 1;
        session.revision += 1;
        tx.execute("UPDATE rag_learning_state SET status=?,shown_count=?,revealed_count=?,first_shown_at=?,last_shown_at=?,last_revealed_at=?,updated_at=?,revision=? WHERE card_id=?", params![state.status,state.shown_count,state.revealed_count,state.first_shown_at,state.last_shown_at,state.last_revealed_at,now.to_rfc3339(),state.revision,state.card_id])?;
        tx.execute(
            "INSERT INTO rag_learning_events VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![
                event.id,
                event.session_id,
                state.card_id,
                event.presentation_id,
                event.kind,
                now.to_rfc3339(),
                before,
                state.status,
                state.revision,
                request
            ],
        )?;
        save_session(&tx, &session, now)?;
        tx.commit()?;
        Ok(state)
    }

    pub fn learning_reset(&self, kb: &str) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM rag_learning_sessions WHERE kb=?", [kb])?;
        tx.execute(
            "DELETE FROM rag_learning_state WHERE card_id IN (SELECT id FROM rag_cards WHERE kb=?)",
            [kb],
        )?;
        tx.execute(
            "INSERT INTO rag_learning_state(card_id) SELECT id FROM rag_cards WHERE kb=?",
            [kb],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub fn setup() -> (tempfile::TempDir, Database, DateTime<Utc>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("learn.db"));
        db.initialize().unwrap();
        let now = DateTime::parse_from_rfc3339("2026-09-10T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        for (id, kb) in [("c1", "book"), ("c2", "other")] {
            db.connect()
                .unwrap()
                .execute(
                    "INSERT INTO rag_cards VALUES (?,?,?,?,?)",
                    params![
                        id,
                        kb,
                        id,
                        now.to_rfc3339(),
                        json!({"id":id,"kb":kb,"packet":{"evidence":[]}}).to_string()
                    ],
                )
                .unwrap();
        }
        let session = LearningSession {
            id: "s1".into(),
            kb: "book".into(),
            chapter: None,
            chapter_path: vec![],
            filter: "default".into(),
            relaxed: false,
            history: vec![Presentation {
                id: "p1".into(),
                card_id: "c1".into(),
                confirmed: false,
                revealed: false,
                expires_at: (now + chrono::Duration::minutes(5)).to_rfc3339(),
            }],
            cursor: Some(0),
            revision: 0,
        };
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO rag_learning_sessions VALUES ('s1','book',1,?,?)",
                params![serde_json::to_string(&session).unwrap(), now.to_rfc3339()],
            )
            .unwrap();
        (dir, db, now)
    }

    pub fn event(kind: &str, revision: u64) -> LearningEvent {
        LearningEvent {
            id: format!("e-{kind}-{revision}"),
            session_id: "s1".into(),
            presentation_id: "p1".into(),
            kind: kind.into(),
            status: Some("mastered".into()),
            expected_revision: revision,
            undo_event_id: None,
        }
    }

    #[test]
    fn learning_migration_initializes_legacy_cards_without_inventing_history() {
        let (dir, db, _) = setup();
        let conn = db.connect().unwrap();
        conn.execute_batch("DROP TRIGGER rag_card_learning_state; DROP TABLE rag_learning_events; DROP TABLE rag_learning_sessions; DROP TABLE rag_learning_state; DROP TABLE rag_card_units; DROP TABLE rag_learning_units; DROP TABLE review_memory_events; DROP TABLE review_memory; DROP TABLE review_runs; DROP TABLE study_doubts; DROP TABLE study_highlights; DROP TABLE study_memory; DROP TABLE study_sessions; DELETE FROM schema_migrations WHERE version>=8;").unwrap();
        drop(conn);
        let reopened = Database::new(dir.path().join("learn.db"));
        reopened.initialize().unwrap();
        let state = reopened.learning_state("c1").unwrap();
        assert_eq!(state.status, "new");
        assert_eq!(state.shown_count, 0);
        assert_eq!(state.first_shown_at, None);
        assert_eq!(reopened.rag_list("book", true, 0).unwrap()["total"], 1);
    }

    #[test]
    fn learning_events_are_idempotent_and_restore_reveal_and_status_after_restart() {
        let (dir, db, now) = setup();
        assert!(db.learning_record(&event("revealed", 0), now).is_err());
        let shown = event("shown", 0);
        assert_eq!(db.learning_record(&shown, now).unwrap().status, "new");
        assert_eq!(db.learning_record(&shown, now).unwrap().shown_count, 1);
        assert_eq!(
            db.learning_record(&event("skipped", 1), now)
                .unwrap()
                .status,
            "new"
        );
        let learned = db.learning_record(&event("revealed", 2), now).unwrap();
        assert_eq!(learned.status, "learning");
        assert_eq!(learned.revealed_count, 1);
        let marked = event("status", learned.revision);
        assert_eq!(db.learning_record(&marked, now).unwrap().status, "mastered");
        let reopened = Database::new(dir.path().join("learn.db"));
        reopened.initialize().unwrap();
        assert_eq!(reopened.learning_state("c1").unwrap().status, "mastered");
        assert!(reopened.learning_resume("book").unwrap().unwrap().history[0].revealed);
        let mut undo = event("undo", 4);
        undo.undo_event_id = Some(marked.id);
        assert_eq!(
            reopened.learning_record(&undo, now).unwrap().status,
            "learning"
        );
        assert_eq!(reopened.learning_record(&undo, now).unwrap().revision, 5);
        let mut stale = event("status", 3);
        stale.id = "stale".into();
        assert!(reopened.learning_record(&stale, now).is_err());
    }

    #[test]
    fn learning_failure_rolls_back_events_state_and_session() {
        let (_dir, db, now) = setup();
        db.connect().unwrap().execute_batch("CREATE TRIGGER fail_event BEFORE INSERT ON rag_learning_events BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(db.learning_record(&event("shown", 0), now).is_err());
        assert_eq!(db.learning_state("c1").unwrap().shown_count, 0);
        assert!(!db.learning_resume("book").unwrap().unwrap().history[0].confirmed);
    }

    #[test]
    fn learning_reset_is_scoped_preserves_cards_and_rejects_late_writes() {
        let (_dir, db, now) = setup();
        db.learning_record(&event("shown", 0), now).unwrap();
        db.clear_data("history").unwrap();
        assert_eq!(db.learning_state("c1").unwrap().shown_count, 1);
        db.learning_reset("other").unwrap();
        assert!(db.learning_resume("book").unwrap().is_some());
        db.learning_reset("book").unwrap();
        assert_eq!(db.learning_state("c1").unwrap().status, "new");
        assert!(db.learning_record(&event("revealed", 1), now).is_err());
        assert_eq!(db.rag_list("book", true, 0).unwrap()["total"], 1);
        db.clear_data("all").unwrap();
        assert!(db.learning_state("c1").is_err());
    }

    #[test]
    fn learning_rejects_expired_reservations_and_event_id_reuse() {
        let (_dir, db, now) = setup();
        assert!(db
            .learning_record(&event("shown", 0), now + chrono::Duration::minutes(6))
            .is_err());
        let first = event("shown", 0);
        db.learning_record(&first, now).unwrap();
        let mut reused = event("revealed", 1);
        reused.id = first.id;
        assert!(db.learning_record(&reused, now).is_err());
    }
}
