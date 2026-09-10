use crate::db::{Database, DbError};
use crate::learning_store::{invalid, read_session, save_session, LearningSession, Presentation};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const RECENT_DAYS: i64 = 7;
const RECENT_COUNT: usize = 20;
const RESERVATION_MINUTES: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartLearning {
    pub kb: String,
    pub chapter: Option<String>,
    pub chapter_path: Vec<String>,
    pub filter: String,
    pub relaxed: bool,
}

#[derive(Debug, Serialize)]
pub struct Selection {
    pub session: LearningSession,
    pub reason: Option<String>,
    pub scoped: usize,
    pub eligible: usize,
}

pub(crate) fn source_keys(card: &Value) -> HashSet<String> {
    if let Some(keys) = card["packet"]["learning_unit"]["source_keys"].as_array() {
        return keys
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    let packet = &card["packet"];
    let source = packet["source_sha256"]
        .as_str()
        .unwrap_or_else(|| packet["version"].as_str().unwrap_or("unknown"));
    packet["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let block = e["block_id"].as_str()?;
            let text = e["text"]
                .as_str()?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let page = e["page"].as_u64()?;
            Some(format!(
                "{source}:{block}:{page}:{:x}",
                Sha256::digest(text.as_bytes())
            ))
        })
        .collect()
}

fn paths(card: &Value) -> Vec<Vec<String>> {
    if let Some(path) = card["packet"]["learning_unit"]["chapter_path"].as_array() {
        return vec![path
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()];
    }
    card["packet"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["chapter_path"].as_array())
        .map(|p| {
            p.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .collect()
}

pub(crate) fn in_scope(card: &Value, scope: &[String]) -> bool {
    if scope.is_empty() {
        return true;
    }
    let paths = paths(card);
    !paths.is_empty() && paths.iter().all(|p| p.starts_with(scope))
}

fn chapter_group(card: &Value, scope: &[String]) -> String {
    paths(card)
        .first()
        .and_then(|p| p.get(scope.len()))
        .cloned()
        .unwrap_or_else(|| "本章正文".into())
}

fn random_index(seed: &mut u64, length: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed as usize) % length
}

impl Database {
    pub fn learning_start(
        &self,
        request: StartLearning,
        now: DateTime<Utc>,
    ) -> Result<LearningSession, DbError> {
        if request.kb.is_empty()
            || request.kb.len() > 120
            || request.chapter_path.len() > 20
            || request
                .chapter_path
                .iter()
                .any(|p| p.is_empty() || p.len() > 1000)
            || !["default", "review", "all"].contains(&request.filter.as_str())
            || (request.chapter.is_some() && request.chapter_path.is_empty())
        {
            return Err(invalid("学习范围或筛选条件无效"));
        }
        let session = LearningSession {
            id: uuid::Uuid::new_v4().to_string(),
            kb: request.kb,
            chapter: request.chapter,
            chapter_path: request.chapter_path,
            filter: request.filter,
            relaxed: request.relaxed,
            history: vec![],
            cursor: None,
            revision: 0,
        };
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE rag_learning_sessions SET active=0 WHERE kb=? AND active=1",
            [&session.kb],
        )?;
        tx.execute(
            "INSERT INTO rag_learning_sessions VALUES (?,?,1,?,?)",
            params![
                session.id,
                session.kb,
                serde_json::to_string(&session)?,
                now.to_rfc3339()
            ],
        )?;
        tx.commit()?;
        Ok(session)
    }

    pub fn learning_next(
        &self,
        id: &str,
        revision: u64,
        now: DateTime<Utc>,
        mut seed: u64,
    ) -> Result<Selection, DbError> {
        if seed == 0 {
            seed = 1;
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = read_session(&tx, id)?;
        if session.revision != revision {
            return Err(invalid("学习进度已更新，请恢复当前会话"));
        }
        if let Some(index) = session.cursor {
            let current = &session.history[index];
            if !current.confirmed
                && DateTime::parse_from_rfc3339(&current.expires_at)
                    .is_ok_and(|expiry| expiry > now)
            {
                return Ok(Selection {
                    session,
                    reason: None,
                    scoped: 0,
                    eligible: 1,
                });
            }
            if index + 1 < session.history.len() {
                session.cursor = Some(index + 1);
                session.revision += 1;
                save_session(&tx, &session, now)?;
                tx.commit()?;
                return Ok(Selection {
                    session,
                    reason: None,
                    scoped: 0,
                    eligible: 1,
                });
            }
        }
        session.history.retain(|p| p.confirmed);
        session.cursor = session.history.len().checked_sub(1);
        let mut stmt=tx.prepare("SELECT c.record,s.status FROM rag_cards c JOIN rag_learning_state s ON s.card_id=c.id WHERE c.kb=? ORDER BY c.id")?;
        let rows = stmt
            .query_map([&session.kb], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        let cards = rows
            .into_iter()
            .map(|(record, status)| Ok((serde_json::from_str::<Value>(&record)?, status)))
            .collect::<Result<Vec<_>, DbError>>()?;
        let mut recent=tx.prepare("SELECT e.card_id,e.occurred_at FROM rag_learning_events e JOIN rag_learning_sessions s ON s.id=e.session_id WHERE s.kb=? AND e.kind='shown' ORDER BY julianday(e.occurred_at) DESC,e.rowid DESC")?;
        let seen = recent
            .query_map([&session.kb], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(recent);
        let mut excluded: HashSet<String> =
            session.history.iter().map(|p| p.card_id.clone()).collect();
        if !session.relaxed {
            for (i, (card, at)) in seen.iter().enumerate() {
                if i < RECENT_COUNT
                    || DateTime::parse_from_rfc3339(at)
                        .is_ok_and(|t| t >= now - Duration::days(RECENT_DAYS))
                {
                    excluded.insert(card.clone());
                }
            }
        }
        let excluded_sources: HashSet<String> = cards
            .iter()
            .filter(|(c, _)| excluded.contains(c["id"].as_str().unwrap_or("")))
            .flat_map(|(c, _)| source_keys(c))
            .collect();
        let scoped = cards
            .iter()
            .filter(|(c, _)| in_scope(c, &session.chapter_path))
            .count();
        let mut candidates: Vec<_> = cards
            .iter()
            .filter(|(card, status)| {
                in_scope(card, &session.chapter_path)
                    && match session.filter.as_str() {
                        "review" => status == "review",
                        "all" => true,
                        _ => status != "mastered",
                    }
            })
            .filter(|(card, _)| {
                !excluded.contains(card["id"].as_str().unwrap_or(""))
                    && source_keys(card).is_disjoint(&excluded_sources)
            })
            .collect();
        if session.filter == "default" && candidates.iter().any(|(_, s)| s == "new") {
            candidates.retain(|(_, s)| s == "new");
        }
        let eligible = candidates.len();
        if eligible == 0 {
            save_session(&tx, &session, now)?;
            tx.commit()?;
            return Ok(Selection {
                session,
                reason: Some(
                    if scoped == 0 {
                        "所选范围暂无已存学习卡"
                    } else {
                        "当前筛选下没有可抽取卡片：候选已在本轮展示、近期看过或被状态筛选排除"
                    }
                    .into(),
                ),
                scoped,
                eligible: 0,
            });
        }
        let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for (card, _) in candidates {
            groups
                .entry(chapter_group(card, &session.chapter_path))
                .or_default()
                .push(card);
        }
        let mut counts: BTreeMap<String, usize> = groups.keys().map(|k| (k.clone(), 0)).collect();
        for p in &session.history {
            if let Some((c, _)) = cards.iter().find(|(c, _)| c["id"] == p.card_id) {
                if let Some(n) = counts.get_mut(&chapter_group(c, &session.chapter_path)) {
                    *n += 1;
                }
            }
        }
        let least = counts.values().min().copied().unwrap_or(0);
        let keys: Vec<_> = counts
            .iter()
            .filter(|(_, n)| **n == least)
            .map(|(k, _)| k)
            .collect();
        let group = &groups[keys[random_index(&mut seed, keys.len())]];
        let card = group[random_index(&mut seed, group.len())];
        session.history.push(Presentation {
            id: uuid::Uuid::new_v4().to_string(),
            card_id: card["id"]
                .as_str()
                .ok_or_else(|| invalid("卡片缺少编号"))?
                .into(),
            confirmed: false,
            revealed: false,
            expires_at: (now + Duration::minutes(RESERVATION_MINUTES)).to_rfc3339(),
        });
        session.cursor = Some(session.history.len() - 1);
        session.revision += 1;
        save_session(&tx, &session, now)?;
        tx.commit()?;
        Ok(Selection {
            session,
            reason: None,
            scoped,
            eligible,
        })
    }

    pub fn learning_previous(
        &self,
        id: &str,
        revision: u64,
        now: DateTime<Utc>,
    ) -> Result<LearningSession, DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = read_session(&tx, id)?;
        if session.revision != revision {
            return Err(invalid("学习进度已更新，请重新加载"));
        }
        if let Some(cursor) = session.cursor {
            session.cursor = Some(cursor.saturating_sub(1));
        }
        session.revision += 1;
        save_session(&tx, &session, now)?;
        tx.commit()?;
        Ok(session)
    }

    pub fn learning_summary(&self, kb: &str, now: DateTime<Utc>) -> Result<Value, DbError> {
        let conn = self.connect()?;
        let mut result =
            json!({"new":0,"learning":0,"review":0,"mastered":0,"today":0,"covered_units":0});
        let mut stmt=conn.prepare("SELECT s.status,COUNT(*) FROM rag_learning_state s JOIN rag_cards c ON c.id=s.card_id WHERE c.kb=? GROUP BY s.status")?;
        for row in stmt.query_map([kb], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))? {
            let (key, count) = row?;
            result[key] = json!(count);
        }
        let today:u64=conn.query_row("SELECT COUNT(DISTINCT e.card_id) FROM rag_learning_events e JOIN rag_learning_sessions s ON s.id=e.session_id WHERE s.kb=? AND e.kind IN ('revealed','status') AND date(e.occurred_at,'localtime')=date(?,'localtime')",params![kb,now.to_rfc3339()],|r|r.get(0))?;
        let covered:u64=conn.query_row("SELECT COUNT(DISTINCT m.unit_id) FROM rag_card_units m JOIN rag_learning_state s ON s.card_id=m.card_id JOIN rag_cards c ON c.id=m.card_id WHERE c.kb=? AND s.status!='new'",[kb],|r|r.get(0))?;
        result["today"] = json!(today);
        result["covered_units"] = json!(covered);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning_store::LearningEvent;

    fn populate(db: &Database, now: DateTime<Utc>, count: usize) {
        for i in 0..count {
            let id = format!("card-{i:03}");
            let chapter = if i % 3 == 0 {
                "A"
            } else if i % 3 == 1 {
                "B"
            } else {
                "C"
            };
            let card = json!({"id":id,"kb":"book","packet":{"source_sha256":"source","version":"v1","evidence":[{"block_id":format!("block-{i}"),"text":format!("正文{i}"),"page":i+1,"chapter_path":[chapter]}]}});
            db.connect()
                .unwrap()
                .execute(
                    "INSERT INTO rag_cards VALUES (?,?,?,?,?)",
                    params![id, "book", id, now.to_rfc3339(), card.to_string()],
                )
                .unwrap();
        }
    }
    fn start(db: &Database, now: DateTime<Utc>, relaxed: bool) -> LearningSession {
        db.learning_start(
            StartLearning {
                kb: "book".into(),
                chapter: None,
                chapter_path: vec![],
                filter: "default".into(),
                relaxed,
            },
            now,
        )
        .unwrap()
    }
    fn draw(db: &Database, s: &LearningSession, now: DateTime<Utc>, seed: u64) -> LearningSession {
        let selected = db.learning_next(&s.id, s.revision, now, seed).unwrap();
        assert!(selected.reason.is_none());
        let p = selected.session.history.last().unwrap();
        db.learning_record(
            &LearningEvent {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: s.id.clone(),
                presentation_id: p.id.clone(),
                kind: "shown".into(),
                status: None,
                expected_revision: 0,
                undo_event_id: None,
            },
            now,
        )
        .unwrap();
        db.learning_resume("book").unwrap().unwrap()
    }
    #[test]
    fn learning_random_balances_chapters_and_never_repeats_in_round() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        db.connect()
            .unwrap()
            .execute("DELETE FROM rag_cards", [])
            .unwrap();
        populate(&db, now, 30);
        let mut session = start(&db, now, false);
        let mut counts = BTreeMap::new();
        let mut ids = HashSet::new();
        for seed in 1..=15 {
            session = draw(&db, &session, now, seed);
            let id = &session.history.last().unwrap().card_id;
            assert!(ids.insert(id.clone()));
            let card = db.learning_card(id).unwrap();
            *counts.entry(chapter_group(&card, &[])).or_insert(0) += 1;
        }
        assert!(counts.values().all(|n| *n == 5));
    }
    #[test]
    fn learning_recent_windows_are_union_and_persist_between_sessions() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        db.connect()
            .unwrap()
            .execute("DELETE FROM rag_cards", [])
            .unwrap();
        populate(&db, now, 24);
        let mut session = start(&db, now, false);
        for seed in 1..=24 {
            session = draw(&db, &session, now, seed);
        }
        let fresh = start(&db, now, false);
        assert_eq!(db.learning_next(&fresh.id, 0, now, 5).unwrap().eligible, 0);
        let boundary = now + Duration::days(7);
        let boundary_session = start(&db, boundary, false);
        assert_eq!(
            db.learning_next(&boundary_session.id, 0, boundary, 3)
                .unwrap()
                .eligible,
            0
        );
        let later = now + Duration::days(8);
        let fresh = start(&db, later, false);
        assert_eq!(
            db.learning_next(&fresh.id, 0, later, 3).unwrap().eligible,
            4
        );
        let review = start(&db, later, true);
        assert!(db.learning_next(&review.id, 0, later, 3).unwrap().eligible > 0);
    }
    #[test]
    fn learning_scope_mastery_priority_and_duplicate_sources_are_filtered() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        db.connect()
            .unwrap()
            .execute("DELETE FROM rag_cards", [])
            .unwrap();
        populate(&db, now, 3);
        db.connect()
            .unwrap()
            .execute(
                "UPDATE rag_learning_state SET status='mastered' WHERE card_id='card-001'",
                [],
            )
            .unwrap();
        let session = db
            .learning_start(
                StartLearning {
                    kb: "book".into(),
                    chapter: Some("a".into()),
                    chapter_path: vec!["A".into()],
                    filter: "default".into(),
                    relaxed: false,
                },
                now,
            )
            .unwrap();
        let session = draw(&db, &session, now, 99);
        assert_eq!(session.history[0].card_id, "card-000");
        let mut duplicate = db.learning_card("card-000").unwrap();
        duplicate["id"] = json!("duplicate");
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO rag_cards VALUES ('duplicate','book','other-question',?,?)",
                params![now.to_rfc3339(), duplicate.to_string()],
            )
            .unwrap();
        let all = start(&db, now, false);
        let selected = db.learning_next(&all.id, 0, now, 1).unwrap();
        assert_eq!(selected.session.history[0].card_id, "card-002");
        assert_eq!(db.learning_summary("other", now).unwrap()["today"], 0);
    }
    #[test]
    fn learning_reservation_reuse_expiry_and_stale_next_do_not_skip_cards() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        populate(&db, now, 3);
        let s = start(&db, now, false);
        let a = db.learning_next(&s.id, 0, now, 1).unwrap().session;
        assert!(db.learning_next(&s.id, 0, now, 2).is_err());
        let b = db.learning_next(&a.id, a.revision, now, 2).unwrap().session;
        assert_eq!(a.history[0].id, b.history[0].id);
        let c = db
            .learning_next(&a.id, a.revision, now + Duration::minutes(6), 2)
            .unwrap()
            .session;
        assert_eq!(c.history.len(), 1);
        assert_ne!(a.history[0].id, c.history[0].id);
    }
    #[test]
    fn learning_previous_and_resume_do_not_add_shown_events() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        populate(&db, now, 4);
        let a = start(&db, now, false);
        let b = draw(&db, &a, now, 1);
        let c = draw(&db, &b, now, 2);
        let old = db.learning_previous(&c.id, c.revision, now).unwrap();
        assert_eq!(old.cursor, Some(0));
        let next = db
            .learning_next(&old.id, old.revision, now, 3)
            .unwrap()
            .session;
        assert_eq!(next.cursor, Some(1));
        assert_eq!(next.history.len(), 2);
        assert_eq!(
            db.learning_state(&next.history[1].card_id)
                .unwrap()
                .shown_count,
            1
        );
    }
}
