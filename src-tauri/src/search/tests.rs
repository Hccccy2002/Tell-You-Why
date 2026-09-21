use super::*;
use super::{
    types::*,
    zhipu::{parse_response, request_body},
};
use crate::{
    providers::{ProviderError, RestrictedHttpClient, TransportResponse},
    secret_store::tests_support::MemorySecretStore,
};
use std::sync::{atomic::AtomicBool, Arc};

fn state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::new(dir.path().join("test.db"));
    database.initialize().unwrap();
    (
        dir,
        AppState {
            database,
            secrets: Arc::new(MemorySecretStore::default()),
            http: Arc::new(RestrictedHttpClient::new().unwrap()),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        },
    )
}

struct AgentTransport {
    responses: std::sync::Mutex<std::collections::VecDeque<TransportResponse>>,
    requests: std::sync::Mutex<Vec<(String, Value)>>,
}
#[async_trait::async_trait]
impl ProviderTransport for AgentTransport {
    async fn post_json(
        &self,
        url: &Url,
        _: &SecretValue,
        body: &Value,
        _: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        self.requests
            .lock()
            .unwrap()
            .push((url.host_str().unwrap().into(), body.clone()));
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra paid request"))
    }
}
fn model_response(value: Value) -> TransportResponse {
    TransportResponse {
        status: 200,
        body: json!({"choices":[{"message":{"content":value.to_string()},"finish_reason":"stop"}]})
            .to_string(),
    }
}
fn plan_response() -> TransportResponse {
    model_response(json!({"needsSearch":true,"query":"Node.js LTS release","recency":"noLimit"}))
}
fn web_response(date: Option<String>) -> TransportResponse {
    TransportResponse{status:200,body:json!({"search_result":[{"title":"Official release","link":"https://nodejs.org/en/blog/release","content":"A supported LTS release is available.","publish_date":date}]}).to_string()}
}
fn grounded_response(id: &str) -> TransportResponse {
    model_response(
        json!({"status":"partial","blocks":[{"text":"资料说明有受支持的 LTS 发布。","evidenceIds":[id]}],"limitation":"此资料不足以确认当前最新版本。"}),
    )
}
fn agent_state(
    responses: Vec<TransportResponse>,
) -> (
    tempfile::TempDir,
    AppState,
    Arc<AgentTransport>,
    String,
    String,
) {
    let (dir, mut state) = state();
    save_key(
        SaveSearchKey {
            api_key: "sk-search-test-mock".into(),
            replace_existing_key: false,
        },
        &state,
    )
    .unwrap();
    state
        .database
        .set_search_verified(&state.database.search_credential().unwrap().unwrap(), true)
        .unwrap();
    for (provider, region, model) in [
        ("deepseek", "default", "deepseek-v4-flash"),
        ("kimi", "cn", "kimi-k3"),
    ] {
        state
            .secrets
            .save(provider, "sk-provider-test-mock")
            .unwrap();
        state
            .database
            .save_provider_profile(&crate::db::ProviderProfileRecord {
                provider_id: provider.into(),
                region: region.into(),
                model: model.into(),
                credential_ref: provider.into(),
                key_last4: Some("mock".into()),
                connection_verified: true,
            })
            .unwrap();
    }
    let http = Arc::new(AgentTransport {
        responses: std::sync::Mutex::new(responses.into()),
        requests: Default::default(),
    });
    state.http = http.clone();
    let card = state.database.next_card(None).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    state
        .database
        .search_start(
            &id,
            &card.id,
            &SearchOptions {
                mode: SearchMode::Always,
                ..Default::default()
            },
        )
        .unwrap();
    (dir, state, http, card.id, id)
}

#[test]
fn p2_fallback_reuses_evidence_and_persists_sources_with_display_question() {
    let (_dir, state, http, card, id) = agent_state(vec![
        plan_response(),
        web_response(Some(chrono::Utc::now().date_naive().to_string())),
        grounded_response("forged"),
        grounded_response("W1"),
    ]);
    let result = tauri::async_runtime::block_on(agent::answer(
        &state,
        &id,
        &card,
        "隐藏的额外选中上下文",
        "Node.js 最新 LTS",
        vec![],
    ))
    .unwrap();
    assert_eq!(result.provider_id, "kimi");
    assert_eq!(result.search.as_ref().unwrap().status, "partial");
    let calls = http.requests.lock().unwrap();
    assert_eq!(calls.len(), 4); // 1 plan + 1 search + 2 model attempts, no second search.
    assert_eq!(
        calls
            .iter()
            .filter(|(host, _)| host == "open.bigmodel.cn")
            .count(),
        1
    );
    assert_eq!(calls[1].1["search_query"], "Node.js LTS release");
    assert!(!calls[1].1.to_string().contains("隐藏"));
    let first: Value =
        serde_json::from_str(calls[2].1["messages"][1]["content"].as_str().unwrap()).unwrap();
    let second: Value =
        serde_json::from_str(calls[3].1["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(first["sources"], second["sources"]);
    drop(calls);
    state
        .database
        .save_follow_up_exchange_for_run(&card, "显示问题", "隐藏上下文", &result, Some(&id))
        .unwrap();
    state.database.initialize().unwrap();
    let history = state.database.card_follow_ups(&card).unwrap();
    assert_eq!(history[0].content, "显示问题");
    assert_eq!(history[0].request_content.as_deref(), Some("隐藏上下文"));
    assert_eq!(history[1].result.as_ref().unwrap(), &result);
    assert!(agent::resolve_run(&state, &card, Some(id.clone())).is_err());
    state.database.delete_library_card(&card).unwrap();
    assert!(state
        .database
        .search_run_state(&id, &card)
        .unwrap()
        .is_none());
    assert_eq!(
        state
            .database
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM follow_up_search", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn p2_unknown_dates_reach_the_answer_model_instead_of_becoming_empty_results() {
    let (_dir, state, http, card, id) = agent_state(vec![
        plan_response(),
        web_response(None),
        grounded_response("W1"),
    ]);
    let result = tauri::async_runtime::block_on(agent::answer(
        &state,
        &id,
        &card,
        "最新版本",
        "最新版本",
        vec![],
    ))
    .unwrap();
    let search = result.search.as_ref().unwrap();
    assert_eq!(search.status, "partial");
    assert_eq!(search.sources.len(), 1);
    assert_eq!(search.sources[0].published_at, None);
    assert_eq!(search.blocks[0].evidence_ids, vec!["W1"]);
    let calls = http.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let payload: Value =
        serde_json::from_str(calls[2].1["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert!(payload["sources"][0]["publishedAt"].is_null());
    assert_eq!(
        payload["sources"][0]["snippet"],
        "A supported LTS release is available."
    );
    assert_eq!(state.database.search_attempts_today().unwrap(), 1);
}

#[test]
fn p2_rolling_weather_page_uses_content_dates_and_preserves_source_provenance() {
    let today = chrono::Local::now().date_naive().to_string();
    let snippet = format!("上海天气预报，预报有效日期 {today}，测试样例：多云，20至25摄氏度。");
    for published in [None, Some("2020-01-01")] {
        let (_dir, state, http, card, id) =
            agent_state(vec![
            model_response(json!({"needsSearch":true,"query":"上海天气预报","recency":"noLimit"})),
            TransportResponse { status: 200, body: json!({"search_result":[{
                "title":"上海天气预报（合成测试）", "link":"https://weather.example/shanghai",
                "content":snippet, "publish_date":published
            }]}).to_string() },
            model_response(json!({"status":"answered","blocks":[{
                "text":format!("根据预报，{today} 上海多云，20至25摄氏度。"), "evidenceIds":["W1"]
            }],"limitation":null})),
        ]);
        let result = tauri::async_runtime::block_on(agent::answer(
            &state,
            &id,
            &card,
            "今天上海天气如何",
            "今天上海天气如何",
            vec![],
        ))
        .unwrap();
        let search = result.search.as_ref().unwrap();
        assert_eq!(search.status, "answered");
        assert_eq!(search.sources[0].published_at.as_deref(), published);
        assert_eq!(search.sources[0].snippet, snippet);
        let calls = http.requests.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[1].0, "open.bigmodel.cn");
        assert_eq!(calls[1].1["search_recency_filter"], "oneDay");
        let payload: Value =
            serde_json::from_str(calls[2].1["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(payload["sources"][0]["snippet"], snippet);
        assert_eq!(payload["today"], today);
        assert_eq!(payload["recency"], "oneDay");
        state
            .database
            .save_follow_up_exchange_for_run(
                &card,
                "今天上海天气如何",
                "今天上海天气如何",
                &result,
                Some(&id),
            )
            .unwrap();
        assert_eq!(
            state.database.card_follow_ups(&card).unwrap()[1]
                .result
                .as_ref(),
            Some(&result)
        );
    }
}

#[test]
fn p2_genuinely_empty_results_do_not_call_the_answer_model() {
    let (_dir, state, http, card, id) = agent_state(vec![
        plan_response(),
        TransportResponse {
            status: 200,
            body: json!({"search_result":[]}).to_string(),
        },
    ]);
    let result = tauri::async_runtime::block_on(agent::answer(
        &state,
        &id,
        &card,
        "今天上海天气如何",
        "今天上海天气如何",
        vec![],
    ))
    .unwrap();
    assert_eq!(result.search.as_ref().unwrap().status, "insufficient");
    assert!(result.search.as_ref().unwrap().sources.is_empty());
    assert!(result.answer.contains("结果列表为空"));
    assert_eq!(http.requests.lock().unwrap().len(), 2);
}

#[test]
fn linkless_provider_text_reaches_the_model_and_round_trips_for_multiple_topics() {
    for topic in [
        "介绍几个今天的科技新闻",
        "近期软件发布记录",
        "解释一种科学原理",
    ] {
        let (_dir, state, http, card, id) = agent_state(vec![
            model_response(json!({"needsSearch":true,"query":topic,"recency":"noLimit"})),
            TransportResponse { status: 200, body: json!({"search_result":[
                {"title":"测试资料一","content":"第一条公开摘要（合成测试）。","link":"","media":"","publish_date":"2026-09-21"},
                {"title":"测试资料二","content":"第二条公开摘要（合成测试）。","link":null},
                {"title":"测试资料三","content":"第三条公开摘要（合成测试）。"}
            ]}).to_string() },
            model_response(json!({"status":"partial","blocks":[{"text":"依据搜索摘要整理的测试回答。","evidenceIds":["W1","W2","W3"]}],"limitation":"搜索结果未提供原文链接。"})),
        ]);
        let result =
            tauri::async_runtime::block_on(agent::answer(&state, &id, &card, topic, topic, vec![]))
                .unwrap();
        let search = result.search.as_ref().unwrap();
        assert_eq!(search.sources.len(), 3);
        assert!(search.sources.iter().all(|s| s.url.is_none()));
        let calls = http.requests.lock().unwrap();
        assert_eq!(calls.len(), 3);
        let payload: Value =
            serde_json::from_str(calls[2].1["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(payload["sources"].as_array().unwrap().len(), 3);
        assert!(payload["sources"][0]["url"].is_null());
        assert_eq!(
            payload["sources"][1]["snippet"],
            "第二条公开摘要（合成测试）。"
        );
        drop(calls);
        state
            .database
            .save_follow_up_exchange_for_run(&card, topic, topic, &result, Some(&id))
            .unwrap();
        state.database.initialize().unwrap();
        assert_eq!(
            state.database.card_follow_ups(&card).unwrap()[1]
                .result
                .as_ref(),
            Some(&result)
        );
    }
}

#[test]
fn parser_keeps_text_when_optional_metadata_is_missing_or_links_are_unusable() {
    let rows = json!({"search_result":[
        {"title":"缺少链接","content":"仍可整理的摘要","link":""},
        {"title":"缺少链接","content":"仍可整理的摘要","link":null},
        {"content":"缺少标题和链接的另一条摘要"},
        {"title":"无效链接","content":"文本仍可显示","link":"javascript:alert(1)"},
        {"title":"旧式链接","content":"文本保留但不自动升级地址","link":"http://example.com/page"},
        {"title":"有效链接","content":"可打开原文","link":"https://example.com/page#section"},
        {"title":"没有正文","link":"https://example.com/empty"}
    ]})
    .to_string();
    let evidence = parse_response(200, &rows, &sample_request()).unwrap();
    assert_eq!(evidence.len(), 5);
    assert_eq!(evidence[0].id, "W1");
    assert_eq!(evidence[1].title, "智谱搜索摘要 3");
    assert!(evidence[..4].iter().all(|source| source.url.is_none()));
    assert_eq!(evidence[4].url.as_deref(), Some("https://example.com/page"));
    assert!(validate_probe(200, &rows).is_ok());
}

#[test]
fn parser_distinguishes_empty_provider_results_from_results_without_usable_text() {
    let request = sample_request();
    assert!(parse_response(200, r#"{"search_result":[]}"#, &request)
        .unwrap()
        .is_empty());
    let empty_content =
        r#"{"search_result":[{"title":"标题","content":" ","link":"https://example.com"}]}"#;
    assert_eq!(
        parse_response(200, empty_content, &request),
        Err(SearchError::UnusableResults {
            received: 1,
            without_text: 1,
            outside_domain: 0,
        })
    );
    let mut restricted = request;
    restricted.domain = Some("nodejs.org".into());
    assert_eq!(
        parse_response(
            200,
            r#"{"search_result":[{"title":"摘要","content":"来源地址未知"}]}"#,
            &restricted
        ),
        Err(SearchError::UnusableResults {
            received: 1,
            without_text: 0,
            outside_domain: 1,
        })
    );
    let old: WebEvidence = serde_json::from_value(json!({"id":"W1","title":"历史资料","url":"https://example.com","snippet":"旧版本的证据快照","publisher":null,"publishedAt":null,"retrievedAt":"2026-09-21T00:00:00Z"})).unwrap();
    assert_eq!(old.url.as_deref(), Some("https://example.com"));
}

#[test]
fn p2_cancelled_and_foreign_runs_never_commit_a_late_answer() {
    let (_dir, state, http, card, id) = agent_state(vec![
        plan_response(),
        web_response(None),
        grounded_response("W1"),
    ]);
    assert!(agent::resolve_run(&state, "foreign", Some(id.clone())).is_err());
    let result = tauri::async_runtime::block_on(agent::answer(
        &state,
        &id,
        &card,
        "联网核查 Node.js",
        "联网核查 Node.js",
        vec![],
    ))
    .unwrap();
    state
        .database
        .search_set_state(&id, "cancelled", None)
        .unwrap();
    assert!(state
        .database
        .save_follow_up_exchange_for_run(&card, "问题", "问题", &result, Some(&id))
        .is_err());
    assert!(state.database.card_follow_ups(&card).unwrap().is_empty());
    assert!(agent::resolve_run(&state, &card, Some(id.clone())).is_err());
    assert_eq!(http.requests.lock().unwrap().len(), 3);
}

#[test]
fn p2_rejects_private_or_oversized_plans_before_search() {
    for query in [
        "a@example.com".into(),
        "查找 13800138000".into(),
        "a".repeat(71),
    ] {
        let (_dir, state, http, card, id) = agent_state(vec![model_response(
            json!({"needsSearch":true,"query":query,"recency":"noLimit"}),
        )]);
        assert!(tauri::async_runtime::block_on(agent::answer(
            &state,
            &id,
            &card,
            "联网查询",
            "联网查询",
            vec![]
        ))
        .is_err());
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        assert_eq!(state.database.search_attempts_today().unwrap(), 0);
    }
}

#[test]
fn p2_routing_keeps_stable_questions_local_and_detects_implicit_freshness() {
    for q in ["解释光合作用的原理", "什么是 TCP", "why is the sky blue"] {
        assert!(routing::stable(q));
        assert!(!routing::is_current(q));
    }
    for q in [
        "这个库还在维护吗",
        "现行收费价格",
        "目前的最新版本",
        "current price",
    ] {
        assert!(routing::is_current(q));
        assert!(!routing::stable(q));
    }
    assert!(routing::explicit_search("联网核查 TCP"));
}

#[test]
fn search_keys_persist_metadata_only_and_require_confirmed_replacement() {
    let (_dir, state) = state();
    assert!(!state.database.search_settings().unwrap().key_configured);
    let result = save_key(
        SaveSearchKey {
            api_key: "p0-mock-secret-1234".into(),
            replace_existing_key: false,
        },
        &state,
    )
    .unwrap();
    assert_eq!(result.key_last4.as_deref(), Some("1234"));
    assert!(!result.connection_verified);
    let first = state.database.search_credential().unwrap().unwrap();
    assert!(state.database.provider_profiles().unwrap().is_empty());
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("p0-mock-secret"));
    assert!(save_key(
        SaveSearchKey {
            api_key: "p0-mock-secret-5678".into(),
            replace_existing_key: false
        },
        &state
    )
    .is_err());
    assert_eq!(
        state.secrets.get(&first).unwrap().expose(),
        "p0-mock-secret-1234"
    );
    state.database.set_search_verified(&first, true).unwrap();
    let replaced = save_key(
        SaveSearchKey {
            api_key: "p0-mock-secret-5678".into(),
            replace_existing_key: true,
        },
        &state,
    )
    .unwrap();
    assert!(!replaced.connection_verified);
    assert!(state.secrets.get(&first).is_err());
    state.database.initialize().unwrap();
    assert_eq!(
        state
            .database
            .search_settings()
            .unwrap()
            .key_last4
            .as_deref(),
        Some("5678")
    );
    let conn = state.database.connect().unwrap();
    let saved: String = conn
        .query_row(
            "SELECT credential_ref || key_last4 FROM search_profiles",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!saved.contains("p0-mock-secret"));
}

#[test]
fn pending_cleanup_protects_active_search_keys_and_all_clear_removes_them() {
    let (_dir, state) = state();
    save_key(
        SaveSearchKey {
            api_key: "p0-mock-secret-1234".into(),
            replace_existing_key: false,
        },
        &state,
    )
    .unwrap();
    let credential = state.database.search_credential().unwrap().unwrap();
    state
        .database
        .queue_credential_deletion(&credential)
        .unwrap();
    cleanup_pending_credentials(&state).unwrap();
    assert!(state.secrets.get(&credential).is_ok());
    state.database.clear_data("all").unwrap();
    cleanup_pending_credentials(&state).unwrap();
    assert!(state.secrets.get(&credential).is_err());
    assert!(!state.database.search_settings().unwrap().key_configured);
}

#[test]
fn failed_activation_keeps_the_previous_key_and_cleans_the_candidate() {
    let (_dir, state) = state();
    save_key(
        SaveSearchKey {
            api_key: "p0-original-1234".into(),
            replace_existing_key: false,
        },
        &state,
    )
    .unwrap();
    let original = state.database.search_credential().unwrap().unwrap();
    state.database.connect().unwrap().execute_batch("CREATE TRIGGER reject_search_update BEFORE UPDATE ON search_profiles BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    assert!(save_key(
        SaveSearchKey {
            api_key: "p0-replacement-5678".into(),
            replace_existing_key: true
        },
        &state
    )
    .is_err());
    assert_eq!(
        state.database.search_credential().unwrap().unwrap(),
        original
    );
    assert_eq!(
        state.secrets.get(&original).unwrap().expose(),
        "p0-original-1234"
    );
    assert!(state
        .database
        .pending_credential_deletions()
        .unwrap()
        .is_empty());
}

#[test]
fn deletion_is_detached_before_system_cleanup_and_can_be_retried() {
    let (_dir, state) = state();
    save_key(
        SaveSearchKey {
            api_key: "p0-mock-secret-1234".into(),
            replace_existing_key: false,
        },
        &state,
    )
    .unwrap();
    let credential = state.database.search_credential().unwrap().unwrap();
    state.database.detach_search_key().unwrap();
    assert!(!state.database.search_settings().unwrap().key_configured);
    assert_eq!(
        state.database.pending_credential_deletions().unwrap(),
        vec![credential.clone()]
    );
    cleanup_pending_credentials(&state).unwrap();
    assert!(state.secrets.get(&credential).is_err());
    state.database.detach_search_key().unwrap();
}

#[test]
fn probe_requires_a_real_usable_search_result_and_redacts_errors() {
    assert!(validate_probe(200, &json!({"search_result":[{"title":"Node.js","content":"公开发布记录","link":"https://nodejs.org/en/blog/"}]}).to_string()).is_ok());
    for body in [
        json!({}),
        json!({"search_result":[]}),
        json!({"search_result":[{"title":"test","content":"","link":"https://example.com"}]}),
        json!({"search_intent":[{"intent":"SEARCH_NONE"}],"search_result":[]}),
    ] {
        assert!(validate_probe(200, &body.to_string()).is_err());
    }
    let error = validate_probe(
        429,
        r#"{"error":{"code":"1113","message":"do-not-leak-secret"}}"#,
    )
    .unwrap_err();
    assert!(error.contains("余额不足"));
    assert!(!error.contains("do-not-leak-secret"));
}

#[test]
fn search_transport_allows_only_the_documented_endpoint() {
    let client = RestrictedHttpClient::new().unwrap();
    assert!(client.validate_url(&Url::parse(ENDPOINT).unwrap()).is_ok());
    for target in [
        "https://open.bigmodel.cn/api/paas/v4/chat/completions",
        "https://open.bigmodel.cn/api/paas/v4/web_search?key=secret",
        "http://open.bigmodel.cn/api/paas/v4/web_search",
        "https://user@open.bigmodel.cn/api/paas/v4/web_search",
        "https://open.bigmodel.cn.evil.test/api/paas/v4/web_search",
    ] {
        assert!(client.validate_url(&Url::parse(target).unwrap()).is_err());
    }
}

struct ProbeTransport;
#[async_trait::async_trait]
impl ProviderTransport for ProbeTransport {
    async fn post_json(
        &self,
        endpoint: &Url,
        _key: &SecretValue,
        body: &Value,
        timeout: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        assert_eq!(endpoint.as_str(), ENDPOINT);
        assert_eq!(body["search_intent"], false);
        assert_eq!(body["search_engine"], "search_pro");
        assert!(!body.to_string().contains("messages"));
        assert!(body["search_query"].as_str().unwrap().chars().count() <= 70);
        assert_eq!(timeout, Duration::from_secs(20));
        Ok(TransportResponse { status: 200, body: json!({"search_result":[{"title":"公开资料","content":"发布记录","link":"https://nodejs.org/en/blog/"}]}).to_string() })
    }
}

#[test]
fn connection_probe_sends_only_a_fixed_public_query_without_user_content() {
    tauri::async_runtime::block_on(probe(&ProbeTransport, &SecretValue::for_test("mock-key")))
        .unwrap();
}

#[test]
#[ignore = "P0 live public-query probe using the configured Zhipu credential; at most four HTTP calls"]
fn p0_live_contract_probe() {
    use crate::secret_store::{SecretStore, WindowsCredentialStore};
    // An explicit application-owned reference avoids accidentally selecting an old
    // installed app's database when the source app uses a different data directory.
    let credential = std::env::var("TELLWHY_SEARCH_CREDENTIAL_REF")
        .expect("Set the application-owned search credential reference (never the Key)");
    assert!(credential.starts_with("zhipu-search:cn:") && credential.len() < 100);
    let secret = WindowsCredentialStore
        .get(&credential)
        .expect("Cannot read configured credential");
    let transport = RestrictedHttpClient::new().unwrap();
    let mut report = Vec::new();
    tauri::async_runtime::block_on(async {
        for (name, engine, query, domain, invalid) in [
            (
                "success_pro",
                "search_pro",
                "Node.js 最新 LTS 发布记录".to_owned(),
                "nodejs.org",
                false,
            ),
            (
                "success_std",
                "search_std",
                "Node.js 最新 LTS 发布记录".to_owned(),
                "nodejs.org",
                false,
            ),
            (
                "empty",
                "search_pro",
                format!("tellyouwhy-empty-{}", uuid::Uuid::new_v4()),
                "example.invalid",
                false,
            ),
            (
                "invalid_key",
                "search_pro",
                "Node.js 官方发布记录".to_owned(),
                "nodejs.org",
                true,
            ),
        ] {
            let fake = SecretValue::for_test("p0-deliberately-invalid-token");
            let body = json!({"search_query":query,"search_engine":engine,"search_intent":false,"count":5,
                "content_size":"medium","search_recency_filter":"noLimit","search_domain_filter":domain,
                "request_id":uuid::Uuid::new_v4().to_string()});
            let started = std::time::Instant::now();
            let response = transport
                .post_json(
                    &Url::parse(ENDPOINT).unwrap(),
                    if invalid { &fake } else { &secret },
                    &body,
                    Duration::from_secs(20),
                )
                .await;
            let row = match response {
                Ok(response) => {
                    let value: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
                    let code = value["error"]["code"].as_str().unwrap_or("");
                    let results = value["search_result"].as_array();
                    let passed = match name {
                        "empty" => {
                            code == "1703"
                                || ((200..300).contains(&response.status)
                                    && value.get("error").is_none()
                                    && results.is_some_and(|r| r.is_empty()))
                        }
                        "invalid_key" => {
                            response.status == 401
                                || ["1000", "1001", "1002", "1003", "1004"].contains(&code)
                        }
                        _ => validate_probe(response.status, &response.body).is_ok(),
                    };
                    let sources: Vec<Value> = results.into_iter().flatten().take(5).map(|r| json!({
                        "title":r["title"],"url":r["link"],"snippet":r["content"],"publisher":r["media"],"publishedAt":r["publish_date"]
                    })).collect();
                    json!({"name":name,"engine":engine,"httpStatus":response.status,"errorCode":code,
                        "passed":passed,"durationMs":started.elapsed().as_millis(),"sources":sources})
                }
                Err(error) => {
                    json!({"name":name,"passed":false,"errorCode":error.code(),"durationMs":started.elapsed().as_millis()})
                }
            };
            println!(
                "{}: passed={}, http={}, results={}",
                name,
                row["passed"],
                row["httpStatus"],
                row["sources"].as_array().map_or(0, Vec::len)
            );
            let account_error = !invalid
                && [
                    "1000", "1001", "1002", "1003", "1004", "1113", "1311", "1315",
                ]
                .contains(&row["errorCode"].as_str().unwrap_or(""));
            report.push(row);
            if account_error {
                break;
            }
        }
    });
    let folder = std::path::Path::new("../tmp/search-agent");
    std::fs::create_dir_all(folder).unwrap();
    let output = json!({"retrievedAt":chrono::Utc::now().to_rfc3339(),"provenance":"live API; public queries; credentials and raw errors excluded","attempts":report});
    let text = serde_json::to_string_pretty(&output)
        .unwrap()
        .replace(secret.expose(), "[REDACTED]");
    std::fs::write(folder.join("p0-live.json"), text).unwrap();
    assert!(
        report.len() == 4 && report.iter().all(|r| r["passed"] == true),
        "Live probe failed; inspect redacted tmp/search-agent/p0-live.json"
    );
}

fn sample_request() -> SearchRequest {
    SearchRequest {
        query: "公开发布记录".into(),
        engine: "search_pro".into(),
        recency: "noLimit".into(),
        domain: None,
    }
}

#[test]
fn search_preserves_date_metadata_without_dropping_otherwise_usable_results() {
    let today = chrono::Utc::now().date_naive().to_string();
    let rows=json!({"search_result":[
        {"title":"相似域名","link":"https://cnodejs.org/post","content":"不是指定官网","publish_date":today},
        {"title":"官网","link":"https://nodejs.org/blog#part1","content":"公开发布记录","publish_date":today},
        {"title":"同页","link":"https://nodejs.org/blog#part2","content":"重复来源","publish_date":today},
        {"title":"未标日期","link":"https://nodejs.org/unknown","content":"日期缺失"},
        {"title":"未来","link":"https://nodejs.org/future","content":"不适用","publish_date":"2099-01-01"},
        {"title":"持续更新页面","link":"https://nodejs.org/rolling","content":"发布记录","publish_date":"2020-01-01"},
        {"title":"不安全","link":"http://nodejs.org/unsafe","content":"非HTTPS","publish_date":today}
    ]}).to_string();
    let mut request = sample_request();
    request.domain = Some("nodejs.org".into());
    request.recency = "oneWeek".into();
    let evidence = parse_response(200, &rows, &request).unwrap();
    assert_eq!(evidence.len(), 4);
    assert_eq!(evidence[0].id, "W1");
    assert_eq!(evidence[0].url.as_deref(), Some("https://nodejs.org/blog"));
    assert_eq!(evidence[1].published_at, None);
    assert_eq!(evidence[2].published_at, None);
    assert_eq!(evidence[3].published_at.as_deref(), Some("2020-01-01"));
    request.recency = "noLimit".into();
    assert_eq!(
        parse_response(200, &rows, &request).unwrap().len(),
        evidence.len()
    );
}

#[test]
fn p1_request_contract_and_business_errors_are_enforced_in_rust() {
    let mut request = sample_request();
    assert_eq!(request_body(&request).unwrap()["search_intent"], false);
    request.query = "知".repeat(71);
    assert!(request_body(&request).is_err());
    request.query = "知".repeat(70);
    assert!(request_body(&request).is_ok());
    for (code, expected) in [
        ("1113", SearchError::Balance),
        ("1000", SearchError::InvalidKey),
        ("1311", SearchError::Permission),
        ("1701", SearchError::RateLimited),
        ("1702", SearchError::Unavailable),
    ] {
        assert_eq!(
            parse_response(
                429,
                &json!({"error":{"code":code,"message":"secret-do-not-echo"}}).to_string(),
                &request
            )
            .unwrap_err(),
            expected
        );
    }
    assert!(
        parse_response(200, r#"{"error":{"code":"1703"}}"#, &request)
            .unwrap()
            .is_empty()
    );
    assert!(parse_response(200, "{}", &request).is_err());
}

#[test]
fn p1_limits_count_attempts_atomically_including_concurrent_requests() {
    let (_dir, state) = state();
    state.database.reserve_search_attempt("same", 50).unwrap();
    state.database.reserve_search_attempt("same", 50).unwrap();
    assert_eq!(
        state.database.reserve_search_attempt("same", 50),
        Err(SearchError::Budget)
    );
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let db = state.database.clone();
            std::thread::spawn(move || db.reserve_search_attempt(&format!("run-{i}"), 5).is_ok())
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        3
    );
    assert_eq!(state.database.search_attempts_today().unwrap(), 5);
}

#[test]
fn p1_configuration_validation_and_restart_keep_search_disabled_by_default() {
    let (_dir, state) = state();
    assert_eq!(
        state.database.search_options().unwrap().mode,
        SearchMode::Off
    );
    let options = SearchOptions {
        mode: SearchMode::Auto,
        engine: "search_std".into(),
        daily_attempt_limit: 10,
    };
    state.database.save_search_options(&options).unwrap();
    state.database.initialize().unwrap();
    assert_eq!(state.database.search_options().unwrap(), options);
    assert!(state
        .database
        .save_search_options(&SearchOptions {
            engine: "unknown".into(),
            ..options.clone()
        })
        .is_err());
    assert!(state
        .database
        .save_search_options(&SearchOptions {
            daily_attempt_limit: 0,
            ..options
        })
        .is_err());
    state.database.clear_data("all").unwrap();
    assert_eq!(
        state.database.search_options().unwrap().mode,
        SearchMode::Off
    );
}

#[test]
fn p1_cancellation_drops_the_inflight_future_and_never_replays_after_restart() {
    let (_dir, state) = state();
    let card = state.database.next_card(None).unwrap();
    state
        .database
        .search_start("cancel", &card.id, &SearchOptions::default())
        .unwrap();
    let db = state.database.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        db.search_set_state("cancel", "cancelled", None).unwrap();
    });
    let result = tauri::async_runtime::block_on(policy::guarded(
        &state.database,
        "cancel",
        &card.id,
        std::time::Instant::now() + Duration::from_secs(2),
        std::future::pending::<()>(),
    ));
    assert_eq!(result, Err(SearchError::Cancelled));
    worker.join().unwrap();
    assert!(!state
        .database
        .search_set_state("cancel", "completed", None)
        .unwrap());
    state
        .database
        .search_start("interrupted", &card.id, &SearchOptions::default())
        .unwrap();
    state.database.search_recover().unwrap();
    assert_eq!(
        state
            .database
            .search_run_state("interrupted", &card.id)
            .unwrap()
            .as_deref(),
        Some("interrupted")
    );
}

#[test]
fn p1_expiring_cache_does_not_remove_the_saved_evidence_snapshot() {
    let (_dir, state) = state();
    let card = state.database.next_card(None).unwrap();
    let sources=parse_response(200,r#"{"search_result":[{"title":"公开資料","content":"示例摘要","link":"https://example.com/page"}]}"#,&sample_request()).unwrap();
    state
        .database
        .search_start("snapshot", &card.id, &SearchOptions::default())
        .unwrap();
    state
        .database
        .search_set_state("snapshot", "searching", None)
        .unwrap();
    state
        .database
        .search_snapshot("snapshot", &sources)
        .unwrap();
    state.database.search_cache_put("cache", &sources).unwrap();
    assert!(state.database.search_cache_get("cache").unwrap().is_some());
    state
        .database
        .connect()
        .unwrap()
        .execute("UPDATE search_cache SET expires_at=0", [])
        .unwrap();
    assert!(state.database.search_cache_get("cache").unwrap().is_none());
    let saved: String = state
        .database
        .connect()
        .unwrap()
        .query_row(
            "SELECT evidence_json FROM search_runs WHERE id='snapshot'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(saved.contains("示例摘要"));
}

#[test]
#[ignore = "one real search through the production adapter using the configured application credential"]
fn p1_live_adapter() {
    use super::zhipu::SearchProvider;
    use crate::secret_store::{SecretStore, WindowsCredentialStore};
    let credential = std::env::var("TELLWHY_SEARCH_CREDENTIAL_REF").unwrap();
    assert!(credential.starts_with("zhipu-search:cn:"));
    let secret = WindowsCredentialStore.get(&credential).unwrap();
    let results = tauri::async_runtime::block_on(zhipu::ZhipuSearch.search(
        &sample_request(),
        &secret,
        &RestrictedHttpClient::new().unwrap(),
    ))
    .unwrap();
    assert!(!results.is_empty(), "No usable search results");
    println!(
        "Production adapter returned {} normalized sources; credential not logged",
        results.len()
    );
}
