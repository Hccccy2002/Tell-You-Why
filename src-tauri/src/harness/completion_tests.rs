use super::completion::{self, Outcome};
use crate::{
    review_commands::{continue_inner, start_inner, StartReview},
    review_tests::{quiz, say, setup, start, tools},
};
use serde_json::json;

#[test]
fn harness_completion_premature_finish_is_repaired_then_stopped_with_a_bound() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![
            say("已经完成。"),
            say("已完成所有复习。"),
            say("完成了。"),
        ]);
        let id = start(&state, &library).await;
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "stopped");
        assert_eq!(result["stop_reason"]["code"], "completion_repair_limit");
        assert_eq!(result["completion"]["outcome"], "needs_repair");
        assert_eq!(result["completion"]["content_quality"], "not_assessed");
        let run = state.database.review_load(&id).unwrap();
        assert_eq!(run.completion.repair_attempts, 2);
        assert_eq!(model.requests.lock().unwrap().len(), 3);
        assert!(
            run.output.is_empty(),
            "unverified completion text must not become the accepted output"
        );
        assert!(run.questions.is_empty());
    });
}

#[test]
fn harness_completion_requires_requested_count_and_actual_committed_answers() {
    tauri::async_runtime::block_on(async {
        let mut second = quiz(json!([]));
        second["question"] = json!("存储器中能否同时存放程序与数据？");
        second["topic"] = json!("存储器用途");
        let (_dir, state, _, library) = setup(vec![
            tools(vec![("save1", "save_review_question", quiz(json!([])))]),
            say("请回答第一题。"),
            tools(vec![(
                "grade1",
                "record_quiz_result",
                json!({"question_id":"q1"}),
            )]),
            say("复习完成。"),
            tools(vec![("save2", "save_review_question", second)]),
            say("请回答第二题。"),
            tools(vec![(
                "grade2",
                "record_quiz_result",
                json!({"question_id":"q2"}),
            )]),
            say("两道题都已完成。"),
        ]);
        let id = start_inner(
            StartReview {
                kb: "book".into(),
                version: "v1".into(),
                chapter: Some("chapter1".into()),
                goal: "复习两道题".into(),
                provider: "deepseek".into(),
                region: "default".into(),
                due_only: false,
                question_count: Some(2),
                require_sources: false,
                mcp_server_ids: vec![],
            },
            &state,
            &library,
        )
        .await
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let waiting = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(waiting["completion"]["outcome"], "awaiting_answer");
        state.database.review_answer(&id, "q1", 0).unwrap();
        let second = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(second["state"], "waiting_answer");
        assert_eq!(second["questions"].as_array().unwrap().len(), 2);
        state.database.review_answer(&id, "q2", 1).unwrap();
        let completed = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(completed["state"], "completed");
        assert_eq!(completed["completion"]["verified_submissions"], 2);
        assert_eq!(
            state
                .database
                .review_load(&id)
                .unwrap()
                .completion
                .repair_attempts,
            1
        );
        assert!(completed["completion"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["status"] == "passed"));
        assert_eq!(completed["completion"]["content_quality"], "not_assessed");
    });
}

#[test]
fn harness_completion_strict_sources_rejects_unsupported_quiz_before_it_is_saved() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![
            tools(vec![(
                "unsupported",
                "save_review_question",
                quiz(json!([])),
            )]),
            tools(vec![(
                "search",
                "search_textbook",
                json!({"query":"存储器","mode":"keyword"}),
            )]),
            tools(vec![(
                "supported",
                "save_review_question",
                quiz(json!(["S1"])),
            )]),
            say("请作答。"),
        ]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        run.completion.contract.require_sources = true;
        run.state = "paused".into();
        state.database.review_save(&run, None).unwrap();
        let result = continue_inner(id.clone(), &state, &library).await.unwrap();
        assert_eq!(result["state"], "waiting_answer");
        assert_eq!(result["questions"].as_array().unwrap().len(), 1);
        assert_eq!(result["questions"][0]["source_ids"], json!(["S1"]));
        let run = state.database.review_load(&id).unwrap();
        assert!(run
            .messages
            .iter()
            .any(|m| m["tool_call_id"] == "unsupported"
                && m["content"].as_str().unwrap().contains("false")));
    });
}

#[test]
fn harness_completion_detects_missing_grade_transaction_and_fabricated_submission() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![
            tools(vec![("save", "save_review_question", quiz(json!([])))]),
            say("请作答。"),
            tools(vec![(
                "grade",
                "record_quiz_result",
                json!({"question_id":"q1"}),
            )]),
            say("完成。"),
        ]);
        let id = start(&state, &library).await;
        continue_inner(id.clone(), &state, &library).await.unwrap();
        let mut forged = state.database.review_load(&id).unwrap();
        forged.questions[0].selected_index = Some(0);
        forged.questions[0].correct = Some(true);
        assert_eq!(
            completion::validate(&state.database, &forged)
                .unwrap()
                .report
                .outcome,
            Outcome::Rejected
        );
        state.database.review_answer(&id, "q1", 0).unwrap();
        continue_inner(id.clone(), &state, &library).await.unwrap();
        let run = state.database.review_load(&id).unwrap();
        assert_eq!(
            completion::validate(&state.database, &run)
                .unwrap()
                .report
                .outcome,
            Outcome::Completed
        );
        state
            .database
            .connect()
            .unwrap()
            .execute("DELETE FROM review_memory_events WHERE run_id=?", [&id])
            .unwrap();
        let report = completion::validate(&state.database, &run).unwrap().report;
        assert_eq!(report.outcome, Outcome::Rejected);
        assert!(report
            .checks
            .iter()
            .any(|c| c.code == "grade_transaction" && c.status == "failed"));
    });
}

#[test]
fn harness_completion_invalid_contract_is_rejected_before_starting_a_task() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, model, library) = setup(vec![]);
        for (count, due) in [(0, false), (4, false), (2, true)] {
            let result = start_inner(
                StartReview {
                    kb: "book".into(),
                    version: "v1".into(),
                    chapter: None,
                    goal: "复习".into(),
                    provider: "deepseek".into(),
                    region: "default".into(),
                    due_only: due,
                    question_count: Some(count),
                    require_sources: false,
                    mcp_server_ids: vec![],
                },
                &state,
                &library,
            )
            .await;
            assert!(result.is_err());
        }
        assert!(model.requests.lock().unwrap().is_empty());
        assert!(library.requests.lock().unwrap().is_empty());
        assert!(state.database.review_latest("book").unwrap().is_none());
    });
}
