use crate::{
    review_agent::{execute_tool, ReviewQuestion, ReviewRun, ToolCall},
    review_memory::{next_interval_seconds, save_grade},
    review_tests::{setup, start},
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};

fn quiz(correct: bool) -> ReviewQuestion {
    ReviewQuestion {
        id: "q1".into(),
        topic: "存储器".into(),
        question: "存储器保存什么？".into(),
        options: vec!["程序和数据".into(), "只有图片".into()],
        correct_index: 0,
        explanation: "保存程序和数据。".into(),
        source_ids: vec![],
        card_id: None,
        memory_id: None,
        selected_index: Some(if correct { 0 } else { 1 }),
        correct: Some(correct),
    }
}
fn overview(state: &crate::AppState, run: &ReviewRun, seconds: i64) -> Value {
    state
        .database
        .review_memory_overview(&run.scope, Utc.timestamp_opt(seconds, 0).unwrap())
        .unwrap()
}
fn grade_at(state: &crate::AppState, run: &ReviewRun, q: &ReviewQuestion, seconds: i64) -> bool {
    let mut conn = state.database.connect().unwrap();
    let tx = conn.transaction().unwrap();
    let inserted = save_grade(&tx, run, q, seconds).unwrap();
    tx.commit().unwrap();
    inserted
}

#[test]
fn schedule_is_bounded_and_wrong_answer_resets_interval() {
    assert_eq!(next_interval_seconds(false, 50), 600);
    assert_eq!(
        (1..=7)
            .map(|s| next_interval_seconds(true, s) / 86400)
            .collect::<Vec<_>>(),
        vec![1, 3, 7, 14, 30, 60, 60]
    );
}

#[test]
fn grading_persists_and_replay_does_not_increment_or_reschedule() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_load(&id).unwrap();
        let mut q = quiz(false);
        assert!(grade_at(&state, &run, &q, 1000));
        assert!(!grade_at(&state, &run, &q, 2000));
        let memory = overview(&state, &run, 1600);
        assert_eq!(memory["due_count"], 1);
        assert_eq!(memory["items"][0]["attempts"], 1);
        assert_eq!(overview(&state, &run, 1599)["due_count"], 0);
        q.memory_id = Some(memory["items"][0]["id"].as_str().unwrap().into());
        q.topic = "存储器用途（换一种问法）".into();
        q.correct = Some(true);
        q.selected_index = Some(0);
        run.id = "second".into();
        grade_at(&state, &run, &q, 2000);
        let after = overview(&state, &run, 2000);
        assert_eq!(after["total"], 1);
        assert_eq!(after["items"][0]["attempts"], 2);
        assert_eq!(after["items"][0]["streak"], 1);
        assert_eq!(after["weak_count"], 0);
        run.id = "third".into();
        grade_at(&state, &run, &q, 3000);
        assert_eq!(overview(&state, &run, 3000)["items"][0]["streak"], 2);
        assert_eq!(overview(&state, &run, 3000 + 3 * 86400)["due_count"], 1);
        q.correct = Some(false);
        q.selected_index = Some(1);
        run.id = "fourth".into();
        grade_at(&state, &run, &q, 4000);
        assert_eq!(overview(&state, &run, 4600)["weak_count"], 1);
        assert_eq!(overview(&state, &run, 4600)["items"][0]["streak"], 0);
    });
}

#[test]
fn memory_scope_includes_version_and_due_priority_is_server_controlled() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_load(&id).unwrap();
        let q = quiz(false);
        grade_at(&state, &run, &q, 1000);
        let memory = overview(&state, &run, 2000);
        let original = run.scope.clone();
        for (kb, version, path) in [
            ("other", "v1", vec![]),
            ("book", "v2", vec![]),
            ("book", "v1", vec!["其他章节".into()]),
        ] {
            run.scope.kb = kb.into();
            run.scope.version = version.into();
            run.scope.chapter_path = path;
            assert_eq!(overview(&state, &run, 2000)["total"], 0);
        }
        run.scope = original;
        run.id = "second".into();
        let mut good = quiz(true);
        good.topic = "其他概念".into();
        grade_at(&state, &run, &good, 0);
        let all = overview(&state, &run, 100000);
        assert_eq!(all["total"], 2);
        assert_eq!(all["items"][0]["id"], memory["items"][0]["id"]);
    });
}

#[test]
fn unsubmitted_or_forged_answer_cannot_write_memory() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_claim(&id).unwrap();
        let mut q = quiz(true);
        run.questions.push(q.clone());
        // A caller cannot invent a submission merely by placing it in a candidate checkpoint.
        assert!(state.database.review_save(&run, Some(&q)).is_err());
        q.selected_index = None;
        let mut conn = state.database.connect().unwrap();
        let tx = conn.transaction().unwrap();
        assert!(save_grade(&tx, &run, &q, 1000).is_err());
        drop(tx);
        assert_eq!(overview(&state, &run, 2000)["total"], 0);
    });
}

#[test]
fn memory_id_must_have_been_read_from_current_scope() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_load(&id).unwrap();
        grade_at(&state, &run, &quiz(false), 0);
        let memory = overview(&state, &run, 1000);
        let memory_id = memory["items"][0]["id"].clone();
        let call=ToolCall{id:"save".into(),name:"save_review_question".into(),arguments:json!({"topic":"存储器","question":"保存什么？","options":["程序和数据","只有图片"],"correct_index":0,"explanation":"保存程序和数据","source_ids":[],"memory_id":memory_id}).to_string()};
        assert!(execute_tool(&state.database, &library, &mut run, &call)
            .await
            .is_err());
        execute_tool(
            &state.database,
            &library,
            &mut run,
            &ToolCall {
                id: "progress".into(),
                name: "get_learning_progress".into(),
                arguments: "{}".into(),
            },
        )
        .await
        .unwrap();
        execute_tool(&state.database, &library, &mut run, &call)
            .await
            .unwrap();
        assert_eq!(run.questions[0].memory_id.as_deref(), memory_id.as_str());
    });
}

#[test]
fn migration_backfills_actual_answers_once_and_ignores_unanswered_questions() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_load(&id).unwrap();
        run.questions.push(quiz(false));
        let mut unanswered = quiz(true);
        unanswered.id = "q2".into();
        unanswered.selected_index = None;
        unanswered.correct = None;
        run.questions.push(unanswered);
        state
            .database
            .connect()
            .unwrap()
            .execute(
                "UPDATE review_runs SET record=? WHERE id=?",
                rusqlite::params![serde_json::to_string(&run).unwrap(), id],
            )
            .unwrap();
        state.database.connect().unwrap().execute_batch("DROP TABLE review_memory_events;DROP TABLE review_memory;DROP TABLE study_doubts;DROP TABLE study_highlights;DROP TABLE study_memory;DROP TABLE study_sessions;DROP TABLE follow_up_search; DROP TABLE search_runs; DROP TABLE search_cache; DROP TABLE search_attempts; DROP TABLE search_options; DROP TABLE search_profiles;DELETE FROM schema_migrations WHERE version>=10;").unwrap();
        state.database.initialize().unwrap();
        state.database.initialize().unwrap();
        let memory = overview(&state, &run, Utc::now().timestamp() + 3600);
        assert_eq!(memory["total"], 1);
        assert_eq!(memory["items"][0]["attempts"], 1);
    });
}

#[test]
fn due_agent_reuses_memory_after_actual_submission_and_updates_the_queue() {
    tauri::async_runtime::block_on(async {
        use crate::review_commands::{continue_inner, start_inner, StartReview};
        use crate::review_tests::{say, tools};
        let (_dir, state, model, library) = setup(vec![]);
        let request = || StartReview {
            question_count: None,
            require_sources: false,
            mcp_server_ids: vec![],
            kb: "book".into(),
            version: "v1".into(),
            chapter: Some("chapter1".into()),
            goal: "复习到期知识点".into(),
            provider: "deepseek".into(),
            region: "default".into(),
            due_only: true,
        };
        assert!(start_inner(request(), &state, &library)
            .await
            .unwrap_err()
            .contains("没有到期"));
        let old_id = start(&state, &library).await;
        let old = state.database.review_load(&old_id).unwrap();
        grade_at(&state, &old, &quiz(false), 0);
        let memory_id = overview(&state, &old, 1000)["items"][0]["id"].clone();
        let new_id = start_inner(request(), &state, &library).await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut pending = state.database.review_load(&new_id).unwrap();
        assert_eq!(pending.due_memory_ids, vec![memory_id.as_str().unwrap()]);
        let bad = ToolCall {
            id: "bad".into(),
            name: "save_review_question".into(),
            arguments: crate::review_tests::quiz(json!([])).to_string(),
        };
        assert_eq!(
            execute_tool(&state.database, &library, &mut pending, &bad)
                .await
                .unwrap_err()
                .code,
            crate::harness::tools::ErrorCode::PreconditionFailed
        );
        let mut question = crate::review_tests::quiz(json!(["S1"]));
        question["memory_id"] = memory_id.clone();
        question["topic"] = json!("存储器用途（再次巩固）");
        model.replies.lock().unwrap().extend([
            tools(vec![
                ("p", "get_learning_progress", json!({})),
                (
                    "s",
                    "search_textbook",
                    json!({"query":"存储器","mode":"keyword"}),
                ),
            ]),
            tools(vec![("r", "read_source", json!({"source_id":"S1"}))]),
            tools(vec![("q", "save_review_question", question)]),
            say("请作答。"),
            tools(vec![(
                "g",
                "record_quiz_result",
                json!({"question_id":"q1"}),
            )]),
            say("本轮完成。"),
        ]);
        let waiting = continue_inner(new_id.clone(), &state, &library)
            .await
            .unwrap();
        assert_eq!(waiting["state"], "waiting_answer");
        assert_eq!(
            state
                .database
                .review_memory_get(memory_id.as_str().unwrap())
                .unwrap()
                .unwrap()
                .attempts,
            1
        );
        state.database.review_answer(&new_id, "q1", 0).unwrap();
        let complete = continue_inner(new_id.clone(), &state, &library)
            .await
            .unwrap();
        assert_eq!(complete["state"], "completed");
        let result = state
            .database
            .review_memory_get(memory_id.as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(result.attempts, 2);
        assert_eq!(result.correct_count, 1);
        assert_eq!(result.streak, 1);
        assert_eq!(result.due_at - result.last_reviewed_at, 86400);
        assert_eq!(
            overview(&state, &old, Utc::now().timestamp())["due_count"],
            0
        );
        let completed_run = state.database.review_load(&new_id).unwrap();
        assert_eq!(
            completed_run
                .trace
                .iter()
                .filter(|e| e.name == "record_quiz_result")
                .count(),
            1
        );
    });
}

#[test]
fn clearing_all_data_removes_memory_but_clearing_reading_history_keeps_it() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let run = state.database.review_load(&id).unwrap();
        grade_at(&state, &run, &quiz(false), 0);
        state.database.clear_data("history").unwrap();
        assert_eq!(overview(&state, &run, 1000)["total"], 1);
        state.database.clear_data("all").unwrap();
        assert_eq!(overview(&state, &run, 1000)["total"], 0);
        let count: i64 = state
            .database
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM review_memory_events", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn progress_and_card_links_reject_a_previous_textbook_version() {
    tauri::async_runtime::block_on(async {
        let (_dir, state, _, library) = setup(vec![]);
        let id = start(&state, &library).await;
        let mut run = state.database.review_load(&id).unwrap();
        run.scope.version = "v2".into();
        assert_eq!(
            state.database.review_progress(&run).unwrap()["cards"],
            json!([])
        );
        let call = ToolCall {
            id: "old".into(),
            name: "save_review_question".into(),
            arguments: crate::review_tests::quiz(json!([])).to_string(),
        };
        assert!(execute_tool(&state.database, &library, &mut run, &call)
            .await
            .is_err());
    });
}
