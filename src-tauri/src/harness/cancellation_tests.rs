#![cfg(windows)]
use super::{
    execution::ExecutionControl,
    process,
    process_tests::{exited, process_handle, python, tree_script, wait_marker},
    tools::ToolError,
};
use crate::{
    db::Database,
    review_agent::ReviewLibrary,
    review_commands::continue_inner,
    review_tests::{quiz, say, setup, start, tools, Library},
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

struct HangingLibrary {
    base: Arc<Library>,
    marker: PathBuf,
    operation: &'static str,
}
#[async_trait]
impl ReviewLibrary for HangingLibrary {
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.base.call(request).await
    }
    async fn call_controlled(
        &self,
        request: Value,
        control: ExecutionControl,
    ) -> Result<Value, ToolError> {
        if request["op"] == self.operation {
            process::execute(
                process::python_command(&python(), &tree_script(&self.marker, false)),
                vec![],
                control,
            )
            .await?;
        }
        self.base.call(request).await.map_err(ToolError::dependency)
    }
}

#[test]
fn cancelling_real_python_from_review_waits_for_cleanup_then_resumes_pending_work() {
    tauri::async_runtime::block_on(async {
        for operation in ["learning_version", "evidence"] {
            let (directory, state, model, base) = setup(vec![
                tools(vec![(
                    "search",
                    "search_textbook",
                    json!({"query":"存储器","mode":"keyword"}),
                )]),
                tools(vec![("save", "save_review_question", quiz(json!(["S1"])))]),
                say("请作答。"),
            ]);
            let id = start(&state, &base).await;
            let state = Arc::new(state);
            let base = Arc::new(base);
            let marker = directory.path().join("active.json");
            let library = HangingLibrary {
                base: base.clone(),
                marker: marker.clone(),
                operation,
            };
            let active = state.clone();
            let active_id = id.clone();
            let task =
                tokio::spawn(async move { continue_inner(active_id, &active, &library).await });
            let pids = wait_marker(&marker).await;
            let parent = process_handle(pids["parent"].as_u64().unwrap() as u32);
            let child = process_handle(pids["child"].as_u64().unwrap() as u32);
            let cancelled = Instant::now();
            state.database.review_cancel(&id).unwrap();
            let result = task.await.unwrap().unwrap();
            assert_eq!(result["state"], "paused");
            assert_eq!(result["stop_reason"]["code"], "user_pause");
            assert!(cancelled.elapsed() < Duration::from_secs(3));
            assert!(exited(&parent) && exited(&child));
            let saved = state.database.review_load(&id).unwrap();
            assert!(saved.sources.is_empty() && saved.questions.is_empty());
            assert_eq!(saved.pending.len(), usize::from(operation == "evidence"));
            assert_eq!(
                model.requests.lock().unwrap().len(),
                usize::from(operation == "evidence")
            );
            assert!(saved
                .trace
                .iter()
                .any(|e| e.details["process_cleanup"] == "confirmed"));
            assert_eq!(
                continue_inner(id, &state, base.as_ref()).await.unwrap()["state"],
                "waiting_answer"
            );
        }
    });
}

struct LateResult {
    base: Library,
    db: Database,
    id: String,
}
#[async_trait]
impl ReviewLibrary for LateResult {
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.base.call(request).await
    }
    async fn call_controlled(
        &self,
        request: Value,
        control: ExecutionControl,
    ) -> Result<Value, ToolError> {
        let result = self
            .base
            .call(request.clone())
            .await
            .map_err(ToolError::dependency)?;
        if request["op"] == "evidence" {
            // The OS operation has completed, but a pause arrives before the result
            // is handed to the domain handler and its checkpoint is committed.
            process::execute(
                process::python_command(&python(), "print('done',flush=True)"),
                vec![],
                control,
            )
            .await?;
            self.db.review_cancel(&self.id).unwrap();
        }
        Ok(result)
    }
}

#[test]
fn cancelled_late_tool_result_cannot_publish_sources_or_trigger_the_next_model_call() {
    tauri::async_runtime::block_on(async {
        let (_directory, state, model, base) = setup(vec![tools(vec![(
            "search",
            "search_textbook",
            json!({"query":"存储器","mode":"keyword"}),
        )])]);
        let id = start(&state, &base).await;
        let library = LateResult {
            base,
            db: state.database.clone(),
            id: id.clone(),
        };
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "paused");
        assert_eq!(model.requests.lock().unwrap().len(), 1);
        let saved = state.database.review_load(&id).unwrap();
        assert!(saved.sources.is_empty() && saved.questions.is_empty());
        assert_eq!(saved.pending.len(), 1);
        assert!(!saved.messages.iter().any(|m| m["tool_call_id"] == "search"));
        let events: usize = state
            .database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_memory_events WHERE run_id=?",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(events, 0);
    });
}
