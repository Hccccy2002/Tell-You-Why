//! End-to-end fault injection through the real loop, SQLite and completion validator.
use crate::{
    db::Database,
    review_agent::{ReviewLibrary, ToolCall},
    review_commands::continue_inner,
    review_tests::{quiz, say, setup, start, tools, Library},
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct FaultLibrary<'a> {
    base: &'a Library,
    database: Database,
    run_id: String,
    fault: String,
    searches: AtomicUsize,
}
#[async_trait]
impl ReviewLibrary for FaultLibrary<'_> {
    async fn call(&self, request: Value) -> Result<Value, String> {
        if request["op"] == "evidence" && self.searches.fetch_add(1, Ordering::SeqCst) == 0 {
            if self.fault == "tool_timeout" {
                return std::future::pending().await;
            }
            if self.fault == "inflight_pause" {
                self.database
                    .review_cancel(&self.run_id)
                    .map_err(|e| e.to_string())?;
                return std::future::pending().await;
            }
        }
        self.base.call(request).await
    }
}

fn script(case: &str) -> Vec<Value> {
    if case == "no_progress" {
        return ["a", "b", "c"]
            .map(|id| {
                tools(vec![(
                    id,
                    "search_textbook",
                    json!({"query":"存储器","mode":"keyword"}),
                )])
            })
            .to_vec();
    }
    if case == "token_limit" {
        return vec![];
    }
    let mut replies = vec![];
    if case == "transient_retry" {
        replies.push(json!("NETWORK_ERROR"));
    }
    if case == "premature_finish" {
        replies.push(say("复习已经完成。"));
    }
    if ["tool_timeout", "inflight_pause"].contains(&case) {
        replies.push(tools(vec![(
            "search1",
            "search_textbook",
            json!({"query":"程序数据","mode":"keyword"}),
        )]));
        if case == "inflight_pause" {
            return replies;
        }
        replies.push(tools(vec![(
            "search2",
            "search_textbook",
            json!({"query":"存储器","mode":"keyword"}),
        )]));
    }
    replies.push(tools(vec![(
        "save",
        "save_review_question",
        quiz(if case == "tool_timeout" {
            json!(["S1"])
        } else {
            json!([])
        }),
    )]));
    replies.push(say("请独立选择答案。"));
    replies.push(tools(vec![(
        "grade",
        "record_quiz_result",
        json!({"question_id":"q1"}),
    )]));
    if case == "checkpoint_redelivery" {
        replies.extend([json!("NETWORK_ERROR"), json!("NETWORK_ERROR")]);
    }
    replies.push(say("复习完成。"));
    replies
}

async fn trial(case: &Value, trial_number: usize) -> Value {
    let scenario = case["id"].as_str().unwrap();
    let (directory, mut state, model, library) = setup(script(scenario));
    let id = start(&state, &library).await;
    let mut seed = state.database.review_claim(&id).unwrap();
    seed.control.policy.retry_delay_ms = 1;
    if scenario == "token_limit" {
        seed.control.policy.max_token_charge = 1;
    }
    if scenario == "tool_timeout" {
        // This limit also applies to healthy SQLite-backed tools. Leave time for
        // them under concurrent test load; the injected search never resolves.
        seed.control.policy.tool_timeout_ms = 500;
    }
    if scenario == "context_compaction" {
        for n in 0..30 {
            seed.messages
                .push(say(&format!("旧讲解 {n}：{}", "背景材料。".repeat(100))));
            seed.messages
                .push(json!({"role":"user","content":format!("旧问题 {n}")}));
        }
    }
    let original_messages = seed.messages.clone();
    seed.state = "paused".into();
    state.database.review_save(&seed, None).unwrap();
    let faults = FaultLibrary {
        base: &library,
        database: state.database.clone(),
        run_id: id.clone(),
        fault: scenario.into(),
        searches: AtomicUsize::new(0),
    };
    let timer = Instant::now();
    let mut result = continue_inner(id.clone(), &state, &faults).await.unwrap();
    if result["state"] == "waiting_answer" {
        state.database.review_answer(&id, "q1", 0).unwrap();
        result = continue_inner(id.clone(), &state, &faults).await.unwrap();
    }
    if scenario == "checkpoint_redelivery" {
        assert_eq!(result["state"], "failed");
        // Simulate redelivery after the grade transaction committed, then a crash
        // with a pending tool. Reopen the database before running the recovery path.
        let mut interrupted = state.database.review_claim(&id).unwrap();
        let call = ToolCall {
            id: "redelivery".into(),
            name: "record_quiz_result".into(),
            arguments: json!({"question_id":"q1"}).to_string(),
        };
        interrupted.messages.push(tools(vec![(
            "redelivery",
            "record_quiz_result",
            json!({"question_id":"q1"}),
        )]));
        interrupted.pending.push(call);
        interrupted.trace_begin(
            "tool",
            "record_quiz_result",
            json!({"call_id":"redelivery"}),
        );
        state.database.review_save(&interrupted, None).unwrap();
        state.database = Database::new(directory.path().join("test.db"));
        state.database.initialize().unwrap();
        state.database.review_recover().unwrap();
        state.database.review_answer(&id, "q1", 0).unwrap(); // duplicate user delivery
        result = continue_inner(id.clone(), &state, &faults).await.unwrap();
    }
    let saved = state.database.review_load(&id).unwrap();
    let events: usize = state
        .database
        .connect()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM review_memory_events WHERE run_id=?",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(result["state"], case["expected_state"], "{scenario}");
    assert_eq!(json!(events), case["expected_grade_events"], "{scenario}");
    assert_eq!(
        &saved.messages[..original_messages.len()],
        &original_messages
    );
    if result["state"] == "completed" {
        assert_eq!(result["completion"]["outcome"], "completed");
        assert_eq!(result["completion"]["content_quality"], "not_assessed");
    }
    match scenario {
        "transient_retry" => assert!(saved.trace.iter().any(|e| e.name == "retry_scheduled")),
        "no_progress" => assert_eq!(result["stop_reason"]["code"], "no_progress"),
        "token_limit" => assert!(model.requests.lock().unwrap().is_empty()),
        "context_compaction" => {
            let report = saved.context.last_report.as_ref().unwrap();
            assert!(report.compacted && report.input_units < report.full_input_units);
        }
        "premature_finish" => assert_eq!(saved.completion.repair_attempts, 1),
        "tool_timeout" => assert!(saved
            .trace
            .iter()
            .any(|e| e.name == "search_textbook" && e.status == "failed")),
        "checkpoint_redelivery" => {
            assert!(saved.trace.iter().any(|e| e.name == "recovered"));
            assert_eq!(
                state
                    .database
                    .review_memory_overview(&saved.scope, chrono::Utc::now())
                    .unwrap()["items"][0]["attempts"],
                1
            );
        }
        "inflight_pause" => {
            assert_eq!(result["stop_reason"]["code"], "user_pause");
            assert_eq!(saved.pending.len(), 1);
        }
        _ => unreachable!(),
    }
    json!({"case_id":scenario,"trial":trial_number,"passed":true,"expected_state":case["expected_state"],
        "state":saved.state,"grade_events":events,"elapsed_ms":timer.elapsed().as_millis(),
        "trace":saved.trace_document()})
}

#[test]
fn harness_fault_injection_suite() {
    let definitions = include_str!("../../../evals/harness/cases.json");
    let dataset: Value = serde_json::from_str(definitions).unwrap();
    let mut results = vec![];
    for case in dataset["cases"].as_array().unwrap() {
        for n in 1..=dataset["trials_per_case"].as_u64().unwrap() {
            // Finish the report even when one isolated trial fails. Assertion
            // details remain in test stderr rather than copying payloads into JSON.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tauri::async_runtime::block_on(trial(case, n as usize))
            }));
            results.push(result.unwrap_or_else(|_| json!({
                    "case_id":case["id"],"trial":n,"passed":false,
                    "expected_state":case["expected_state"],"error":"assertion_failed_see_test_output"
                })));
        }
    }
    let passed = results.iter().filter(|r| r["passed"] == true).count();
    let report = json!({"schema_version":"review-harness-report-v1","harness_version":super::VERSION,
            "generated_at":chrono::Utc::now().to_rfc3339(),
            "dataset_version":dataset["version"],"dataset_sha256":format!("{:x}",Sha256::digest(definitions.as_bytes())),
            "prompt_version":crate::review_agent::REVIEW_PROMPT_VERSION,"mode":dataset["mode"],"model":"scripted-v1",
            "cases":dataset["cases"].as_array().unwrap().len(),"trials":results.len(),"passed":passed,
            "live_api_requests":0,"content_quality":"not_assessed","results":results});
    if let Some(output) = std::env::var_os("TELLWHY_HARNESS_OUTPUT") {
        let directory = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!(
            "Harness report: {}",
            directory.join("report.json").display()
        );
    }
    println!(
        "Harness fault injection: {} / {} trials passed (scripted, no live API).",
        report["passed"], report["trials"]
    );
    assert_eq!(
        report["passed"], report["trials"],
        "Harness fault regression failed"
    );
}
