use super::tools::{self, Access, ErrorCode, Permissions, ToolError};
use crate::{
    review_agent::{ReviewLibrary, ToolCall},
    review_commands::continue_inner,
    review_tests::{quiz, say, setup, start, tools as calls, Library},
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "tool-test".into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

struct FailingLibrary<'a> {
    base: &'a Library,
    code: ErrorCode,
    attempts: AtomicUsize,
    always: bool,
}
#[async_trait]
impl ReviewLibrary for FailingLibrary<'_> {
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.base.call(request).await
    }
    async fn call_tool(&self, request: Value) -> Result<Value, ToolError> {
        if request["op"] != "evidence" {
            return self.base.call(request).await.map_err(ToolError::dependency);
        }
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 || self.always {
            Err(ToolError::new(self.code, "受控工具故障"))
        } else {
            self.base.call(request).await.map_err(ToolError::dependency)
        }
    }
}

#[test]
fn tools_validate_contracts_and_permissions_before_domain_execution() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        let initial = library.requests.lock().unwrap().len();
        for invalid in [
            call("shell", json!({})),
            call(
                "search_textbook",
                json!({"query":"存储器","mode":"keyword","kb":"other"}),
            ),
            call("search_textbook", json!({"query":"  ","mode":"keyword"})),
            call("read_source", json!({"source_id":"S1","max_chars":2001})),
            call(
                "record_quiz_result",
                json!({"question_id":"q1","selected":0}),
            ),
        ] {
            assert!(
                tools::execute(&state.database, &library, &mut run, &invalid)
                    .await
                    .is_err()
            );
        }
        run.control.tool_permissions = Permissions {
            read: false,
            write_review: false,
        };
        for denied in [
            call("get_learning_progress", json!({})),
            call("save_review_question", quiz(json!([]))),
        ] {
            assert_eq!(
                tools::execute(&state.database, &library, &mut run, &denied)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::PermissionDenied
            );
        }
        assert!(run.questions.is_empty() && run.sources.is_empty());
        assert_eq!(library.requests.lock().unwrap().len(), initial);
        assert!(state
            .database
            .review_load(&id)
            .unwrap()
            .questions
            .is_empty());
        run.state = "paused".into();
        state.database.review_save(&run, None).unwrap();
        assert_eq!(
            continue_inner(id, &state, &library).await.unwrap()["stop_reason"]["code"],
            "permission_denied"
        );
        assert_eq!(library.requests.lock().unwrap().len(), initial);
        assert_eq!(tools::registry().len(), 5);
        for spec in tools::registry()
            .into_iter()
            .filter(|s| s.access == Access::WriteReview)
        {
            assert!(!spec.retry(&ToolError::new(ErrorCode::Timeout, "timeout"), 0));
        }
    });
}

#[test]
fn tools_retry_only_transient_reads_and_report_each_attempt() {
    tauri::async_runtime::block_on(async {
        for (code, always, expected) in [
            (ErrorCode::DependencyUnavailable, false, 2),
            (ErrorCode::DependencyUnavailable, true, 2),
            (ErrorCode::InvalidResult, true, 1),
        ] {
            let (_dir, state, model, base) = setup(vec![
                calls(vec![(
                    "search",
                    "search_textbook",
                    json!({"query":"存储器","mode":"keyword"}),
                )]),
                calls(vec![("save", "save_review_question", quiz(json!([])))]),
                say("请作答。"),
            ]);
            let id = start(&state, &base).await;
            let library = FailingLibrary {
                base: &base,
                code,
                attempts: AtomicUsize::new(0),
                always,
            };
            let result = continue_inner(id.clone(), &state, &library).await.unwrap();
            assert_eq!(result["state"], "waiting_answer");
            assert_eq!(library.attempts.load(Ordering::SeqCst), expected);
            assert_eq!(model.requests.lock().unwrap().len(), 3);
            let saved = state.database.review_load(&id).unwrap();
            assert_eq!(saved.control.tool_attempts["search"], expected);
            assert_eq!(
                saved
                    .messages
                    .iter()
                    .filter(|m| m["tool_call_id"] == "search")
                    .count(),
                1
            );
            let events: Vec<_> = saved
                .trace
                .iter()
                .filter(|e| e.name == "search_textbook")
                .collect();
            assert_eq!(events.len(), expected);
            assert_eq!(events[0].details["result"]["error_code"], json!(code));
        }
    });
}

#[test]
fn tools_retries_share_the_task_budget_and_keep_pending_work_on_stop() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, base) = setup(vec![calls(vec![(
            "search",
            "search_textbook",
            json!({"query":"存储器","mode":"keyword"}),
        )])]);
        let id = start(&state, &base).await;
        let mut configured = state.database.review_claim(&id).unwrap();
        configured.control.policy.max_tool_calls = 1;
        configured.state = "paused".into();
        state.database.review_save(&configured, None).unwrap();
        let library = FailingLibrary {
            base: &base,
            code: ErrorCode::DependencyUnavailable,
            attempts: AtomicUsize::new(0),
            always: true,
        };
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["stop_reason"]["code"], "tool_call_limit");
        assert_eq!(library.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(state.database.review_load(&id).unwrap().pending.len(), 1);
    });
}

struct MixedScope;
#[async_trait]
impl ReviewLibrary for MixedScope {
    async fn call(&self, _: Value) -> Result<Value, String> {
        Ok(json!({"kb":"book","version":"v1","evidence":[
            {"block_id":"b1","page":1,"text":"合法资料","chapter_path":["存储器"]},
            {"block_id":"b2","page":2,"text":"越界资料","chapter_path":["other"]}]}))
    }
}

#[test]
fn tools_failed_result_does_not_publish_partial_state() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, base) = setup(vec![]);
        let id = start(&state, &base).await;
        let mut run = state.database.review_claim(&id).unwrap();
        let error = tools::execute(
            &state.database,
            &MixedScope,
            &mut run,
            &call("search_textbook", json!({"query":"x","mode":"keyword"})),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ScopeMismatch);
        assert!(run.sources.is_empty());
        assert_eq!(
            crate::review_trace::tool_result("search_textbook", &error.envelope())["error_code"],
            "scope_mismatch"
        );
    });
}
