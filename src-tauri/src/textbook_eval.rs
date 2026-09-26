//! Opt-in real PDF/model benchmark. Never mutates the user's learning database.
use crate::{
    db::Database,
    providers::{
        ProviderContext, ProviderError, ProviderTransport, RestrictedHttpClient, TransportResponse,
    },
    review_agent::LocalReviewLibrary,
    review_commands::{continue_inner, start_inner, StartReview},
    secret_store::{SecretStore, SecretValue, WindowsCredentialStore},
    AppState,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{Duration, Instant},
};
use url::Url;

fn write(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

struct MeasuredTransport {
    inner: Arc<dyn ProviderTransport>,
    events: Mutex<Vec<Value>>,
    journal: PathBuf,
    limit: usize,
}
#[async_trait]
impl ProviderTransport for MeasuredTransport {
    async fn post_json(
        &self,
        endpoint: &Url,
        key: &SecretValue,
        body: &Value,
        timeout: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        let sequence = {
            let mut events = self.events.lock().unwrap();
            if events.len() >= self.limit {
                return Err(ProviderError::RateLimited);
            }
            let sequence = events.len();
            // Persist before dispatch. An interrupted request is never silently retried.
            events.push(json!({"sequence":sequence + 1,"status":"started","started_at":chrono::Utc::now().to_rfc3339(),"model":body["model"]}));
            write(&self.journal, &json!(*events));
            sequence
        };
        let timer = Instant::now();
        let result = self.inner.post_json(endpoint, key, body, timeout).await;
        let mut events = self.events.lock().unwrap();
        events[sequence]["duration_ms"] = json!(timer.elapsed().as_millis());
        events[sequence]["status"] = json!(if result.is_ok() {
            "received"
        } else {
            "transport_error"
        });
        if let Ok(response) = &result {
            events[sequence]["http_status"] = json!(response.status);
            let payload: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
            // Do not persist raw payloads, prompts, keys or hidden reasoning.
            for name in ["prompt_tokens", "completion_tokens", "total_tokens"] {
                if let Some(n) = payload["usage"][name].as_u64() {
                    events[sequence]["usage"][name] = json!(n);
                }
            }
        }
        write(&self.journal, &json!(*events));
        result
    }
}

fn selected_records<'a>(source: &'a Value, ids: &[&str]) -> Vec<&'a Value> {
    ids.iter()
        .map(|id| {
            source["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == *id && r["mode"] == "hybrid")
                .unwrap_or_else(|| panic!("Missing hybrid packet: {id}"))
        })
        .collect()
}

#[test]
fn selection_never_substitutes_another_case_or_mode() {
    let source = json!({"records":[{"id":"x","mode":"keyword"},{"id":"x","mode":"hybrid"}]});
    assert_eq!(selected_records(&source, &["x"])[0]["mode"], "hybrid");
}

#[test]
fn measurement_enforces_budget_and_never_exports_payload_or_secret() {
    tauri::async_runtime::block_on(async {
        let (dir, _, model, _) =
            crate::review_tests::setup(vec![crate::review_tests::say("private response")]);
        let transport = MeasuredTransport {
            inner: model.clone(),
            events: Mutex::new(vec![]),
            journal: dir.path().join("requests.json"),
            limit: 1,
        };
        let url = Url::parse("https://api.deepseek.com/chat/completions").unwrap();
        let key = SecretValue::for_test("test-only-key");
        let body = json!({"model":"test","messages":[{"content":"private prompt"}]});
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
        assert!(!journal.contains("test-only-key"));
        let events: Value = serde_json::from_str(&journal).unwrap();
        assert_eq!(events[0]["usage"]["total_tokens"], 30);
        assert_eq!(events[0]["status"], "received");
    });
}

#[test]
#[ignore = "opt-in real textbook/API evaluation; requires profile snapshot, retrieval report and output directory"]
fn textbook_eval_live() {
    tauri::async_runtime::block_on(async {
        let input = PathBuf::from(
            std::env::var_os("TELLWHY_TEXTBOOK_INPUT").expect("retrieval report required"),
        );
        let out = PathBuf::from(
            std::env::var_os("TELLWHY_TEXTBOOK_OUTPUT").expect("fresh report directory required"),
        );
        let profile_db = PathBuf::from(
            std::env::var_os("TELLWHY_TEXTBOOK_PROFILE_DB").expect("profile snapshot required"),
        );
        let provider = std::env::var("TELLWHY_TEXTBOOK_PROVIDER").expect("provider required");
        let ids = std::env::var("TELLWHY_TEXTBOOK_CASES")
            .unwrap_or_else(|_| "d01,d05,t10,t13,x01,x04,n01,n03".into());
        let only_agent = std::env::var("TELLWHY_TEXTBOOK_ONLY_AGENT").is_ok();
        let ids: Vec<_> = if only_agent {
            vec![]
        } else {
            ids.split(',').collect()
        };
        assert!((only_agent || !ids.is_empty()) && ids.len() <= 40);
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            ids.len(),
            "Duplicate case IDs"
        );
        let bytes = std::fs::read(&input).unwrap();
        let source: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(source["source_checked"], true);
        let selected = selected_records(&source, &ids);
        let profile = Database::new(profile_db)
            .provider_profiles()
            .unwrap()
            .into_iter()
            .find(|p| p.provider_id == provider && p.connection_verified && p.key_last4.is_some())
            .expect("verified provider required");
        let context =
            ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
                .unwrap();
        let key = WindowsCredentialStore.get(&profile.credential_ref).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let report_path = out.join("runs.json");
        let binding = json!({"input_sha256":format!("{:x}",Sha256::digest(&bytes)),"dataset_sha256":source["dataset_sha256"],
            "provider":profile.provider_id,"region":profile.region,"model":profile.model,"ids":ids,
            "memory_seed":std::env::var("TELLWHY_TEXTBOOK_MEMORY_SEED").ok(),
            "rag_prompt_version":crate::rag::PROMPT_VERSION,"agent_prompt_version":crate::review_agent::REVIEW_PROMPT_VERSION});
        let mut report: Value = if report_path.exists() {
            let saved: Value =
                serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
            assert_eq!(saved["binding"], binding, "Resume inputs changed");
            saved
        } else {
            json!({"schema_version":1,"mode":"live_model_real_pdf","binding":binding,"created_at":chrono::Utc::now().to_rfc3339(),"cases":[],"agent":null})
        };
        let journal = out.join("requests.json");
        let events: Vec<Value> = if journal.exists() {
            serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap()
        } else {
            vec![]
        };
        let transport = Arc::new(MeasuredTransport {
            inner: Arc::new(RestrictedHttpClient::new().unwrap()),
            events: Mutex::new(events),
            journal,
            limit: 40,
        });
        for item in selected {
            if report["cases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == item["id"])
            {
                continue;
            }
            let entry =
                json!({"id":item["id"],"state":"started","packet_sha256":item["packet"]["sha256"]});
            report["cases"].as_array_mut().unwrap().push(entry);
            write(&report_path, &report);
            let first_request = transport.events.lock().unwrap().len();
            let timer = Instant::now();
            let mut usage = vec![];
            let result = crate::rag::generate(
                &item["packet"],
                "ask",
                &provider,
                &context,
                &key,
                transport.as_ref(),
                &mut usage,
            )
            .await;
            let network_failed = transport
                .events
                .lock()
                .unwrap()
                .iter()
                .skip(first_request)
                .any(|e| {
                    e["status"] == "transport_error"
                        || e["http_status"]
                            .as_u64()
                            .is_some_and(|s| !(200..300).contains(&s))
                });
            let entry = report["cases"].as_array_mut().unwrap().last_mut().unwrap();
            entry["duration_ms"] = json!(timer.elapsed().as_millis());
            entry["request_start"] = json!(first_request);
            entry["request_end"] = json!(transport.events.lock().unwrap().len());
            entry["verification"] = usage
                .iter()
                .filter_map(|u| u.get("verification"))
                .cloned()
                .collect::<Vec<_>>()
                .into();
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
            println!("Textbook {}: {}", item["id"], entry["state"]);
            write(&report_path, &report);
            assert!(
                !network_failed,
                "Transport failure persisted; inspect before choosing a new run directory"
            );
        }
        // A genuine agent run uses the local library, not the precomputed gold or retrieval packets.
        if report["agent"].is_null() && std::env::var("TELLWHY_TEXTBOOK_SKIP_AGENT").is_err() {
            report["agent"] = json!({"state":"started"});
            write(&report_path, &report);
            let seed = std::env::var_os("TELLWHY_TEXTBOOK_MEMORY_SEED").map(PathBuf::from);
            if let Some(seed) = &seed {
                assert!(
                    !PathBuf::from(format!("{}-wal", seed.display())).exists(),
                    "Seed must be a closed checkpoint"
                );
                assert!(
                    !out.join("agent.db").exists(),
                    "Do not overwrite a previous agent database"
                );
                std::fs::copy(seed, out.join("agent.db")).unwrap();
            }
            let database = Database::new(out.join("agent.db"));
            database.initialize().unwrap();
            database.save_provider_profile(&profile).unwrap();
            if seed.is_some() {
                // Controlled due-time simulation in this scratch copy; actual answers are retained.
                let count = database
                    .connect()
                    .unwrap()
                    .execute(
                        "UPDATE review_memory SET due_at=?",
                        [chrono::Utc::now().timestamp() - 1],
                    )
                    .unwrap();
                assert!(count > 0, "Seed must contain an actual graded review");
                report["memory_test"] = json!({"due_time_simulated":true,"seed_kind":"previous_real_pdf_agent_run","seed_points":count});
            }
            let state = AppState {
                database,
                secrets: Arc::new(WindowsCredentialStore),
                http: transport.clone(),
                mcp: Arc::new(crate::mcp::McpRuntime::new().unwrap()),
                exiting: AtomicBool::new(false),
                generation_in_progress: AtomicBool::new(false),
                auto_hide: Default::default(),
                persistence_gate: Default::default(),
            };
            let timer = Instant::now();
            let id = start_inner(StartReview {question_count:Some(1),require_sources:false,mcp_server_ids:vec![],due_only:seed.is_some(),kb:"computer-organization".into(), version:source["knowledge_version"].as_str().unwrap().into(),chapter:None,
                goal:"复习Cache写直达与写回的区别。查阅教材后出且仅出一道选择题，答题后记录结果、讲解并结束，不再出题。".into(),provider:profile.provider_id.clone(),region:profile.region.clone()},&state,&LocalReviewLibrary).await.unwrap()["id"].as_str().unwrap().to_string();
            for _ in 0..6 {
                let run = state.database.review_load(&id).unwrap();
                if run.state == "completed" || run.state == "failed" {
                    break;
                }
                if run.state == "waiting_answer" {
                    let q = run
                        .questions
                        .iter()
                        .find(|q| q.selected_index.is_none())
                        .unwrap();
                    state.database.review_answer(&id, &q.id, 0).unwrap();
                }
                continue_inner(id.clone(), &state, &LocalReviewLibrary)
                    .await
                    .unwrap();
            }
            let run = state.database.review_load(&id).unwrap();
            report["agent"] = json!({"state":run.state,"run":run.public(),"trace":run.trace_document(),"duration_ms":timer.elapsed().as_millis(),"simulated_user_selection":0});
            if seed.is_some() {
                let overview = state
                    .database
                    .review_memory_overview(&run.scope, chrono::Utc::now())
                    .unwrap();
                report["memory_test"]["after"] = overview.clone();
                write(&report_path, &report);
                assert_eq!(run.state, "completed");
                assert_eq!(overview["due_count"], 0);
                assert_eq!(overview["total"], report["memory_test"]["seed_points"]);
                assert!(overview["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["attempts"] == 2));
            }
            write(&report_path, &report);
        }
        println!("Report: {}", report_path.display());
    });
}
