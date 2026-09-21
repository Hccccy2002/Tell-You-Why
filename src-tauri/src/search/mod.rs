//! Credential setup and a bounded connection probe, brought forward for P0 validation.
pub(crate) mod agent;
pub(crate) mod core;
pub(crate) mod policy;
pub(crate) mod routing;
mod store;
pub mod types;
pub(crate) mod zhipu;
use crate::{
    commands::{cleanup_pending_credentials, GenerationLock},
    db::{Database, DbError},
    secret_store::last_four,
    AppState,
};
#[cfg(test)]
use crate::{providers::ProviderTransport, secret_store::SecretValue};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::{json, Value};
#[cfg(test)]
use std::time::Duration;
use tauri::State;
#[cfg(test)]
use url::Url;

const ENDPOINT: &str = "https://open.bigmodel.cn/api/paas/v4/web_search";
const REPLACEMENT_REQUIRED: &str = "目标服务通道已有 API Key，请确认覆盖后重试";

#[derive(Clone, Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettings {
    pub key_configured: bool,
    pub key_last4: Option<String>,
    pub connection_verified: bool,
    pub options: types::SearchOptions,
    pub attempts_today: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSearchKey {
    api_key: String,
    #[serde(default)]
    replace_existing_key: bool,
}

impl Database {
    pub(crate) fn search_credential(&self) -> Result<Option<String>, DbError> {
        Ok(self
            .connect()?
            .query_row(
                "SELECT credential_ref FROM search_profiles WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub(crate) fn search_settings(&self) -> Result<SearchSettings, DbError> {
        let mut settings = self
            .connect()?
            .query_row(
                "SELECT key_last4, connection_verified FROM search_profiles WHERE id=1",
                [],
                |row| {
                    Ok(SearchSettings {
                        key_configured: true,
                        key_last4: Some(row.get(0)?),
                        connection_verified: row.get(1)?,
                        ..SearchSettings::default()
                    })
                },
            )
            .optional()?
            .unwrap_or_default();
        settings.options = self.search_options()?;
        settings.attempts_today = self.search_attempts_today()?;
        Ok(settings)
    }

    fn activate_search_key(
        &self,
        credential: &str,
        last4: &str,
        previous: Option<&str>,
    ) -> Result<(), DbError> {
        let mut connection = self.connect()?;
        let tx = connection.transaction()?;
        let current: Option<String> = tx
            .query_row(
                "SELECT credential_ref FROM search_profiles WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if current.as_deref() != previous {
            return Err(DbError::Validation(
                "搜索凭据已发生变化，请重新加载后再试".into(),
            ));
        }
        tx.execute("INSERT INTO search_profiles(id,credential_ref,key_last4,connection_verified,updated_at)
                    VALUES(1,?1,?2,0,?3) ON CONFLICT(id) DO UPDATE SET credential_ref=excluded.credential_ref,
                    key_last4=excluded.key_last4,connection_verified=0,updated_at=excluded.updated_at",
            params![credential,last4,chrono::Utc::now().to_rfc3339()])?;
        tx.execute(
            "DELETE FROM pending_credential_deletions WHERE credential_ref=?1",
            [credential],
        )?;
        if let Some(previous) = previous {
            tx.execute(
                "INSERT OR IGNORE INTO pending_credential_deletions VALUES(?1,?2)",
                params![previous, chrono::Utc::now().to_rfc3339()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn detach_search_key(&self) -> Result<(), DbError> {
        let mut connection = self.connect()?;
        let tx = connection.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO pending_credential_deletions(credential_ref,created_at)
                    SELECT credential_ref,?1 FROM search_profiles",
            [chrono::Utc::now().to_rfc3339()],
        )?;
        tx.execute("DELETE FROM search_profiles", [])?;
        tx.execute(
            "UPDATE search_options SET record=json_set(record,'$.mode','off')",
            [],
        )?;
        tx.execute("DELETE FROM search_cache", [])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn set_search_verified(
        &self,
        credential: &str,
        verified: bool,
    ) -> Result<(), DbError> {
        self.connect()?.execute(
            "UPDATE search_profiles SET connection_verified=?1 WHERE credential_ref=?2",
            params![verified, credential],
        )?;
        Ok(())
    }
}

#[tauri::command]
pub fn search_settings(state: State<'_, AppState>) -> Result<SearchSettings, String> {
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state.database.search_settings().map_err(|e| e.to_string())
}

fn save_key(input: SaveSearchKey, state: &AppState) -> Result<SearchSettings, String> {
    let previous = state
        .database
        .search_credential()
        .map_err(|e| e.to_string())?;
    if previous.is_some() && !input.replace_existing_key {
        return Err(REPLACEMENT_REQUIRED.into());
    }
    let key = input.api_key.trim();
    if key.is_empty() {
        return Err("请输入完整智谱 API Key".into());
    }
    let credential = format!("zhipu-search:cn:{}", uuid::Uuid::new_v4().simple());
    // Queue before writing the secret: a crash cannot orphan an untracked credential.
    state
        .database
        .queue_credential_deletion(&credential)
        .map_err(|e| e.to_string())?;
    let result = state
        .secrets
        .save(&credential, key)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            state
                .database
                .activate_search_key(&credential, &last_four(key), previous.as_deref())
                .map_err(|e| e.to_string())
        });
    let cleanup = cleanup_pending_credentials(state);
    result?;
    // The new credential is already active. Failed old-key cleanup stays queued.
    let _ = cleanup;
    state.database.search_settings().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_search_key(
    input: SaveSearchKey,
    state: State<'_, AppState>,
) -> Result<SearchSettings, String> {
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    save_key(input, &state)
}

#[tauri::command]
pub fn delete_search_key(state: State<'_, AppState>) -> Result<(), String> {
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    state
        .database
        .detach_search_key()
        .map_err(|e| e.to_string())?;
    cleanup_pending_credentials(&state)
}

#[cfg(test)]
fn validate_probe(status: u16, body: &str) -> Result<(), String> {
    let results = zhipu::parse_response(
        status,
        body,
        &types::SearchRequest {
            query: "Node.js 官方发布记录".into(),
            engine: "search_pro".into(),
            recency: "noLimit".into(),
            domain: None,
        },
    )
    .map_err(|error| error.to_string())?;
    if results.is_empty() {
        return Err("连接有响应，但未返回搜索结果，请稍后重新测试".into());
    }
    Ok(())
}

#[cfg(test)]
async fn probe(transport: &dyn ProviderTransport, secret: &SecretValue) -> Result<(), String> {
    let body = json!({"search_query":"Node.js 官方发布记录","search_engine":"search_pro",
        "search_intent":false,"count":5,"content_size":"medium","search_recency_filter":"noLimit",
        "request_id":uuid::Uuid::new_v4().to_string()});
    let response = transport
        .post_json(
            &Url::parse(ENDPOINT).unwrap(),
            secret,
            &body,
            Duration::from_secs(20),
        )
        .await
        .map_err(|e| match e {
            crate::providers::ProviderError::Timeout => "智谱搜索请求超时，请稍后重试".to_owned(),
            _ => "无法连接智谱搜索，请检查网络后重试".to_owned(),
        })?;
    validate_probe(response.status, &response.body)
}

#[tauri::command]
pub async fn test_search_connection(state: State<'_, AppState>) -> Result<SearchSettings, String> {
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let credential = state
        .database
        .search_credential()
        .map_err(|e| e.to_string())?
        .ok_or("请先保存智谱 API Key")?;
    let secret = state.secrets.get(&credential).map_err(|e| e.to_string())?;
    let options = state.database.search_options().map_err(|e| e.to_string())?;
    state
        .database
        .reserve_search_attempt(
            &uuid::Uuid::new_v4().to_string(),
            options.daily_attempt_limit,
        )
        .map_err(|e| e.to_string())?;
    use zhipu::SearchProvider;
    let result = zhipu::ZhipuSearch
        .search(
            &types::SearchRequest {
                query: "Node.js 官方发布记录".into(),
                engine: options.engine,
                recency: "noLimit".into(),
                domain: None,
            },
            &secret,
            state.http.as_ref(),
        )
        .await
        .map_err(|e| e.to_string())
        .and_then(|items| {
            if items.is_empty() {
                Err("未返回可用结果，请稍后重试".into())
            } else {
                Ok(())
            }
        });
    state
        .database
        .set_search_verified(&credential, result.is_ok())
        .map_err(|e| e.to_string())?;
    result?;
    state.database.search_settings().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_search_options(
    options: types::SearchOptions,
    state: State<'_, AppState>,
) -> Result<SearchSettings, String> {
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    options.validate().map_err(|e| e.to_string())?;
    let previous = state.database.search_options().map_err(|e| e.to_string())?;
    if options.mode != types::SearchMode::Off
        && state
            .database
            .search_credential()
            .map_err(|e| e.to_string())?
            .is_none()
    {
        return Err("请先保存智谱 API Key".into());
    }
    state
        .database
        .save_search_options(&options)
        .map_err(|e| e.to_string())?;
    if previous.engine != options.engine {
        if let Some(credential) = state
            .database
            .search_credential()
            .map_err(|e| e.to_string())?
        {
            state
                .database
                .set_search_verified(&credential, false)
                .map_err(|e| e.to_string())?;
        }
    }
    state.database.search_settings().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
