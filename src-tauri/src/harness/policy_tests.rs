use super::policy::{self, RunControl};
use crate::{
    providers::{ProviderError, ProviderTransport, TransportResponse},
    review_agent::ReviewRun,
    review_commands::continue_inner,
    review_tests::{quiz, say, setup, start, tools},
    secret_store::SecretValue,
};
use async_trait::async_trait;
use serde_json::json;
use std::{sync::Arc, time::Duration};

fn configure(state: &crate::AppState, id: &str, change: impl FnOnce(&mut RunControl)) {
    let mut run = state.database.review_claim(id).unwrap();
    change(&mut run.control);
    run.state = "paused".into();
    state.database.review_save(&run, None).unwrap();
}

#[test]
fn harness_policy_limits_stop_before_spending_and_cannot_be_resumed() {
    tauri::async_runtime::block_on(async {
        for code in ["model_call_limit", "token_limit", "active_time_limit"] {
            let (_dir, state, model, library) = setup(vec![]);
            let id = start(&state, &library).await;
            configure(&state, &id, |c| match code {
                "model_call_limit" => c.policy.max_model_calls = 0,
                "token_limit" => c.policy.max_token_charge = 1,
                _ => c.policy.max_active_ms = 0,
            });
            let stopped = continue_inner(id.clone(), &state, &library).await.unwrap();
            assert_eq!(stopped["state"], "stopped");
            assert_eq!(stopped["stop_reason"]["code"], code);
            assert_eq!(stopped["can_resume"], false);
            assert_eq!(
                continue_inner(id.clone(), &state, &library).await.unwrap(),
                stopped
            );
            assert!(state.database.review_claim(&id).is_err());
            assert!(model.requests.lock().unwrap().is_empty());
        }
    });
}

#[test]
fn harness_policy_retries_transient_failure_once_and_preserves_usage_uncertainty() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![
            json!("NETWORK_ERROR"),
            tools(vec![("save", "save_review_question", quiz(json!([])))]),
            say("请作答。"),
        ]);
        let id = start(&state, &library).await;
        configure(&state, &id, |c| c.policy.retry_delay_ms = 1);
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "waiting_answer");
        let run = state.database.review_load(&id).unwrap();
        assert_eq!(model.requests.lock().unwrap().len(), 3);
        assert_eq!(run.trace_document()["reported_total_tokens"], 60);
        assert!(
            run.control.charged_tokens > 60,
            "unknown failed request must retain its reservation"
        );
        assert_eq!(
            run.trace
                .iter()
                .filter(|e| e.name == "retry_scheduled")
                .count(),
            1
        );
        assert_eq!(run.questions.len(), 1);
    });
}

struct InvalidKey;
#[async_trait]
impl ProviderTransport for InvalidKey {
    async fn post_json(
        &self,
        _: &url::Url,
        _: &SecretValue,
        _: &serde_json::Value,
        _: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        Err(ProviderError::InvalidKey)
    }
}

#[test]
fn harness_policy_does_not_retry_permanent_provider_errors() {
    tauri::async_runtime::block_on(async {
        let (_dir, mut state, _, library) = setup(vec![]);
        state.http = Arc::new(InvalidKey);
        let id = start(&state, &library).await;
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "stopped");
        assert_eq!(result["stop_reason"]["code"], "invalid_key");
        assert_eq!(result["model_calls"], 1);
        assert!(!state
            .database
            .review_load(&id)
            .unwrap()
            .trace
            .iter()
            .any(|e| e.name == "retry_scheduled"));
    });
}

#[test]
fn harness_policy_detects_repeated_observations_despite_different_call_ids() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(
            ["a", "b", "c"]
                .map(|id| {
                    tools(vec![(
                        id,
                        "search_textbook",
                        json!({"query":"存储器","mode":"keyword"}),
                    )])
                })
                .to_vec(),
        );
        let id = start(&state, &library).await;
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "stopped");
        assert_eq!(result["stop_reason"]["code"], "no_progress");
        assert_eq!(result["tool_calls"], 3);
        assert_eq!(model.requests.lock().unwrap().len(), 3);
        assert!(state.database.review_load(&id).unwrap().pending.is_empty());
    });
}

#[test]
fn harness_policy_timeout_and_midflight_pause_do_not_restart_the_future() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        state.database.review_claim(&id).unwrap();
        let timed = policy::guarded(
            &state.database,
            &id,
            10,
            false,
            std::future::pending::<()>(),
        )
        .await;
        assert_eq!(timed.unwrap_err().code, "operation_timeout");
        let db = state.database.clone();
        let cancel_id = id.clone();
        let cancel = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            db.review_cancel(&cancel_id).unwrap();
        });
        let stopped = policy::guarded(
            &state.database,
            &id,
            2_000,
            false,
            std::future::pending::<()>(),
        )
        .await;
        assert_eq!(stopped.unwrap_err().code, "user_pause");
        cancel.await.unwrap();
    });
}

#[test]
fn harness_policy_old_runs_load_and_human_wait_does_not_consume_active_budget() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![
            tools(vec![("save", "save_review_question", quiz(json!([])))]),
            say("请作答。"),
        ]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        run.created_at = "2000-01-01T00:00:00Z".into();
        let mut old = serde_json::to_value(&run).unwrap();
        old.as_object_mut().unwrap().remove("control");
        let mut restored: ReviewRun = serde_json::from_value(old).unwrap();
        assert_eq!(restored.control.charged_active_ms, 0);
        restored.state = "paused".into();
        state.database.review_save(&restored, None).unwrap();
        assert_eq!(
            continue_inner(id.clone(), &state, &library).await.unwrap()["state"],
            "waiting_answer"
        );
        let waiting = state.database.review_load(&id).unwrap();
        continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(
            waiting.control.charged_active_ms,
            state
                .database
                .review_load(&id)
                .unwrap()
                .control
                .charged_active_ms
        );
    });
}
