use crate::{
    db::Database,
    providers::{ProviderError, ProviderTransport, TransportResponse},
    search::types::{SearchMode, SearchOptions},
    secret_store::SecretValue,
    study::{StudySession, StudyStep},
    study_commands::continue_inner,
    study_store::QuestionOptions,
    AppState,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Http {
    replies: Mutex<VecDeque<TransportResponse>>,
    requests: Mutex<Vec<(String, Value)>>,
    pause_at: Mutex<Option<(usize, std::path::PathBuf, String, bool)>>,
}
#[async_trait::async_trait]
impl ProviderTransport for Http {
    async fn post_json(
        &self,
        url: &url::Url,
        _: &SecretValue,
        body: &Value,
        _: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        let count = {
            let mut requests = self.requests.lock().unwrap();
            requests.push((url.to_string(), body.clone()));
            requests.len()
        };
        let action = self.pause_at.lock().unwrap().clone();
        if let Some((at, path, id, finish)) = action.filter(|a| a.0 == count) {
            let _ = at;
            Database::new(path).study_pause(&id, finish).unwrap();
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        Ok(self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected paid request"))
    }
}
fn response(body: Value) -> TransportResponse {
    TransportResponse {
        status: 200,
        body: body.to_string(),
    }
}
fn model(value: Value) -> TransportResponse {
    response(
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":value.to_string()}}],"usage":{"total_tokens":40}}),
    )
}
fn plan() -> TransportResponse {
    model(json!({"needsSearch":true,"query":"公开科技资料","recency":"noLimit"}))
}
fn web() -> TransportResponse {
    response(
        json!({"search_result":[{"title":"公开参考资料","link":"https://example.org/reference","content":"可引用的公开正文"},{"title":"另一条资料","link":"","content":"没有链接的补充正文"}]}),
    )
}
fn answer(text: &str, id: &str) -> TransportResponse {
    model(
        json!({"status":"answered","blocks":[{"text":text,"evidenceIds":[id]}],"limitation":null}),
    )
}
fn setup(
    replies: Vec<TransportResponse>,
    question: &str,
    mode: SearchMode,
) -> (tempfile::TempDir, AppState, Arc<Http>, String) {
    let (dir, mut state, _, _) = crate::review_tests::setup(vec![]);
    let http = Arc::new(Http {
        replies: Mutex::new(replies.into()),
        requests: Default::default(),
        pause_at: Default::default(),
    });
    state.http = http.clone();
    state
        .database
        .connect()
        .unwrap()
        .execute(
            "INSERT INTO search_profiles VALUES (1,'test','test',1,?)",
            [chrono::Utc::now().to_rfc3339()],
        )
        .unwrap();
    state
        .database
        .save_search_options(&SearchOptions {
            mode,
            ..Default::default()
        })
        .unwrap();
    let context = crate::providers::ProviderContext::from_registry(
        "deepseek",
        "default",
        "deepseek-v4-flash",
    )
    .unwrap();
    let mut run = StudySession::new(
        "学一点科技".into(),
        "科技".into(),
        "deepseek",
        "default",
        &context,
    );
    run.steps.push(StudyStep {
        id: "step1".into(),
        kind: "concept".into(),
        title: "科技基础".into(),
        text: "这是原学习内容".into(),
        reason: "学习".into(),
        card_id: None,
        quiz: None,
        feedback: None,
        concept_key: None,
    });
    run.state = "waiting".into();
    state.database.study_insert(&run).unwrap();
    state
        .database
        .study_ask(
            &run.id,
            "step1",
            question,
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
    (dir, state, http, run.id)
}
fn run(state: &AppState, id: &str) -> Value {
    tauri::async_runtime::block_on(continue_inner(id.into(), state)).unwrap()
}
fn search_calls(http: &Http) -> usize {
    http.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(url, _)| url.contains("web_search"))
        .count()
}

#[test]
fn p3_search_keeps_step_and_saves_sources_to_history_highlights_and_doubts() {
    let (dir, state, http, id) = setup(
        vec![
            plan(),
            web(),
            answer("这是整理后的解释。", "W1"),
            answer("换个例子说明这个知识。", "W2"),
        ],
        "帮我了解科技资料",
        SearchMode::Always,
    );
    let before = state.database.study_load(&id).unwrap().public();
    let result = run(&state, &id);
    assert_eq!(result["state"], "waiting");
    assert_eq!(result["steps"], before["steps"]);
    assert_eq!(result["summary"], before["summary"]);
    assert_eq!(
        result["questions"][0]["answer"]["search"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let q = result["questions"][0]["id"].as_str().unwrap();
    state
        .database
        .study_save_highlight(&id, "question", q)
        .unwrap();
    state
        .database
        .study_question_feedback(&id, q, "unresolved")
        .unwrap();
    let repeated = run(&state, &id);
    assert_eq!(search_calls(&http), 1);
    assert_eq!(
        repeated["questions"][1]["answer"]["search"]["cacheHit"],
        true
    );
    assert_eq!(
        repeated["questions"][1]["answer"]["text"],
        "换个例子说明这个知识。"
    );
    let reopened = Database::new(dir.path().join("test.db"));
    reopened.initialize().unwrap();
    let notes = reopened.study_highlights(Some(&id), None).unwrap();
    assert_eq!(
        notes[0]["search"]["sources"][0]["url"],
        "https://example.org/reference"
    );
    assert_eq!(notes[0]["text"], "这是整理后的解释。");
    assert_eq!(reopened.study_load(&id).unwrap().model_calls, 3);
}

#[test]
fn p3_auto_routes_current_questions_and_uses_the_pinned_kimi_model() {
    let (_dir, state, http, id) = setup(
        vec![plan(), web(), answer("当天的科技信息。", "W1")],
        "今天的科技新闻",
        SearchMode::Auto,
    );
    state
        .database
        .save_provider_profile(&crate::db::ProviderProfileRecord {
            provider_id: "kimi".into(),
            region: "cn".into(),
            model: "kimi-k3".into(),
            credential_ref: "test".into(),
            key_last4: Some("test".into()),
            connection_verified: true,
        })
        .unwrap();
    let mut session = state.database.study_load(&id).unwrap();
    session.provider = "kimi".into();
    session.region = "cn".into();
    session.model = "kimi-k3".into();
    state.database.study_save(&mut session).unwrap();
    let result = run(&state, &id);
    assert!(result["questions"][0]["answer"].is_object(), "{result}");
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[1].1["search_recency_filter"], "oneDay");
    assert_eq!(requests[0].1["model"], "kimi-k3");
    assert_eq!(requests[2].1["model"], "kimi-k3");
}

#[test]
fn p3_rejects_fabricated_citations_and_reuses_the_same_search_on_repair() {
    let (_dir, state, http, id) = setup(
        vec![
            plan(),
            web(),
            answer("伪引用", "W999"),
            answer("有效引用", "W2"),
        ],
        "今天有什么新闻",
        SearchMode::Auto,
    );
    let result = run(&state, &id);
    assert_eq!(result["questions"][0]["answer"]["text"], "有效引用");
    assert_eq!(search_calls(&http), 1);
    assert_eq!(state.database.study_load(&id).unwrap().model_calls, 3);
    let requests = http.requests.lock().unwrap();
    let retry: Value = serde_json::from_str(
        requests.last().unwrap().1["messages"][1]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(retry["validationFeedback"]
        .as_str()
        .unwrap()
        .contains("不存在的来源"));
}

#[test]
fn p3_repairs_link_text_missing_citations_and_excess_citations_with_specific_feedback() {
    for (text, ids, expected) in [
        ("例子网址 https://example.org", json!(["W1"]), "网址"),
        ("没有引用的开场白", json!([]), "缺少来源"),
        (
            "引用过多",
            json!(["W1", "W2", "W1", "W2", "W1"]),
            "最多引用4个",
        ),
    ] {
        let bad = model(
            json!({"status":"answered","blocks":[{"text":text,"evidenceIds":ids}],"limitation":null}),
        );
        let (_dir, state, http, id) = setup(
            vec![plan(), web(), bad, answer("修正后的正文", "W1")],
            "今天的科技新闻",
            SearchMode::Auto,
        );
        let result = run(&state, &id);
        assert_eq!(result["questions"][0]["answer"]["text"], "修正后的正文");
        assert_eq!(search_calls(&http), 1);
        let requests = http.requests.lock().unwrap();
        let payload: Value = serde_json::from_str(
            requests.last().unwrap().1["messages"][1]["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert!(payload["validationFeedback"]
            .as_str()
            .unwrap()
            .contains(expected));
    }
}

#[test]
fn p3_real_empty_results_do_not_call_the_answer_model() {
    let (_dir, state, http, id) = setup(
        vec![plan(), response(json!({"search_result":[]}))],
        "最新新闻",
        SearchMode::Auto,
    );
    let result = run(&state, &id);
    assert_eq!(
        result["questions"][0]["answer"]["search"]["status"],
        "insufficient"
    );
    assert_eq!(http.requests.lock().unwrap().len(), 2);
}

#[test]
fn p3_off_mode_does_not_invent_current_facts_or_send_requests() {
    let (_dir, state, http, id) = setup(vec![], "今天的新闻", SearchMode::Off);
    let result = run(&state, &id);
    assert!(result["questions"][0]["answer"]["text"]
        .as_str()
        .unwrap()
        .contains("开启智谱"));
    assert!(http.requests.lock().unwrap().is_empty());
}

#[test]
fn p3_pause_and_finish_discard_late_search_and_answer_responses() {
    for finish in [false, true] {
        for at in [2, 3] {
            let (dir, state, http, id) = setup(
                vec![plan(), web(), answer("不应出现的迟到结果", "W1")],
                "今天的新闻",
                SearchMode::Auto,
            );
            *http.pause_at.lock().unwrap() =
                Some((at, dir.path().join("test.db"), id.clone(), finish));
            let result = run(&state, &id);
            assert_eq!(result["state"], if finish { "completed" } else { "paused" });
            assert!(result["questions"][0]["answer"].is_null());
            let saved = state.database.study_load(&id).unwrap();
            assert_eq!(saved.control.tool_attempts["web_search"], 1);
            assert!(saved.control.charged_active_ms >= 20_000);
            assert_eq!(state.database.search_attempts_today().unwrap(), 1);
        }
    }
}

#[test]
fn p3_failed_http_attempts_are_bounded_across_resume_and_charge_parent_budget() {
    let (_dir, state, http, id) = setup(
        vec![
            plan(),
            response(json!({"error":{"code":"1702"}})),
            response(json!({"error":{"code":"1702"}})),
        ],
        "最新新闻",
        SearchMode::Auto,
    );
    let first = run(&state, &id);
    assert_eq!(first["state"], "failed");
    let next = run(&state, &id);
    assert_eq!(next["state"], "failed");
    assert_eq!(search_calls(&http), 2);
    let saved = state.database.study_load(&id).unwrap();
    assert_eq!(saved.control.tool_attempts["web_search"], 2);
    assert_eq!(state.database.search_attempts_today().unwrap(), 2);
}

#[test]
fn p3_force_search_is_snapshotted_and_deduplicates_repeated_submission() {
    let (_dir, state, _, id) = setup(vec![], "普通问题", SearchMode::Off);
    let mut saved = state.database.study_load(&id).unwrap();
    saved.questions.clear();
    saved.state = "waiting".into();
    state.database.study_save(&mut saved).unwrap();
    let request = uuid::Uuid::new_v4().to_string();
    let ask = || {
        state.database.study_ask_with_options(
            &id,
            "step1",
            "什么是光合作用",
            &request,
            QuestionOptions {
                reply_to: None,
                force_search: true,
            },
        )
    };
    let first = ask().unwrap();
    let second = ask().unwrap();
    assert_eq!(first.revision, second.revision);
    assert_eq!(
        first.questions[0].search.as_ref().unwrap().options.mode,
        SearchMode::Always
    );
    assert!(state
        .database
        .study_ask(&id, "step1", "什么是光合作用", &request)
        .is_err());
}

#[test]
fn p3_resume_reuses_completed_search_and_never_duplicates_a_question() {
    let (dir, state, http, id) = setup(
        vec![plan(), web(), answer("迟到回答", "W1")],
        "今天的新闻",
        SearchMode::Auto,
    );
    *http.pause_at.lock().unwrap() = Some((3, dir.path().join("test.db"), id.clone(), false));
    assert_eq!(run(&state, &id)["state"], "paused");
    *http.pause_at.lock().unwrap() = None;
    *http.replies.lock().unwrap() = vec![answer("恢复后整理的回答", "W1")].into();
    let result = run(&state, &id);
    assert_eq!(result["questions"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["questions"][0]["answer"]["text"], "恢复后整理的回答",
        "{result}"
    );
    assert_eq!(search_calls(&http), 1);
    assert_eq!(state.database.study_load(&id).unwrap().model_calls, 3);
}

#[test]
fn p3_expired_evidence_and_explicit_current_questions_search_again() {
    for current in [false, true] {
        let (_dir, state, http, id) = setup(
            vec![
                plan(),
                web(),
                answer("第一版解释", "W1"),
                plan(),
                web(),
                answer("重新查找后的解释", "W2"),
            ],
            if current {
                "今天有哪些科技信息"
            } else {
                "了解科技资料"
            },
            SearchMode::Always,
        );
        let result = run(&state, &id);
        if !current {
            let mut session = state.database.study_load(&id).unwrap();
            for source in &mut session.questions[0]
                .answer
                .as_mut()
                .unwrap()
                .search
                .as_mut()
                .unwrap()
                .sources
            {
                source.retrieved_at =
                    (chrono::Utc::now() - chrono::Duration::minutes(6)).to_rfc3339();
            }
            state.database.study_save(&mut session).unwrap();
        }
        state
            .database
            .study_question_feedback(
                &id,
                result["questions"][0]["id"].as_str().unwrap(),
                "unresolved",
            )
            .unwrap();
        let repeated = run(&state, &id);
        assert_eq!(
            repeated["questions"][1]["answer"]["text"], "重新查找后的解释",
            "{repeated}"
        );
        assert_eq!(search_calls(&http), 2);
        assert_eq!(
            repeated["questions"][1]["answer"]["search"]["cacheHit"],
            false
        );
    }
}

#[test]
fn p3_existing_v1_sessions_and_stable_auto_questions_keep_the_original_flow() {
    let (_dir, state, http, id) = setup(vec![], "为什么需要学习基础知识", SearchMode::Auto);
    let mut legacy = serde_json::to_value(state.database.study_load(&id).unwrap()).unwrap();
    legacy["questions"][0]
        .as_object_mut()
        .unwrap()
        .remove("search");
    legacy["control"]["policy"]
        .as_object_mut()
        .unwrap()
        .remove("max_search_attempts");
    let mut session: StudySession = serde_json::from_value(legacy).unwrap();
    let q = session.questions[0].id.clone();
    state.database.study_save(&mut session).unwrap();
    http.replies.lock().unwrap().push_back(response(json!({"choices":[{"finish_reason":"tool_calls","message":crate::review_tests::tools(vec![("answer1","answer_question",json!({"request_id":q,"kind":"explanation","text":"基础知识帮助理解后续内容。","card_id":null}))])}],"usage":{"total_tokens":50}})));
    let result = run(&state, &id);
    assert_eq!(
        result["questions"][0]["answer"]["text"], "基础知识帮助理解后续内容。",
        "{result}"
    );
    assert!(result["questions"][0]["answer"]["search"].is_null());
    assert_eq!(search_calls(&http), 0);
    assert_eq!(http.requests.lock().unwrap().len(), 1);
    assert_eq!(
        state.database.study_load(&id).unwrap().prompt_version,
        "study-companion-v1"
    );
}

#[test]
fn p3_parent_search_and_model_limits_stop_before_external_requests() {
    for models in [false, true] {
        let (_dir, state, http, id) = setup(
            if models { vec![] } else { vec![plan()] },
            "最新新闻",
            SearchMode::Auto,
        );
        let mut session = state.database.study_load(&id).unwrap();
        if models {
            session.control.policy.max_model_calls = 0;
        } else {
            session.control.policy.max_search_attempts = 0;
        }
        state.database.study_save(&mut session).unwrap();
        if models {
            assert!(tauri::async_runtime::block_on(continue_inner(id.clone(), &state)).is_err());
        } else {
            assert_eq!(run(&state, &id)["state"], "failed");
        }
        assert_eq!(search_calls(&http), 0);
        assert_eq!(http.requests.lock().unwrap().len(), usize::from(!models));
    }
}

#[test]
fn p3_invalid_key_marks_connection_unverified_without_answering_from_memory() {
    let (_dir, state, http, id) = setup(
        vec![plan(), response(json!({"error":{"code":"1002"}}))],
        "今天的新闻",
        SearchMode::Auto,
    );
    let result = run(&state, &id);
    assert_eq!(result["state"], "failed");
    assert!(result["questions"][0]["answer"].is_null());
    assert!(
        !state
            .database
            .search_settings()
            .unwrap()
            .connection_verified
    );
    assert_eq!(search_calls(&http), 1);
}

#[test]
fn p3_doubt_restart_retains_previous_sources_without_repeating_web_search() {
    let (_dir, state, http, id) = setup(
        vec![
            plan(),
            web(),
            answer("原先的解释", "W1"),
            answer("从基础重新解释这个疑问", "W2"),
        ],
        "了解科技资料",
        SearchMode::Always,
    );
    let result = run(&state, &id);
    let q = result["questions"][0]["id"].as_str().unwrap();
    state.database.study_pause(&id, true).unwrap();
    state
        .database
        .study_question_feedback(&id, q, "unresolved")
        .unwrap();
    let doubt = crate::study_commands::start_doubt_inner(format!("{id}:{q}"), &state).unwrap();
    let result = run(&state, doubt["id"].as_str().unwrap());
    assert_eq!(
        result["questions"][0]["answer"]["text"], "从基础重新解释这个疑问",
        "{result}"
    );
    assert_eq!(result["questions"][0]["answer"]["search"]["cacheHit"], true);
    assert_eq!(search_calls(&http), 1);
}
