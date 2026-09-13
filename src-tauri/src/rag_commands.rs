use crate::{
    commands::GenerationLock, knowledge_base::Runtime, providers::ProviderContext, rag, AppState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    kb: String,
    version: String,
    chapter: Option<String>,
    query: String,
    kind: String,
    provider: String,
    region: String,
}

fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("无效的资料或任务标识".into());
    }
    Ok(())
}

#[tauri::command]
pub fn rag_providers(state: State<'_, AppState>) -> Result<Value, String> {
    let profiles = state
        .database
        .provider_profiles()
        .map_err(|e| e.to_string())?;
    Ok(json!(profiles
        .into_iter()
        .filter(|p| p.connection_verified && p.key_last4.is_some())
        .map(|p| json!({"id":p.provider_id,"region":p.region,"model":p.model}))
        .collect::<Vec<_>>()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedSourcesRequest {
    kb: String,
    version: String,
    chapter: Option<String>,
    query: String,
}

#[tauri::command]
pub async fn rag_related_sources(
    request: RelatedSourcesRequest,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    identifier(&request.kb)?;
    identifier(&request.version)?;
    if let Some(chapter) = &request.chapter {
        identifier(chapter)?;
    }
    if request.query.trim().is_empty() || request.query.chars().count() > 1000 {
        return Err("请输入 1–1000 字的问题".into());
    }
    // Only one CPU reranker runs at a time, including requests from multiple windows.
    let wire = json!({"op":"related_sources","kb":request.kb,"version":request.version,
        "chapter":request.chapter,"query":request.query.trim()});
    tauri::async_runtime::spawn_blocking(move || {
        static RERANKER: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _slot = RERANKER.lock().map_err(|e| e.to_string())?;
        Runtime::discover()?.call(wire)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn rag_prepare(
    request: PrepareRequest,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    identifier(&request.kb)?;
    identifier(&request.version)?;
    if request
        .chapter
        .as_deref()
        .is_some_and(|c| identifier(c).is_err())
    {
        return Err("无效章节".into());
    }
    if !["ask", "card"].contains(&request.kind.as_str())
        || request.query.trim().is_empty()
        || request.query.chars().count() > 1000
    {
        return Err("请输入 1–1000 字的问题或学习主题".into());
    }
    let profile = state
        .database
        .provider_profile(&request.provider, &request.region)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified && p.key_last4.is_some())
        .ok_or("请先在设置中配置模型并通过连接测试")?;
    ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
        .map_err(|e| e.to_string())?;
    let query = request.query.trim().to_string();
    let wire = json!({"op":"evidence","kb":request.kb,"version":request.version,"chapter":request.chapter,"query":query});
    let packet = tauri::async_runtime::spawn_blocking(move || Runtime::discover()?.call(wire))
        .await
        .map_err(|e| e.to_string())??;
    let task = json!({"id":uuid::Uuid::new_v4().to_string(),"kb":request.kb,"kind":request.kind,
        "state":"prepared","created_at":chrono::Utc::now().to_rfc3339(),"provider":profile.provider_id,
        "region":profile.region,"model":profile.model,"packet":packet,"result":null,"usage":[],
        "prompt_version":rag::PROMPT_VERSION,"error":null});
    state
        .database
        .rag_insert(&task)
        .map_err(|e| e.to_string())?;
    Ok(task)
}

#[tauri::command]
pub async fn rag_generate(task_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    generate_inner(task_id, &state).await
}

pub(crate) async fn generate_inner(task_id: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    identifier(&task_id)?;
    let mut task = state
        .database
        .rag_task(&task_id)
        .map_err(|e| e.to_string())?;
    if task["state"] == "completed" {
        return Ok(task);
    }
    if task["prompt_version"] != rag::PROMPT_VERSION {
        return Err("教材生成规则已更新，请重新检索并预览摘录".into());
    }
    if task["packet"]["learning_unit"].is_object() || task["packet"]["status"] == "no_evidence" {
        let wire =
            json!({"op":"learning_version","kb":task["kb"],"version":task["packet"]["version"]});
        tauri::async_runtime::spawn_blocking(move || Runtime::discover()?.call(wire))
            .await
            .map_err(|e| e.to_string())??;
    }
    let provider = task["provider"]
        .as_str()
        .ok_or("任务缺少模型通道")?
        .to_string();
    let profile = state
        .database
        .provider_profile(&provider, task["region"].as_str().ok_or("缺少服务区域")?)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified)
        .ok_or("模型通道未就绪，请重新配置并检索")?;
    if task["region"] != profile.region || task["model"] != profile.model {
        return Err("模型设置已改变，请重新预览发送范围".into());
    }
    let context = ProviderContext::from_registry(&provider, &profile.region, &profile.model)
        .map_err(|e| e.to_string())?;
    let key = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|e| e.to_string())?;
    let limit = state
        .database
        .settings()
        .map_err(|e| e.to_string())?
        .daily_generation_limit;
    state
        .database
        .rag_claim(&task_id, limit)
        .map_err(|e| e.to_string())?;
    let mut usage = vec![];
    let outcome = rag::generate(
        &task["packet"],
        task["kind"].as_str().unwrap_or("ask"),
        &provider,
        &context,
        &key,
        state.http.as_ref(),
        &mut usage,
    )
    .await;
    task["usage"] = json!(usage);
    task["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
    match outcome {
        Ok(result) => {
            task["state"] = json!("completed");
            task["result"] = json!(result);
        }
        Err(error) => {
            task["state"] = json!("failed");
            task["error"] = json!(error);
        }
    }
    state
        .database
        .rag_finish(&mut task)
        .map_err(|e| format!("结果保存失败：{e}；请查看任务记录，不要重复发送"))?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{Database, ProviderProfileRecord},
        providers::{ProviderError, ProviderTransport, TransportResponse},
        secret_store::{SecretError, SecretStore, SecretValue},
    };
    use async_trait::async_trait;
    use std::sync::{atomic::AtomicBool, Arc, Mutex};
    use std::time::Duration;
    struct Keys;
    impl SecretStore for Keys {
        fn save(&self, _: &str, _: &str) -> Result<(), SecretError> {
            unreachable!()
        }
        fn delete(&self, _: &str) -> Result<(), SecretError> {
            unreachable!()
        }
        fn get(&self, _: &str) -> Result<SecretValue, SecretError> {
            Ok(SecretValue::for_test("dummy"))
        }
    }
    struct Http {
        calls: Mutex<usize>,
        fail: bool,
        fallback: bool,
    }
    #[async_trait]
    impl ProviderTransport for Http {
        async fn post_json(
            &self,
            _: &url::Url,
            _: &SecretValue,
            _: &Value,
            _: Duration,
        ) -> Result<TransportResponse, ProviderError> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if self.fail {
                return Err(ProviderError::RateLimited);
            }
            let value = if self.fallback && *calls == 1 {
                json!({"status":"insufficient","reason":"原文不足"})
            } else if self.fallback {
                json!("这是模型提供的通俗解释，可作为学习内容。")
            } else if *calls == 1 {
                json!({"status":"answered","question":"存储器有什么作用？","answer":[{"text":"存储器存放程序和数据。","citations":[{"evidence_id":"E1"}]}],"explanation":[{"text":"程序和数据都存放在存储器中。","citations":[{"evidence_id":"E1"}]}],"reason":""})
            } else {
                json!({"checks":[{"reason":"引用直接说明用途。","supported":true},{"reason":"解释忠实重述引用。","supported":true}],"question_check":{"reason":"回答存储器用途。","answers_question":true}})
            };
            let content = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            Ok(TransportResponse{status:200,body:json!({"choices":[{"finish_reason":"stop","message":{"content":content}}],"usage":{"total_tokens":50}}).to_string()})
        }
    }
    fn setup(fail: bool) -> (tempfile::TempDir, AppState, Arc<Http>) {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::new(dir.path().join("test.db"));
        database.initialize().unwrap();
        database
            .save_provider_profile(&ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: "dummy".into(),
                key_last4: Some("test".into()),
                connection_verified: true,
            })
            .unwrap();
        database.rag_insert(&json!({"id":"task","kb":"book","kind":"card","provider":"deepseek","region":"default","model":"deepseek-v4-flash","state":"prepared","prompt_version":rag::PROMPT_VERSION,"created_at":chrono::Utc::now().to_rfc3339(),"packet":{"query":"存储器有什么作用？","version":"v1","evidence":[{"id":"E1","text":"存储器存放程序和数据。","page":1}]}})).unwrap();
        let http = Arc::new(Http {
            calls: Mutex::new(0),
            fail,
            fallback: false,
        });
        let state = AppState {
            database,
            secrets: Arc::new(Keys),
            http: http.clone(),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };
        (dir, state, http)
    }
    #[test]
    fn rag_command_saves_model_card_and_completed_request_is_idempotent() {
        tauri::async_runtime::block_on(async {
            let (_dir, state, http) = setup(false);
            let result = generate_inner("task".into(), &state).await.unwrap();
            assert_eq!(result["state"], "completed");
            assert_eq!(result["usage"].as_array().unwrap().len(), 1);
            assert_eq!(result["result"]["generation_mode"], "llm");
            assert_eq!(
                state.database.rag_list("book", true, 0).unwrap()["total"],
                1
            );
            assert_eq!(generate_inner("task".into(), &state).await.unwrap(), result);
            assert_eq!(*http.calls.lock().unwrap(), 1);
        });
    }
    #[test]
    fn rag_command_persists_prose_fallback_as_a_normal_learning_card() {
        tauri::async_runtime::block_on(async {
            let (_dir, mut state, _) = setup(false);
            let http = Arc::new(Http {
                calls: Mutex::new(0),
                fail: false,
                fallback: true,
            });
            state.http = http.clone();
            let result = generate_inner("task".into(), &state).await.unwrap();
            assert_eq!(result["state"], "completed");
            assert_eq!(
                result["result"]["answer"][0]["text"],
                "这是模型提供的通俗解释，可作为学习内容。"
            );
            assert_eq!(
                state.database.learning_card("task").unwrap()["result"],
                result["result"]
            );
            assert_eq!(state.database.learning_state("task").unwrap().status, "new");
            assert_eq!(generate_inner("task".into(), &state).await.unwrap(), result);
            assert_eq!(*http.calls.lock().unwrap(), 2);
        });
    }
    #[test]
    fn rag_command_failure_is_recorded_without_retry_or_card() {
        tauri::async_runtime::block_on(async {
            let (_dir, state, http) = setup(true);
            let result = generate_inner("task".into(), &state).await.unwrap();
            assert_eq!(result["state"], "failed");
            assert!(generate_inner("task".into(), &state).await.is_err());
            assert_eq!(*http.calls.lock().unwrap(), 1);
            assert_eq!(
                state.database.rag_list("book", true, 0).unwrap()["total"],
                0
            );
        });
    }
    #[test]
    fn rag_command_old_prompt_requires_new_preview_without_spending_budget() {
        tauri::async_runtime::block_on(async {
            let (_dir, state, http) = setup(false);
            let mut old = state.database.rag_task("task").unwrap();
            old["id"] = json!("old-task");
            old["prompt_version"] = json!("textbook-rag-v1");
            state.database.rag_insert(&old).unwrap();
            assert!(generate_inner("old-task".into(), &state)
                .await
                .unwrap_err()
                .contains("规则已更新"));
            assert_eq!(*http.calls.lock().unwrap(), 0);
            assert_eq!(
                state.database.rag_task("old-task").unwrap()["state"],
                "prepared"
            );
            assert!(state.database.rag_claim("task", 1).is_ok());
        });
    }
    #[test]
    fn rag_command_changed_provider_and_busy_gate_block_transmission() {
        tauri::async_runtime::block_on(async {
            let (_dir, state, http) = setup(false);
            state
                .generation_in_progress
                .store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(generate_inner("task".into(), &state).await.is_err());
            state
                .generation_in_progress
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let mut profile = state
                .database
                .provider_profile("deepseek", "default")
                .unwrap()
                .unwrap();
            profile.model = "deepseek-v4-pro".into();
            state.database.save_provider_profile(&profile).unwrap();
            assert!(generate_inner("task".into(), &state).await.is_err());
            assert_eq!(*http.calls.lock().unwrap(), 0);
        });
    }
}

#[tauri::command]
pub fn rag_list(
    kb: String,
    cards: bool,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    identifier(&kb)?;
    state
        .database
        .rag_list(&kb, cards, offset)
        .map_err(|e| e.to_string())
}
