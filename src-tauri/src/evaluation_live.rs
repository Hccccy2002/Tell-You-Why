//! Real textbook evaluations reuse production generation and review tools with a shared request budget.
use crate::{
    db::{Database, ProviderProfileRecord},
    evaluation::{write_json, Control},
    providers::{ProviderContext, ProviderError, ProviderTransport, TransportResponse},
    review_agent::LocalReviewLibrary,
    review_commands::{continue_inner, start_inner, StartReview},
    secret_store::{SecretStore, SecretValue},
    AppState,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

struct MeasuredTransport {
    inner: Arc<dyn ProviderTransport>,
    events: Mutex<Vec<Value>>,
    journal: PathBuf,
    cancel: Arc<AtomicBool>,
    halted: AtomicBool,
    limit: usize,
}
#[async_trait]
impl ProviderTransport for MeasuredTransport {
    async fn post_json(
        &self,
        endpoint: &url::Url,
        key: &SecretValue,
        body: &Value,
        timeout: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        if self.cancel.load(Ordering::SeqCst) || self.halted.load(Ordering::SeqCst) {
            return Err(ProviderError::Unavailable);
        }
        let sequence = {
            let mut events = self.events.lock().map_err(|_| ProviderError::Unavailable)?;
            if events.len() >= self.limit {
                self.halted.store(true, Ordering::SeqCst);
                return Err(ProviderError::RateLimited);
            }
            let sequence = events.len();
            events.push(json!({"sequence":sequence+1,"status":"started","started_at":chrono::Utc::now().to_rfc3339(),"model":body["model"]}));
            write_json(&self.journal, &*events).map_err(|_| ProviderError::Unavailable)?;
            sequence
        };
        let timer = Instant::now();
        let result = self.inner.post_json(endpoint, key, body, timeout).await;
        let mut events = self.events.lock().map_err(|_| ProviderError::Unavailable)?;
        events[sequence]["duration_ms"] = json!(timer.elapsed().as_millis());
        events[sequence]["status"] = json!(if result.is_ok() {
            "received"
        } else {
            "transport_error"
        });
        if let Ok(response) = &result {
            events[sequence]["http_status"] = json!(response.status);
            let payload: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
            for name in ["prompt_tokens", "completion_tokens", "total_tokens"] {
                if let Some(n) = payload["usage"][name].as_u64() {
                    events[sequence]["usage"][name] = json!(n);
                }
            }
        }
        if result
            .as_ref()
            .map_or(true, |r| !(200..300).contains(&r.status))
        {
            self.halted.store(true, Ordering::SeqCst);
        }
        write_json(&self.journal, &*events).map_err(|_| ProviderError::Unavailable)?;
        result
    }
}

pub(crate) async fn run(
    control: &Control,
    profile: ProviderProfileRecord,
    http: Arc<dyn ProviderTransport>,
    secrets: Arc<dyn SecretStore>,
) -> Result<(), String> {
    control.check()?;
    let bytes = std::fs::read(control.dir.join("retrieval.json")).map_err(|e| e.to_string())?;
    let source: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if source["source_checked"] != true {
        return Err("教材和索引尚未通过校验".into());
    }
    let ids = ["d01", "d05", "t10", "t13", "x01", "x04", "n01", "n03"];
    let records = source["records"].as_array().ok_or("缺少教材检索结果")?;
    let selected: Vec<_> = ids
        .iter()
        .map(|id| {
            records
                .iter()
                .find(|r| r["id"] == *id && r["mode"] == "hybrid")
                .ok_or("教材题目不完整")
        })
        .collect::<Result<_, _>>()?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|e| e.to_string())?;
    let key = secrets
        .get(&profile.credential_ref)
        .map_err(|e| e.to_string())?;
    let transport = Arc::new(MeasuredTransport {
        inner: http,
        events: Mutex::new(vec![]),
        journal: control.dir.join("requests.json"),
        cancel: control.cancel.clone(),
        halted: AtomicBool::new(false),
        limit: 40,
    });
    write_json(&transport.journal, &json!([]))?;
    let mut report = json!({"schema_version":1,"mode":"live_model_real_pdf","created_at":chrono::Utc::now().to_rfc3339(),
        "binding":{"input_sha256":format!("{:x}",Sha256::digest(&bytes)),"dataset_sha256":source["dataset_sha256"],"provider":profile.provider_id,"region":profile.region,"model":profile.model,"ids":ids,"rag_prompt_version":crate::rag::PROMPT_VERSION,"agent_prompt_version":crate::review_agent::REVIEW_PROMPT_VERSION},"cases":[],"agent":null});
    let path = control.dir.join("runs.json");
    write_json(&path, &report)?;
    for (position, item) in selected.iter().enumerate() {
        control.progress(
            position,
            9,
            &format!(
                "正在生成第 {} / 8 题（{}）",
                position + 1,
                item["id"].as_str().unwrap_or("")
            ),
        )?;
        report["cases"].as_array_mut().unwrap().push(
            json!({"id":item["id"],"state":"started","packet_sha256":item["packet"]["sha256"]}),
        );
        write_json(&path, &report)?;
        let timer = Instant::now();
        let mut usage = vec![];
        let first = transport.events.lock().map_err(|e| e.to_string())?.len();
        let result = crate::rag::generate(
            &item["packet"],
            "ask",
            &profile.provider_id,
            &context,
            &key,
            transport.as_ref(),
            &mut usage,
        )
        .await;
        let entry = report["cases"].as_array_mut().unwrap().last_mut().unwrap();
        entry["duration_ms"] = json!(timer.elapsed().as_millis());
        entry["request_start"] = json!(first);
        entry["request_end"] = json!(transport.events.lock().map_err(|e| e.to_string())?.len());
        entry["verification"] = json!(usage
            .iter()
            .filter_map(|u| u.get("verification"))
            .collect::<Vec<_>>());
        match result {
            Ok(draft) => {
                entry["state"] = json!("completed");
                entry["draft"] = json!(draft);
            }
            Err(error) => {
                entry["state"] = json!("failed");
                entry["error"] = json!(error);
            }
        }
        write_json(&path, &report)?;
        control.progress(position + 1, 9, "已保存本题结果")?;
        if transport.halted.load(Ordering::SeqCst) {
            return Err("模型请求失败或达到 40 次上限，本轮已停止；已完成结果已保留。".into());
        }
    }
    control.progress(8, 9, "正在执行真实教材复习 Agent…")?;
    report["agent"] = json!({"state":"started"});
    write_json(&path, &report)?;
    let database = Database::new(control.dir.join("agent.db"));
    database.initialize().map_err(|e| e.to_string())?;
    database
        .save_provider_profile(&profile)
        .map_err(|e| e.to_string())?;
    let state = AppState {
        database,
        secrets,
        http: transport.clone(),
        exiting: AtomicBool::new(false),
        generation_in_progress: AtomicBool::new(false),
        auto_hide: Default::default(),
        persistence_gate: Default::default(),
    };
    let timer = Instant::now();
    let id = start_inner(StartReview { question_count: Some(1), require_sources: false, due_only: false, kb: "computer-organization".into(), version: source["knowledge_version"].as_str().ok_or("缺少教材版本")?.into(), chapter: None,
        goal: "复习Cache写直达与写回的区别。查阅教材后出且仅出一道选择题，答题后记录结果、讲解并结束，不再出题。".into(), provider: profile.provider_id, region: profile.region }, &state, &LocalReviewLibrary).await?["id"].as_str().ok_or("缺少复习编号")?.to_string();
    for _ in 0..6 {
        if control.check().is_err() {
            break;
        }
        let run = state.database.review_load(&id).map_err(|e| e.to_string())?;
        if ["completed", "failed"].contains(&run.state.as_str()) {
            break;
        }
        if run.state == "waiting_answer" {
            let question = run
                .questions
                .iter()
                .find(|q| q.selected_index.is_none())
                .ok_or("缺少待答题目")?;
            state
                .database
                .review_answer(&id, &question.id, 0)
                .map_err(|e| e.to_string())?;
        }
        let result = continue_inner(id.clone(), &state, &LocalReviewLibrary).await;
        let run = state.database.review_load(&id).map_err(|e| e.to_string())?;
        report["agent"] = json!({"state":run.state,"run":run.public(),"trace":run.trace_document(),"duration_ms":timer.elapsed().as_millis(),"simulated_user_selection":0});
        write_json(&path, &report)?;
        result?;
    }
    control.check()?;
    control.progress(9, 9, "正在汇总评测报告…")?;
    if report["agent"]["state"] != "completed" {
        return Err("真实教材复习 Agent 未完成，可展开报告查看执行记录。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::read_json;
    #[test]
    fn live_transport_limits_requests_honors_stop_and_redacts_journal() {
        tauri::async_runtime::block_on(async {
            let (dir, _, model, _) =
                crate::review_tests::setup(vec![crate::review_tests::say("private reply")]);
            let transport = MeasuredTransport {
                inner: model.clone(),
                events: Mutex::new(vec![]),
                journal: dir.path().join("requests.json"),
                cancel: Arc::new(AtomicBool::new(true)),
                halted: AtomicBool::new(false),
                limit: 1,
            };
            let url = url::Url::parse("https://api.deepseek.com/chat/completions").unwrap();
            let key = SecretValue::for_test("private-key");
            let body = json!({"model":"test","messages":[{"content":"private prompt"}]});
            assert!(transport
                .post_json(&url, &key, &body, Duration::from_secs(1))
                .await
                .is_err());
            assert!(model.requests.lock().unwrap().is_empty());
            transport.cancel.store(false, Ordering::SeqCst);
            transport
                .post_json(&url, &key, &body, Duration::from_secs(1))
                .await
                .unwrap();
            assert!(transport
                .post_json(&url, &key, &body, Duration::from_secs(1))
                .await
                .is_err());
            assert_eq!(model.requests.lock().unwrap().len(), 1);
            let journal = std::fs::read_to_string(&transport.journal).unwrap();
            assert!(!journal.contains("private"));
            assert_eq!(
                read_json(&transport.journal).unwrap()[0]["usage"]["total_tokens"],
                30
            );
        });
    }
}
