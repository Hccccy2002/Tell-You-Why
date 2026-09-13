use crate::{
    db::{Database, ProviderProfileRecord},
    providers::{ProviderError, ProviderTransport, TransportResponse},
    review_agent::{execute_tool, ReviewLibrary, ReviewRun, ToolCall},
    review_commands::{continue_inner, start_inner, StartReview},
    secret_store::{SecretError, SecretStore, SecretValue},
    AppState,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

pub struct Keys;
impl SecretStore for Keys {
    fn get(&self, _: &str) -> Result<SecretValue, SecretError> {
        Ok(SecretValue::for_test("test-only-key"))
    }
    fn save(&self, _: &str, _: &str) -> Result<(), SecretError> {
        unreachable!()
    }
    fn delete(&self, _: &str) -> Result<(), SecretError> {
        unreachable!()
    }
}
pub struct Library {
    pub requests: Mutex<Vec<Value>>,
}
#[async_trait]
impl ReviewLibrary for Library {
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.requests.lock().unwrap().push(request.clone());
        if request["op"] == "learning_version" {
            return Ok(json!({"version":"v1"}));
        }
        if request["op"] == "learning_units" {
            return Ok(
                json!({"kb":"book","version":"v1","filename":"测试教材.pdf","chapter_path":["存储器"],"items":[]}),
            );
        }
        if request["query"] == "工具故障" {
            return Err("本地检索暂时不可用".into());
        }
        let evidence = if request["query"] == "存储器" {
            json!([{"id":"E1","block_id":"b1","page":3,"chapter_path":["存储器"],"text":"存储器用于存放程序和数据。"}])
        } else {
            json!([])
        };
        Ok(json!({"kb":"book","version":"v1","evidence":evidence}))
    }
}
pub struct Model {
    pub replies: Mutex<VecDeque<Value>>,
    pub requests: Mutex<Vec<Value>>,
}
#[async_trait]
impl ProviderTransport for Model {
    async fn post_json(
        &self,
        _: &url::Url,
        _: &SecretValue,
        body: &Value,
        _: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        self.requests.lock().unwrap().push(body.clone());
        let value = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted model exhausted");
        if value == json!("NETWORK_ERROR") {
            return Err(ProviderError::Unavailable);
        }
        Ok(TransportResponse{status:200,body:json!({"choices":[{"finish_reason":if value["tool_calls"].is_array(){"tool_calls"}else{"stop"},"message":value}],"usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}}).to_string()})
    }
}
pub fn tools(calls: Vec<(&str, &str, Value)>) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":calls.into_iter().map(|(id,name,args)|json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}})).collect::<Vec<_>>()})
}
pub fn say(text: &str) -> Value {
    json!({"role":"assistant","content":text})
}
pub fn quiz(sources: Value) -> Value {
    json!({"topic":"存储器","question":"存储器用于存放什么？","options":["程序和数据","只有图片"],"correct_index":0,"explanation":"原文说明存储器存放程序和数据。","source_ids":sources,"card_id":"existing"})
}
pub fn setup(replies: Vec<Value>) -> (tempfile::TempDir, AppState, Arc<Model>, Library) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("test.db"));
    db.initialize().unwrap();
    db.save_provider_profile(&ProviderProfileRecord {
        provider_id: "deepseek".into(),
        region: "default".into(),
        model: "deepseek-v4-flash".into(),
        credential_ref: "test".into(),
        key_last4: Some("test".into()),
        connection_verified: true,
    })
    .unwrap();
    let card = json!({"id":"existing","kb":"book","packet":{"version":"v1","evidence":[{"chapter_path":["存储器"]}]},"result":{"question":"存储器的作用"}});
    db.connect()
        .unwrap()
        .execute(
            "INSERT INTO rag_cards VALUES ('existing','book','fingerprint',?,?)",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), card.to_string()],
        )
        .unwrap();
    let model = Arc::new(Model {
        replies: Mutex::new(replies.into()),
        requests: Mutex::new(vec![]),
    });
    let state = AppState {
        database: db,
        secrets: Arc::new(Keys),
        http: model.clone(),
        exiting: AtomicBool::new(false),
        generation_in_progress: AtomicBool::new(false),
        auto_hide: Default::default(),
        persistence_gate: Default::default(),
    };
    (
        dir,
        state,
        model,
        Library {
            requests: Mutex::new(vec![]),
        },
    )
}
pub async fn start(state: &AppState, library: &Library) -> String {
    start_inner(
        StartReview {
            due_only: false,
            kb: "book".into(),
            version: "v1".into(),
            chapter: Some("chapter1".into()),
            goal: "结合薄弱点带我复习存储器".into(),
            provider: "deepseek".into(),
            region: "default".into(),
        },
        state,
        library,
    )
    .await
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn review_agent_retrieves_adapts_reads_quizzes_and_records_actual_answer() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![
            tools(vec![
                ("t1", "get_learning_progress", json!({})),
                (
                    "t2",
                    "search_textbook",
                    json!({"query":"没有命中","mode":"keyword"}),
                ),
            ]),
            tools(vec![(
                "t3",
                "search_textbook",
                json!({"query":"存储器","mode":"hybrid"}),
            )]),
            tools(vec![("t4", "read_source", json!({"source_id":"S1"}))]),
            tools(vec![("t5", "save_review_question", quiz(json!(["S1"])))]),
            say("先想一想，再选择答案。"),
            tools(vec![(
                "t6",
                "record_quiz_result",
                json!({"question_id":"q1"}),
            )]),
            say("这次选择还需巩固，存储器存放程序和数据。"),
        ]);
        let id = start(&state, &library).await;
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "waiting_answer");
        assert_eq!(result["questions"][0]["correct_index"], Value::Null);
        assert_eq!(result["questions"][0]["explanation"], Value::Null);
        assert_eq!(result["sources"][0]["text"], "存储器用于存放程序和数据。");
        assert_eq!(
            state.database.learning_state("existing").unwrap().status,
            "new"
        );
        let before = model.requests.lock().unwrap().len();
        continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(model.requests.lock().unwrap().len(), before);
        state.database.review_answer(&id, "q1", 1).unwrap();
        state.database.review_answer(&id, "q1", 1).unwrap();
        assert!(state.database.review_answer(&id, "q1", 0).is_err());
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "completed");
        assert_eq!(result["questions"][0]["correct"], false);
        let learning = state.database.learning_state("existing").unwrap();
        assert_eq!(learning.status, "review");
        assert_eq!(learning.revision, 1);
        assert_eq!(
            continue_inner(id.clone(), &state, &library).await.unwrap(),
            result
        );
        assert_eq!(state.database.review_load(&id).unwrap().model_calls, 7);
        let saved = state.database.review_load(&id).unwrap();
        let trace = saved.trace_document();
        assert_eq!(trace["reported_total_tokens"], 210);
        assert_eq!(trace["usage_reported_requests"], 7);
        assert_eq!(saved.trace.iter().filter(|e| e.kind == "model").count(), 7);
        assert_eq!(saved.trace.iter().filter(|e| e.kind == "tool").count(), 6);
        assert_eq!(saved.trace.iter().filter(|e| e.kind == "user").count(), 1);
        assert!(saved
            .trace
            .iter()
            .all(|e| e.status == "succeeded" && e.duration_ms.is_some()));
        assert!(saved
            .trace
            .iter()
            .enumerate()
            .all(|(index, e)| e.sequence == index + 1));
        let requests = model.requests.lock().unwrap();
        assert_eq!(requests[0]["tools"].as_array().unwrap().len(), 5);
        assert!(requests[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("matches")));
        assert!(!requests[0].to_string().contains("test-only-key"));
    });
}

#[test]
fn review_tools_reject_fabricated_answers_and_sources_without_writes() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        let call = |name: &str, value: Value| ToolCall {
            id: "tool".into(),
            name: name.into(),
            arguments: value.to_string(),
        };
        assert!(execute_tool(
            &state.database,
            &library,
            &mut run,
            &call("save_review_question", quiz(json!(["fake"])))
        )
        .await
        .is_err());
        assert!(run.questions.is_empty());
        execute_tool(
            &state.database,
            &library,
            &mut run,
            &call("save_review_question", quiz(json!([]))),
        )
        .await
        .unwrap();
        assert!(execute_tool(
            &state.database,
            &library,
            &mut run,
            &call("record_quiz_result", json!({"question_id":"q1"}))
        )
        .await
        .is_err());
        assert!(execute_tool(
            &state.database,
            &library,
            &mut run,
            &call(
                "record_quiz_result",
                json!({"question_id":"q1","selected":0})
            )
        )
        .await
        .is_err());
        assert!(execute_tool(
            &state.database,
            &library,
            &mut run,
            &call(
                "search_textbook",
                json!({"query":"x","mode":"keyword","kb":"other"})
            )
        )
        .await
        .is_err());
        assert!(execute_tool(
            &state.database,
            &library,
            &mut run,
            &call("read_source", json!({"source_id":"S999"}))
        )
        .await
        .is_err());
        assert_eq!(
            state.database.learning_state("existing").unwrap().revision,
            0
        );
    });
}

#[test]
fn review_checkpoint_survives_failure_and_resume_does_not_save_question_twice() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![
            tools(vec![("save1", "save_review_question", quiz(json!([])))]),
            json!("NETWORK_ERROR"),
            say("继续回答已保存的题目。"),
        ]);
        let id = start(&state, &library).await;
        assert_eq!(
            continue_inner(id.clone(), &state, &library).await.unwrap()["state"],
            "failed"
        );
        assert_eq!(state.database.review_load(&id).unwrap().questions.len(), 1);
        let failed = state.database.review_load(&id).unwrap();
        assert!(failed
            .trace
            .iter()
            .any(|e| e.kind == "model" && e.status == "failed"));
        assert_eq!(failed.trace_document()["usage_reported_requests"], 1);
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "waiting_answer");
        assert_eq!(result["questions"].as_array().unwrap().len(), 1);
        assert_eq!(state.database.review_load(&id).unwrap().tool_calls, 1);
        let resumed = state.database.review_load(&id).unwrap();
        assert!(resumed.trace.iter().any(|e| e.name == "resumed"));
        assert_eq!(
            resumed
                .trace
                .iter()
                .filter(|e| e.name == "save_review_question")
                .count(),
            1
        );
        state.database.clear_data("history").unwrap();
        assert!(state.database.review_latest("book").unwrap().is_some());
        state.database.clear_data("all").unwrap();
        assert!(state.database.review_latest("book").unwrap().is_none());
    });
}

#[test]
fn review_cancel_and_budget_stop_before_another_model_or_tool_call() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        state.database.review_cancel(&id).unwrap();
        let context = crate::providers::ProviderContext::from_registry(
            "deepseek",
            "default",
            "deepseek-v4-flash",
        )
        .unwrap();
        crate::review_agent::drive(
            &state.database,
            &library,
            model.as_ref(),
            &context,
            &SecretValue::for_test("test"),
            &mut run,
        )
        .await
        .unwrap();
        assert_eq!(state.database.review_load(&id).unwrap().state, "paused");
        let mut run = state.database.review_claim(&id).unwrap();
        run.model_calls = crate::review_agent::MAX_MODEL_CALLS;
        crate::review_agent::drive(
            &state.database,
            &library,
            model.as_ref(),
            &context,
            &SecretValue::for_test("test"),
            &mut run,
        )
        .await
        .unwrap();
        assert!(model.requests.lock().unwrap().is_empty());
    });
}

#[test]
fn review_recovery_replays_pending_tools_and_returns_tool_errors_to_model() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![
            tools(vec![("fixed", "save_review_question", quiz(json!([])))]),
            say("请作答。"),
        ]);
        let id = start(&state, &library).await;
        let mut run: ReviewRun = state.database.review_claim(&id).unwrap();
        run.pending.push(ToolCall {
            id: "broken".into(),
            name: "search_textbook".into(),
            arguments: "{broken".into(),
        });
        run.messages
            .push(tools(vec![("broken", "search_textbook", json!({}))]));
        run.trace_begin("tool", "search_textbook", json!({"call_id":"broken"}));
        state.database.review_save(&run, None).unwrap();
        state.database.review_recover().unwrap();
        assert_eq!(state.database.review_load(&id).unwrap().state, "paused");
        assert!(state
            .database
            .review_load(&id)
            .unwrap()
            .trace
            .iter()
            .any(|e| e.name == "search_textbook" && e.status == "interrupted"));
        let result = continue_inner(id, &state, &library).await.unwrap();
        assert_eq!(result["state"], "waiting_answer");
        assert!(model.requests.lock().unwrap()[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool"
                && m["content"].as_str().unwrap().contains("工具参数格式错误")));
    });
}

#[test]
fn review_trace_excludes_private_messages_answers_and_raw_tool_payloads() {
    tauri::async_runtime::block_on(async {
        let mut reply = tools(vec![("save", "save_review_question", quiz(json!([])))]);
        reply["reasoning_content"] = json!("private-model-reasoning");
        let (_dir, state, _, library) = setup(vec![reply, say("请回答。")]);
        let id = start(&state, &library).await;
        continue_inner(id.clone(), &state, &library).await.unwrap();
        let saved = state.database.review_load(&id).unwrap();
        assert!(serde_json::to_string(&saved.messages)
            .unwrap()
            .contains("private-model-reasoning"));
        let trace = saved.trace_document().to_string();
        for hidden in [
            "private-model-reasoning",
            "test-only-key",
            "correct_index",
            "explanation",
            "存储器用于存放什么",
            "只有图片",
        ] {
            assert!(!trace.contains(hidden), "trace leaked {hidden}");
        }
        assert!(trace.contains("save_review_question"));
        assert_eq!(saved.trace_document()["reported_total_tokens"], 60);
        assert_eq!(state.database.review_history("book").unwrap().len(), 1);
        assert!(state
            .database
            .review_history("other-book")
            .unwrap()
            .is_empty());
        let mut legacy = serde_json::to_value(&saved).unwrap();
        legacy.as_object_mut().unwrap().remove("trace");
        let legacy: ReviewRun = serde_json::from_value(legacy).unwrap();
        assert!(legacy.trace.is_empty());
        assert_eq!(
            legacy.trace_document()["reported_total_tokens"],
            Value::Null
        );
    });
}
