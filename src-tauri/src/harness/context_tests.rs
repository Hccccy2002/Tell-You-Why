use super::context;
use crate::{
    review_agent::{execute_tool, tool_definitions, ReviewRun, ToolCall},
    review_commands::continue_inner,
    review_tests::{quiz, say, setup, start, tools},
};
use serde_json::{json, Value};

fn add_history(run: &mut ReviewRun, count: usize) {
    for n in 0..count {
        run.messages
            .push(say(&format!("旧讲解 {n} {}", "资料背景。".repeat(80))));
        run.messages
            .push(json!({"role":"user","content":format!("旧问题 {n}")}));
    }
}

#[test]
fn harness_context_compacts_without_overwriting_history_or_submitted_state() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        add_history(&mut run, 30);
        execute_tool(
            &state.database,
            &library,
            &mut run,
            &ToolCall {
                id: "save".into(),
                name: "save_review_question".into(),
                arguments: quiz(json!([])).to_string(),
            },
        )
        .await
        .unwrap();
        run.questions[0].selected_index = Some(1);
        run.questions[0].correct = Some(false);
        run.due_memory_ids = vec!["due-123".into()];
        let before = run.messages.clone();
        let built = context::build(&run, &tool_definitions()).unwrap();
        assert!(built.report.compacted);
        assert!(built.report.input_units < built.report.full_input_units);
        assert!(built.report.input_units <= built.report.max_input_units);
        assert_eq!(run.messages, before);
        let pinned: Value =
            serde_json::from_str(built.messages[1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(pinned["questions"][0]["selected_index"], 1);
        assert_eq!(pinned["questions"][0]["recorded_correct"], false);
        assert_eq!(pinned["due_memory_ids"], json!(["due-123"]));
        assert_eq!(pinned["scope"]["version"], "v1");
    });
}

#[test]
fn harness_context_keeps_tool_batches_whole_and_excludes_hidden_reasoning() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        add_history(&mut run, 15);
        let mut batch = tools(vec![
            ("a", "get_learning_progress", json!({})),
            ("b", "read_source", json!({"source_id":"S1"})),
        ]);
        batch["reasoning_content"] =
            json!("hidden reasoning must remain only in the original record");
        run.messages.push(batch);
        for id in ["a", "b"] {
            run.messages
                .push(json!({"role":"tool","tool_call_id":id,"content":"{\"ok\":true}"}));
        }
        run.context.policy.recent_groups = 1;
        let built = context::build(&run, &tool_definitions()).unwrap();
        assert_eq!(built.report.retained_history_messages, 3);
        assert_eq!(built.messages[2]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(built.messages[3]["tool_call_id"], "a");
        assert_eq!(built.messages[4]["tool_call_id"], "b");
        assert!(!json!(built.messages)
            .to_string()
            .contains("hidden reasoning"));
        assert!(json!(run.messages).to_string().contains("hidden reasoning"));
    });
}

#[test]
fn harness_context_rejects_orphan_results_and_oversized_required_groups() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        run.messages
            .push(json!({"role":"tool","tool_call_id":"orphan","content":"{}"}));
        assert_eq!(
            context::build(&run, &tool_definitions())
                .err()
                .unwrap()
                .code,
            "invalid_transcript"
        );
        run.messages.pop();
        run.messages.push(tools(vec![(
            "read",
            "read_source",
            json!({"source_id":"S1"}),
        )]));
        run.messages
            .push(json!({"role":"tool","tool_call_id":"read","content":"大段材料".repeat(10000)}));
        run.state = "paused".into();
        state.database.review_save(&run, None).unwrap();
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "stopped");
        assert_eq!(result["stop_reason"]["code"], "context_limit");
        assert!(model.requests.lock().unwrap().is_empty());
        assert_eq!(
            state.database.review_load(&id).unwrap().messages,
            run.messages
        );
    });
}

#[test]
fn harness_context_paginated_source_can_reach_unicode_tail_without_changing_evidence() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        let text = format!("{}尾部证据🙂", "汉".repeat(2200));
        run.sources
            .push(json!({"id":"S1","text":text,"page":3,"chapter_path":["存储器"]}));
        let originals = run.sources.clone();
        let read = |start| ToolCall {
            id: "read".into(),
            name: "read_source".into(),
            arguments: json!({"source_id":"S1","start_char":start,"max_chars":2000}).to_string(),
        };
        let first = execute_tool(&state.database, &library, &mut run, &read(0))
            .await
            .unwrap()
            .0;
        assert_eq!(first["has_more"], true);
        let tail = execute_tool(&state.database, &library, &mut run, &read(2000))
            .await
            .unwrap()
            .0;
        assert_eq!(tail["has_more"], false);
        assert_eq!(
            format!(
                "{}{}",
                first["text"].as_str().unwrap(),
                tail["text"].as_str().unwrap()
            ),
            text
        );
        assert_eq!(run.sources, originals);
        assert!(
            execute_tool(&state.database, &library, &mut run, &read(99999))
                .await
                .is_err()
        );
    });
}

#[test]
fn harness_context_resume_uses_compact_view_and_keeps_full_durable_transcript() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![
            tools(vec![("save", "save_review_question", quiz(json!([])))]),
            say("请作答。"),
        ]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        add_history(&mut run, 25);
        run.state = "paused".into();
        state.database.review_save(&run, None).unwrap();
        let original = run.messages.clone();
        assert_eq!(
            continue_inner(id.clone(), &state, &library).await.unwrap()["state"],
            "waiting_answer"
        );
        let saved = state.database.review_load(&id).unwrap();
        assert_eq!(&saved.messages[..original.len()], &original);
        assert!(
            model.requests.lock().unwrap()[0]["messages"]
                .as_array()
                .unwrap()
                .len()
                < original.len()
        );
        assert!(saved.context.last_report.as_ref().unwrap().compacted);
        let mut legacy = serde_json::to_value(saved).unwrap();
        legacy.as_object_mut().unwrap().remove("context");
        let restored: ReviewRun = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.context.policy.max_input_units, 24_000);
    });
}
