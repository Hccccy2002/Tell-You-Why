use crate::db::{Database, DbError};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

impl Database {
    pub fn rag_insert(&self, task: &Value) -> Result<(), DbError> {
        self.connect()?.execute("INSERT INTO rag_tasks(id,kb,kind,state,created_at,record) VALUES (?,?,?,'prepared',?,?)",
            params![task["id"].as_str(),task["kb"].as_str(),task["kind"].as_str(),task["created_at"].as_str(),task.to_string()])?;
        Ok(())
    }

    pub fn rag_task(&self, id: &str) -> Result<Value, DbError> {
        let (record, state): (String, String) = self.connect()?.query_row(
            "SELECT record,state FROM rag_tasks WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut value: Value = serde_json::from_str(&record)?;
        value["state"] = json!(state);
        Ok(value)
    }

    pub fn rag_claim(&self, id: &str, limit: u32) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count:u32=tx.query_row("SELECT COUNT(*) FROM rag_tasks WHERE date(started_at,'localtime')=date('now','localtime')",[],|r|r.get(0))?;
        if count >= limit {
            return Err(DbError::Validation(
                "今日教材生成任务已达设置中的每日上限".into(),
            ));
        }
        if tx.execute("UPDATE rag_tasks SET state='running',started_at=? WHERE id=? AND state='prepared' AND julianday(created_at)>julianday('now','-30 minutes')",
            params![chrono::Utc::now().to_rfc3339(),id])? != 1 {
            return Err(DbError::Validation("任务已执行、正在执行或预览已过期，请重新检索；不会自动重复扣费".into()));
        }
        tx.commit()?;
        Ok(())
    }

    pub fn rag_finish(&self, task: &mut Value) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let running: bool = tx.query_row(
            "SELECT state='running' FROM rag_tasks WHERE id=?",
            [task["id"].as_str()],
            |r| r.get(0),
        )?;
        if !running {
            return Err(DbError::Validation("任务已结束，结果不能重复提交".into()));
        }
        if task["state"] == "completed"
            && task["kind"] == "card"
            && task["result"]["status"] == "answered"
        {
            let text = task["result"]["question"]
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            // An explicit new-card request keeps its model output even if the model
            // reuses a question. The task claim still makes repeated submits idempotent.
            let identity = if task["result"]["generation_mode"] == "llm" {
                format!("{text}:{}", task["id"])
            } else {
                text
            };
            let fingerprint = format!("{:x}", Sha256::digest(identity.as_bytes()));
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM rag_cards WHERE kb=? AND fingerprint=?",
                    params![task["kb"].as_str(), fingerprint],
                    |r| r.get(0),
                )
                .optional()?;
            task["card_id"] = json!(existing.as_deref().unwrap_or(task["id"].as_str().unwrap()));
            task["duplicate_card"] = json!(existing.is_some());
            if existing.is_none() {
                tx.execute(
                    "INSERT INTO rag_cards VALUES (?,?,?,?,?)",
                    params![
                        task["id"].as_str(),
                        task["kb"].as_str(),
                        fingerprint,
                        task["created_at"].as_str(),
                        task.to_string()
                    ],
                )?;
                if task["packet"]["learning_unit"].is_object() {
                    crate::learning_generation::save_unit(&tx, &task["packet"]["learning_unit"])?;
                    tx.execute(
                        "INSERT INTO rag_card_units VALUES (?,?)",
                        params![
                            task["id"].as_str(),
                            task["packet"]["learning_unit"]["id"].as_str()
                        ],
                    )?;
                }
            }
        }
        tx.execute(
            "UPDATE rag_tasks SET state=?,record=? WHERE id=?",
            params![
                task["state"].as_str(),
                task.to_string(),
                task["id"].as_str()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn rag_list(&self, kb: &str, cards: bool, offset: u32) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let table = if cards { "rag_cards" } else { "rag_tasks" };
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE kb=?"),
            [kb],
            |r| r.get(0),
        )?;
        let mut stmt=conn.prepare(&format!("SELECT record FROM {table} WHERE kb=? ORDER BY created_at DESC,id DESC LIMIT 20 OFFSET ?"))?;
        let records = stmt
            .query_map(params![kb, offset], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut items = Vec::new();
        for record in records {
            let value: Value = serde_json::from_str(&record)?;
            items.push(if cards {
                value
            } else {
                self.rag_task(value["id"].as_str().unwrap_or(""))?
            });
        }
        Ok(json!({"total":total,"items":items,"offset":offset}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rag_task_claim_is_atomic_budgeted_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("test.db"));
        db.initialize().unwrap();
        let task = json!({"id":"t1","kb":"a","kind":"ask","created_at":chrono::Utc::now().to_rfc3339(),"state":"prepared","packet":{"version":"v1"}});
        db.rag_insert(&task).unwrap();
        db.rag_claim("t1", 1).unwrap();
        assert!(db.rag_claim("t1", 10).is_err());
        let mut second = task.clone();
        second["id"] = json!("t2");
        db.rag_insert(&second).unwrap();
        assert!(db.rag_claim("t2", 1).is_err());
        let mut finished = db.rag_task("t1").unwrap();
        finished["state"] = json!("failed");
        db.rag_finish(&mut finished).unwrap();
        assert!(db.rag_finish(&mut finished).is_err());
        let reopened = Database::new(dir.path().join("test.db"));
        reopened.initialize().unwrap();
        assert_eq!(reopened.rag_task("t1").unwrap()["state"], "failed");
        assert_eq!(reopened.rag_list("b", false, 0).unwrap()["total"], 0);
    }
    #[test]
    fn rag_cards_keep_evidence_and_dedup_only_within_the_book() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(path.clone());
        db.initialize().unwrap();
        for (id, kb, status) in [
            ("one", "a", "answered"),
            ("two", "a", "answered"),
            ("three", "b", "answered"),
            ("four", "a", "insufficient"),
        ] {
            let mut task = json!({"id":id,"kb":kb,"kind":"card","created_at":chrono::Utc::now().to_rfc3339(),
                "state":"prepared","packet":{"version":"v1","source_sha256":"original","evidence":[{"page":50,"text":"真实原文"}]},
                "result":{"status":status,"question":"什么是总线？"},"usage":[{"call":1}]});
            db.rag_insert(&task).unwrap();
            db.rag_claim(id, 10).unwrap();
            task["state"] = json!("completed");
            db.rag_finish(&mut task).unwrap();
            if id == "two" {
                assert_eq!(task["duplicate_card"], true);
                assert_eq!(task["card_id"], "one");
            }
        }
        let reopened = Database::new(path);
        reopened.initialize().unwrap();
        let cards = reopened.rag_list("a", true, 0).unwrap();
        assert_eq!(cards["total"], 1);
        assert_eq!(cards["items"][0]["packet"]["evidence"][0]["page"], 50);
        assert_eq!(reopened.rag_list("b", true, 0).unwrap()["total"], 1);
        assert_eq!(
            reopened.rag_list("a", true, 20).unwrap()["items"],
            json!([])
        );
        db.clear_data("history").unwrap();
        assert_eq!(db.rag_list("a", true, 0).unwrap()["total"], 1);
        db.clear_data("all").unwrap();
        assert_eq!(db.rag_list("a", true, 0).unwrap()["total"], 0);
    }

    #[test]
    fn llm_cards_keep_each_requested_output_when_questions_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("test.db"));
        db.initialize().unwrap();
        for id in ["one", "two"] {
            let mut task = json!({"id":id,"kb":"book","kind":"card","created_at":chrono::Utc::now().to_rfc3339(),
                "packet":{"evidence":[]}, "result":{"status":"answered","generation_mode":"llm", "question":"相同问题", "answer":[{"text":id,"citations":[]}]}});
            db.rag_insert(&task).unwrap();
            db.rag_claim(id, 10).unwrap();
            task["state"] = json!("completed");
            db.rag_finish(&mut task).unwrap();
            assert_eq!(task["duplicate_card"], false);
            assert_eq!(task["card_id"], id);
            assert!(db.rag_finish(&mut task).is_err());
        }
        assert_eq!(db.rag_list("book", true, 0).unwrap()["total"], 2);
        assert_eq!(
            db.learning_card("one").unwrap()["result"]["answer"][0]["text"],
            "one"
        );
        assert_eq!(
            db.learning_card("two").unwrap()["result"]["answer"][0]["text"],
            "two"
        );
        assert_eq!(db.learning_state("one").unwrap().shown_count, 0);
    }
    #[test]
    fn rag_card_and_task_update_roll_back_together() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("test.db"));
        db.initialize().unwrap();
        let mut task = json!({"id":"t","kb":"a","kind":"card","state":"prepared","created_at":chrono::Utc::now().to_rfc3339(),"result":{"status":"answered","question":"存储器是什么？"}});
        db.rag_insert(&task).unwrap();
        db.rag_claim("t", 10).unwrap();
        db.connect().unwrap().execute_batch("CREATE TRIGGER fail_finish BEFORE UPDATE ON rag_tasks BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        task["state"] = json!("completed");
        assert!(db.rag_finish(&mut task).is_err());
        assert_eq!(db.rag_list("a", true, 0).unwrap()["total"], 0);
        assert_eq!(db.rag_task("t").unwrap()["state"], "running");
    }
}
