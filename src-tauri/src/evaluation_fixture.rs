//! Synthetic responses for offline orchestration evaluation. No real HTTP client or key.
use crate::{
    db::{Database, ProviderProfileRecord},
    providers::{ProviderError, ProviderTransport, TransportResponse},
    secret_store::{SecretError, SecretStore, SecretValue},
    AppState,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

struct SimulatedKeys;
impl SecretStore for SimulatedKeys {
    fn get(&self, _: &str) -> Result<SecretValue, SecretError> {
        Ok(SecretValue::simulated())
    }
    fn save(&self, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }
    fn delete(&self, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }
}
struct SimulatedModel(Mutex<VecDeque<Value>>);
#[async_trait]
impl ProviderTransport for SimulatedModel {
    async fn post_json(
        &self,
        _: &url::Url,
        _: &SecretValue,
        _: &Value,
        _: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        let value = self
            .0
            .lock()
            .map_err(|_| ProviderError::Unavailable)?
            .pop_front()
            .ok_or(ProviderError::Unavailable)?;
        if value == json!("NETWORK_ERROR") {
            return Err(ProviderError::Unavailable);
        }
        Ok(TransportResponse { status: 200, body: json!({"choices":[{"finish_reason":if value["tool_calls"].is_array(){"tool_calls"}else{"stop"},"message":value}],"usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}}).to_string() })
    }
}
pub(crate) fn tools(calls: Vec<(&str, &str, Value)>) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":calls.into_iter().map(|(id,name,args)|json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}})).collect::<Vec<_>>()})
}
pub(crate) fn say(text: &str) -> Value {
    json!({"role":"assistant","content":text})
}
pub(crate) fn setup(path: &Path, replies: Vec<Value>) -> Result<AppState, String> {
    let database = Database::new(path.to_path_buf());
    database.initialize().map_err(|e| e.to_string())?;
    database
        .save_provider_profile(&ProviderProfileRecord {
            provider_id: "deepseek".into(),
            region: "default".into(),
            model: "deepseek-v4-flash".into(),
            credential_ref: "simulation".into(),
            key_last4: Some("mock".into()),
            connection_verified: true,
        })
        .map_err(|e| e.to_string())?;
    let card = json!({"id":"existing","kb":"book","packet":{"version":"v1","evidence":[{"chapter_path":["存储器"]}]},"result":{"question":"存储器的作用"}});
    database
        .connect()
        .map_err(|e| e.to_string())?
        .execute(
            "INSERT INTO rag_cards VALUES ('existing','book','fingerprint',?,?)",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), card.to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(AppState {
        database,
        secrets: Arc::new(SimulatedKeys),
        http: Arc::new(SimulatedModel(Mutex::new(replies.into()))),
        mcp: Arc::new(crate::mcp::McpRuntime::new().unwrap()),
        exiting: AtomicBool::new(false),
        generation_in_progress: AtomicBool::new(false),
        auto_hide: Default::default(),
        persistence_gate: Default::default(),
    })
}
