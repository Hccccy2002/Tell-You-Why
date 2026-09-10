use crate::{
    commands::GenerationLock,
    db::{Database, DbError},
    knowledge_base::Runtime,
    learning::source_keys,
    learning_store::invalid,
    providers::ProviderContext,
    rag, AppState,
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use tauri::State;

fn unit_group(unit: &Value) -> &str {
    unit["sampling_group"]
        .as_str()
        .unwrap_or_else(|| unit["chapter_path"][0].as_str().unwrap_or(""))
}

pub(crate) fn save_unit(conn: &Connection, unit: &Value) -> Result<(), DbError> {
    for key in ["id", "kb", "version", "source_key"] {
        if unit[key].as_str().map_or(true, str::is_empty) {
            return Err(invalid("素材目录缺少身份字段"));
        }
    }
    conn.execute("INSERT INTO rag_learning_units VALUES (?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record",params![unit["id"].as_str(),unit["kb"].as_str(),unit["version"].as_str(),unit["source_key"].as_str(),unit["chapter_path"].to_string(),unit.to_string()])?;
    Ok(())
}

impl Database {
    pub fn learning_generation_candidates(
        &self,
        kb: &str,
        units: &[Value],
        now: DateTime<Utc>,
        seed: u64,
    ) -> Result<Vec<Value>, DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut excluded = HashSet::new();
        let mut chapter_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut stmt = tx.prepare("SELECT record FROM rag_cards WHERE kb=?")?;
        let records = stmt
            .query_map([kb], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for record in records {
            let card: Value = serde_json::from_str(&record)?;
            excluded.extend(source_keys(&card));
        }
        let covered_sources = excluded.clone();
        let mut stmt=tx.prepare("SELECT record,state,created_at FROM rag_tasks WHERE kb=? AND state IN ('prepared','running')")?;
        let tasks = stmt
            .query_map([kb], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (record, state, created) in tasks {
            if state == "running"
                || DateTime::parse_from_rfc3339(&created)
                    .is_ok_and(|t| t > now - Duration::minutes(30))
            {
                let task: Value = serde_json::from_str(&record)?;
                if task["packet"]["learning_unit"].is_object() {
                    excluded.extend(source_keys(&task));
                }
            }
        }
        let mut candidates = vec![];
        for unit in units {
            if unit["kb"] != kb {
                return Err(invalid("素材来自其他知识库"));
            }
            save_unit(&tx, unit)?;
            let keys: HashSet<_> = unit["source_keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            if !keys.is_disjoint(&covered_sources) {
                *chapter_counts.entry(unit_group(unit).into()).or_default() += 1;
            }
            if !keys.is_empty() && keys.is_disjoint(&excluded) {
                candidates.push(unit.clone());
            }
        }
        candidates.sort_by_cached_key(|unit| {
            let group = unit_group(unit);
            (
                chapter_counts.get(group).copied().unwrap_or(0),
                format!(
                    "{:x}",
                    Sha256::digest(format!("{seed}:{}", unit["id"]).as_bytes())
                ),
            )
        });
        // Choose a chapter before a unit; a large chapter must not win by having more blocks.
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for unit in candidates {
            groups
                .entry(unit_group(&unit).into())
                .or_default()
                .push(unit);
        }
        let mut group_keys: Vec<_> = groups.keys().cloned().collect();
        group_keys.sort_by_cached_key(|group| {
            (
                chapter_counts.get(group).copied().unwrap_or(0),
                format!("{:x}", Sha256::digest(format!("{seed}:{group}").as_bytes())),
            )
        });
        let mut selected = vec![];
        while selected.len() < 5 && !group_keys.is_empty() {
            group_keys.retain(|group| {
                let items = groups.get_mut(group).unwrap();
                if !items.is_empty() && selected.len() < 5 {
                    selected.push(items.remove(0));
                }
                !items.is_empty()
            });
        }
        tx.commit()?;
        Ok(selected)
    }

    pub fn rag_recover_interrupted(&self) -> Result<(), DbError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stmt = tx.prepare("SELECT id,record FROM rag_tasks WHERE state='running'")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, record) in rows {
            let mut task: Value = serde_json::from_str(&record)?;
            task["state"] = json!("failed");
            task["error"] =
                json!("应用退出中断了生成，结果及用量可能未知；不会自动重试，请核对任务后重新预览");
            tx.execute(
                "UPDATE rag_tasks SET state='failed',record=? WHERE id=?",
                params![task.to_string(), id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomRequest {
    kb: String,
    version: String,
    chapter: Option<String>,
    provider: String,
    region: String,
}

#[tauri::command]
pub async fn rag_prepare_random(
    request: RandomRequest,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let profile = state
        .database
        .provider_profile(&request.provider, &request.region)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified && p.key_last4.is_some())
        .ok_or("请先配置并测试模型通道")?;
    ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
        .map_err(|e| e.to_string())?;
    let wire = json!({"op":"learning_units","kb":request.kb,"version":request.version,"chapter":request.chapter});
    let catalog = tauri::async_runtime::spawn_blocking(move || Runtime::discover()?.call(wire))
        .await
        .map_err(|e| e.to_string())??;
    let units = catalog["items"].as_array().ok_or("素材目录格式无效")?;
    let candidates = state
        .database
        .learning_generation_candidates(
            &request.kb,
            units,
            Utc::now(),
            uuid::Uuid::new_v4().as_u128() as u64,
        )
        .map_err(|e| e.to_string())?;
    if candidates.is_empty() {
        return Err("所选范围没有未覆盖且可用的素材：可扩大章节范围或学习已有卡片".into());
    }
    let mut last_error = String::new();
    for unit in candidates {
        let wire = json!({"op":"learning_evidence","kb":request.kb,"version":request.version,"chapter":request.chapter,"unit_id":unit["id"]});
        let packet =
            match tauri::async_runtime::spawn_blocking(move || Runtime::discover()?.call(wire))
                .await
                .map_err(|e| e.to_string())?
            {
                Ok(p) => p,
                Err(e) => {
                    last_error = e;
                    continue;
                }
            };
        if packet["evidence"].as_array().map_or(true, Vec::is_empty) {
            continue;
        }
        let task = json!({"id":uuid::Uuid::new_v4().to_string(),"kb":request.kb,"kind":"card","state":"prepared","created_at":Utc::now().to_rfc3339(),"provider":profile.provider_id,"region":profile.region,"model":profile.model,"packet":packet,"result":null,"usage":[],"prompt_version":rag::PROMPT_VERSION,"error":null});
        state
            .database
            .rag_insert(&task)
            .map_err(|e| e.to_string())?;
        return Ok(task);
    }
    Err(format!(
        "已检查最多 5 个本地素材，未能准备完整证据；未调用模型。{last_error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unit(id: &str, kb: &str, key: &str) -> Value {
        json!({"id":id,"kb":kb,"version":"v1","source_key":key,"source_keys":[key],"chapter_path":["第一章"],"block_ids":["b1"]})
    }
    #[test]
    fn learning_generation_reserves_previews_expires_them_and_preserves_new_state() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        let units = vec![unit("u1", "book", "key1"), unit("u2", "book", "key2")];
        assert_eq!(
            db.learning_generation_candidates("book", &units, now, 1)
                .unwrap()
                .len(),
            2
        );
        let mut task = json!({"id":"generated","kb":"book","kind":"card","state":"prepared","created_at":now.to_rfc3339(),"packet":{"learning_unit":units[0]},"result":{"status":"answered","question":"新的教材问题"}});
        db.rag_insert(&task).unwrap();
        assert_eq!(
            db.learning_generation_candidates("book", &units, now, 1)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.learning_generation_candidates("book", &units, now + Duration::minutes(31), 1)
                .unwrap()
                .len(),
            2
        );
        db.connect()
            .unwrap()
            .execute(
                "UPDATE rag_tasks SET state='running' WHERE id='generated'",
                [],
            )
            .unwrap();
        task["state"] = json!("completed");
        db.rag_finish(&mut task).unwrap();
        assert_eq!(db.learning_state("generated").unwrap().status, "new");
        assert_eq!(db.learning_state("generated").unwrap().shown_count, 0);
        assert_eq!(
            db.learning_generation_candidates("book", &units, now + Duration::days(8), 1)
                .unwrap()
                .len(),
            1
        );
        let count: u32 = db
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM rag_card_units", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(db
            .learning_generation_candidates("other", &units, now, 1)
            .is_err());
    }
    #[test]
    fn learning_interrupted_generation_releases_material_without_retry() {
        let (_dir, db, now) = crate::learning_store::tests::setup();
        let units = vec![unit("u1", "book", "key1")];
        db.rag_insert(&json!({"id":"pending","kb":"book","kind":"card","created_at":now.to_rfc3339(),"packet":{"learning_unit":units[0]}})).unwrap();
        db.connect()
            .unwrap()
            .execute(
                "UPDATE rag_tasks SET state='running' WHERE id='pending'",
                [],
            )
            .unwrap();
        assert!(db
            .learning_generation_candidates("book", &units, now, 1)
            .unwrap()
            .is_empty());
        db.rag_recover_interrupted().unwrap();
        assert_eq!(db.rag_task("pending").unwrap()["state"], "failed");
        assert_eq!(
            db.learning_generation_candidates("book", &units, now, 1)
                .unwrap()
                .len(),
            1
        );
    }
}
