use crate::{
    db::Database,
    study::{StudyCall, StudySession},
    study_agent, study_commands,
};
use serde_json::{json, Value};

fn plan(count: usize) -> Value {
    json!({"objectives":(1..=count).map(|n|json!({"title":format!("目标{n}"),"criterion":format!("用新情境解释目标{n}")})).collect::<Vec<_>>(),"checkin_prompt":"你最不清楚哪一部分？"})
}
fn lesson(target: usize, kind: &str) -> Value {
    json!({"objective_id":format!("objective-{target}"),"title":format!("讲解目标{target}"),"text":"一段围绕当前目标的讲解。","reason":"你已经掌握了所有目标","kind":kind,"card_id":null})
}
fn quiz(target: usize) -> Value {
    json!({"objective_id":format!("objective-{target}"),"title":"验证目标","question":"下面哪一个情境符合刚才的概念？","reason":"你已全部掌握","options":["符合","不符合"],"correct_index":0,"explanation":"第一种情境符合定义。","card_id":null})
}
fn call(db: &Database, run: &mut StudySession, name: &str, args: Value) -> Result<Value, String> {
    let mut candidate = run.clone();
    let result = study_agent::execute(
        db,
        &mut candidate,
        &StudyCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            arguments: args.to_string(),
        },
    )?;
    db.study_save(&mut candidate).unwrap();
    *run = candidate;
    Ok(result)
}
fn prepare(count: usize) -> (tempfile::TempDir, crate::AppState, StudySession) {
    let (dir, state, _, _) = crate::review_tests::setup(vec![]);
    let created = study_commands::start_goal_inner("分清生成和挥发".into(), &state).unwrap();
    let mut run = state
        .database
        .study_load(created["id"].as_str().unwrap())
        .unwrap();
    call(&state.database, &mut run, "get_learning_context", json!({})).unwrap();
    call(
        &state.database,
        &mut run,
        "search_cards",
        json!({"query":"生成"}),
    )
    .unwrap();
    call(&state.database, &mut run, "plan_learning", plan(count)).unwrap();
    (dir, state, run)
}
fn feedback(db: &Database, run: &mut StudySession, value: &str, selected: Option<usize>) {
    *run = db
        .study_feedback(&run.id, &run.steps.last().unwrap().id, value, selected)
        .unwrap();
}

#[test]
fn goal_plan_is_bounded_immutable_and_cannot_forge_progress() {
    let (_dir, state, mut run) = prepare(3);
    assert_eq!(
        run.public()["goal_plan"]["objectives"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(call(&state.database, &mut run, "plan_learning", plan(1)).is_err());
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept")
    )
    .is_err());
    assert!(call(
        &state.database,
        &mut run,
        "finish_learning",
        json!({"next_topic":"完成"})
    )
    .is_err());
    run.goal_plan = None;
    for invalid in [
        plan(0),
        plan(4),
        json!({"objectives":[{"title":"相同","criterion":"验证"},{"title":" 相同 ","criterion":"验证"}],"checkin_prompt":"你卡在哪"}),
        json!({"objectives":[{"title":"目标","criterion":"验证","verified":true}],"checkin_prompt":"你卡在哪"}),
    ] {
        assert!(call(&state.database, &mut run, "plan_learning", invalid).is_err());
    }
    run.context_read = false;
    assert!(call(&state.database, &mut run, "plan_learning", plan(1)).is_err());
    run.context_read = true;
    run.goal_mode = false;
    assert!(call(&state.database, &mut run, "plan_learning", plan(1)).is_err());
}

#[test]
fn goal_checkin_requires_real_input_or_explicit_skip_and_survives_restart() {
    let (dir, state, run) = prepare(2);
    assert!(state
        .database
        .study_goal_checkin(&run.id, &"字".repeat(301))
        .is_err());
    let saved = state
        .database
        .study_goal_checkin(&run.id, "  我把两种变化混在一起  ")
        .unwrap();
    assert_eq!(saved.state, "ready");
    assert_eq!(saved.public()["summary"]["answered"], 0);
    assert_eq!(
        state
            .database
            .study_goal_checkin(&run.id, "我把两种变化混在一起")
            .unwrap()
            .revision,
        saved.revision
    );
    assert!(state.database.study_goal_checkin(&run.id, "").is_err());
    let db = Database::new(dir.path().join("test.db"));
    db.initialize().unwrap();
    assert_eq!(
        db.study_load(&run.id).unwrap().public()["goal_plan"]["checkin_reply"],
        "我把两种变化混在一起"
    );
    assert_eq!(db.study_memories(&run.topic).unwrap(), json!([]));
}

#[test]
fn goal_lesson_order_adapts_but_remediation_stays_on_target_and_only_once() {
    let (_dir, state, mut run) = prepare(3);
    run = state
        .database
        .study_goal_checkin(&run.id, "我最不清楚第二部分")
        .unwrap();
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(2, "concept"),
    )
    .unwrap();
    assert!(!run.public()["steps"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("掌握"));
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept")
    )
    .is_err());
    feedback(&state.database, &mut run, "confused", None);
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "prerequisite")
    )
    .is_err());
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(2, "deeper")
    )
    .is_err());
    assert!(call(&state.database, &mut run, "offer_quiz", quiz(2)).is_err());
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(2, "example"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "confused", None);
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(2, "example")
    )
    .is_err());
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "understood", None);
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(3, "deeper"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "continue", None);
    let progress = run.public()["goal_plan"].clone();
    assert_eq!(progress["objectives"][0]["self_reported_understood"], true);
    assert_eq!(progress["objectives"][1]["needs_help"], true);
    assert_eq!(progress["objectives"][2]["self_reported_understood"], false);
    assert!(progress["objectives"]
        .as_array()
        .unwrap()
        .iter()
        .all(|o| o["verified"] == false));
    assert_eq!(
        state.database.study_memories(&run.topic).unwrap(),
        json!([])
    );
    call(&state.database, &mut run, "offer_quiz", quiz(2)).unwrap();
    assert!(run.public()["steps"][4]["quiz"]["correct_index"].is_null());
    assert!(run.public()["goal_plan"]["objectives"][1]["correct"].is_null());
    feedback(&state.database, &mut run, "answer", Some(1));
    assert!(run.goal_finished()); // Remediation budget already spent; don't loop.
    assert_eq!(run.public()["goal_plan"]["objectives"][1]["correct"], false);
    assert_eq!(
        run.public()["goal_plan"]["objectives"][0]["verified"],
        false
    );
    assert!(call(&state.database, &mut run, "offer_quiz", quiz(1)).is_err());
    assert_eq!(run.steps.len(), 5);
}

#[test]
fn goal_wrong_answer_requires_one_remediation_and_never_changes_original_grade() {
    let (_dir, state, mut run) = prepare(1);
    run = state.database.study_goal_checkin(&run.id, "").unwrap();
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "understood", None);
    call(&state.database, &mut run, "offer_quiz", quiz(1)).unwrap();
    feedback(&state.database, &mut run, "answer", Some(1));
    let quiz_id = run.steps.last().unwrap().id.clone();
    assert!(!run.goal_finished());
    assert!(call(
        &state.database,
        &mut run,
        "finish_learning",
        json!({"next_topic":"下一步"})
    )
    .is_err());
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "prerequisite"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "understood", None);
    assert!(run.goal_finished());
    assert_eq!(run.public()["goal_plan"]["objectives"][0]["correct"], false);
    assert_eq!(
        run.public()["goal_plan"]["objectives"][0]["self_reported_understood"],
        true
    );
    assert!(state
        .database
        .study_feedback(&run.id, &quiz_id, "answer", Some(0))
        .is_err());
    // Identical retries remain harmless, even after advancement.
    let retry = state
        .database
        .study_feedback(&run.id, &quiz_id, "answer", Some(1))
        .unwrap();
    assert_eq!(retry.revision, run.revision);
    assert_eq!(
        state.database.study_memories(&run.topic).unwrap()[0]["attempts"],
        1
    );
}

#[test]
fn goal_skip_completes_only_the_round_without_verification_or_practice() {
    let (_dir, state, mut run) = prepare(1);
    run = state.database.study_goal_checkin(&run.id, "").unwrap();
    assert!(call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(4, "concept")
    )
    .is_err());
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept"),
    )
    .unwrap();
    feedback(&state.database, &mut run, "continue", None);
    call(&state.database, &mut run, "offer_quiz", quiz(1)).unwrap();
    feedback(&state.database, &mut run, "skip", None);
    run.model_calls = run.control.policy.max_model_calls;
    state.database.study_save(&mut run).unwrap();
    let final_value =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(final_value["state"], "completed");
    assert_eq!(final_value["goal_plan"]["verification_status"], "skipped");
    assert_eq!(final_value["goal_plan"]["objectives"][0]["verified"], false);
    assert!(final_value["goal_plan"]["objectives"][0]["correct"].is_null());
    assert_eq!(
        state.database.study_memories(&run.topic).unwrap(),
        json!([])
    );
    assert!(state.database.study_pending_goals().unwrap().is_empty());
    assert!(state.database.study_reopen_goal(&run.id).is_err());
}

#[test]
fn goal_reopen_preserves_identity_budget_and_waiting_phase_without_new_requests() {
    let (dir, state, mut run) = prepare(2);
    run.model_calls = 3;
    run.control.charged_tokens = 700;
    state.database.study_save(&mut run).unwrap();
    state.database.study_pause(&run.id, true).unwrap();
    let db = Database::new(dir.path().join("test.db"));
    db.initialize().unwrap();
    assert_eq!(
        db.study_home().unwrap()["goals"][0],
        json!({"id":run.id,"title":run.goal})
    );
    run = db.study_reopen_goal(&run.id).unwrap();
    assert_eq!(run.state, "waiting");
    assert_eq!(run.model_calls, 3);
    assert_eq!(run.control.charged_tokens, 700);
    assert_eq!(
        db.study_reopen_goal(&run.id).unwrap().revision,
        run.revision
    );
    assert!(db.study_pending_goals().unwrap().is_empty());
    run = db.study_goal_checkin(&run.id, "").unwrap();
    call(&db, &mut run, "present_lesson", lesson(1, "concept")).unwrap();
    let step_id = run.steps[0].id.clone();
    db.study_pause(&run.id, true).unwrap();
    run = db.study_reopen_goal(&run.id).unwrap();
    assert_eq!(run.state, "waiting");
    assert_eq!(run.steps[0].id, step_id);
    feedback(&db, &mut run, "understood", None);
    db.study_pause(&run.id, true).unwrap();
    run = db.study_reopen_goal(&run.id).unwrap();
    assert_eq!(run.state, "ready");
    assert_eq!(
        run.public()["goal_plan"]["objectives"][0]["self_reported_understood"],
        true
    );
    assert_eq!(run.model_calls, 3);
    db.study_pause(&run.id, true).unwrap();
    study_commands::start_goal_inner("另外一个目标".into(), &state).unwrap();
    assert!(db.study_reopen_goal(&run.id).is_err());
}

#[test]
fn goal_reopen_refuses_exhausted_budget_and_keeps_a_pending_tool_checkpoint() {
    let (_dir, state, mut run) = prepare(1);
    run = state.database.study_goal_checkin(&run.id, "").unwrap();
    run.pending = Some(StudyCall {
        id: "saved".into(),
        name: "present_lesson".into(),
        arguments: lesson(1, "concept").to_string(),
    });
    run.model_calls = run.control.policy.max_model_calls;
    state.database.study_save(&mut run).unwrap();
    let ended = state.database.study_pause(&run.id, true).unwrap();
    assert!(ended.pending.is_some());
    run = state.database.study_reopen_goal(&run.id).unwrap();
    assert_eq!(run.state, "ready");
    let shown =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(shown["steps"].as_array().unwrap().len(), 1);
    let ended = state.database.study_pause(&run.id, true).unwrap();
    assert_eq!(ended.public()["can_reopen_goal"], false);
    assert!(state.database.study_reopen_goal(&run.id).is_err());
    assert_eq!(
        state.database.study_load(&run.id).unwrap().model_calls,
        run.model_calls
    );
}

#[test]
fn goal_model_flow_uses_checkin_and_actual_feedback_then_finishes_locally() {
    use crate::review_tests::tools;
    let responses = vec![
        tools(vec![("ctx", "get_learning_context", json!({}))]),
        tools(vec![("search", "search_cards", json!({"query":"目标"}))]),
        tools(vec![("plan", "plan_learning", plan(2))]),
        tools(vec![("teach2", "present_lesson", lesson(2, "concept"))]),
        tools(vec![("remedial", "present_lesson", lesson(2, "example"))]),
        tools(vec![("teach1", "present_lesson", lesson(1, "concept"))]),
        tools(vec![("verify", "offer_quiz", quiz(2))]),
    ];
    let (_dir, state, model, _) = crate::review_tests::setup(responses);
    let created = study_commands::start_goal_inner("理解两个概念的区别".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let advance = || {
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap()
    };
    assert_eq!(advance()["state"], "waiting");
    assert_eq!(advance()["steps"], json!([])); // Can't advance past the initial question.
    state
        .database
        .study_goal_checkin(id, "我主要卡在第二部分")
        .unwrap();
    assert_eq!(advance()["steps"][0]["title"], "讲解目标2");
    let mut run = state.database.study_load(id).unwrap();
    feedback(&state.database, &mut run, "confused", None);
    assert_eq!(advance()["steps"][1]["kind"], "example");
    run = state.database.study_load(id).unwrap();
    feedback(&state.database, &mut run, "understood", None);
    advance();
    run = state.database.study_load(id).unwrap();
    feedback(&state.database, &mut run, "continue", None);
    advance();
    run = state.database.study_load(id).unwrap();
    feedback(&state.database, &mut run, "answer", Some(0));
    let done = advance();
    assert_eq!(done["state"], "completed");
    assert_eq!(done["goal_plan"]["objectives"][0]["verified"], false);
    assert_eq!(done["goal_plan"]["objectives"][1]["correct"], true);
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert!(requests[3]["messages"]
        .to_string()
        .contains("我主要卡在第二部分"));
    assert!(requests[4]["messages"].to_string().contains("confused"));
    assert!(requests[0]["tools"].to_string().contains("plan_learning"));
    assert!(!study_agent::definitions()
        .to_string()
        .contains("objective_id"));
}

#[test]
fn goal_pending_question_survives_end_and_must_be_answered_before_progress() {
    let (_dir, state, mut run) = prepare(1);
    run = state.database.study_goal_checkin(&run.id, "").unwrap();
    call(
        &state.database,
        &mut run,
        "present_lesson",
        lesson(1, "concept"),
    )
    .unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    run = state
        .database
        .study_ask(&run.id, &run.steps[0].id, "什么算新物质？", &request_id)
        .unwrap();
    assert!(call(&state.database, &mut run, "offer_quiz", quiz(1)).is_err());
    state.database.study_pause(&run.id, true).unwrap();
    run = state.database.study_reopen_goal(&run.id).unwrap();
    assert_eq!(run.state, "ready");
    assert_eq!(run.pending_question().unwrap().id, request_id);
    call(&state.database, &mut run, "answer_question", json!({"request_id":request_id,"kind":"example","text":"燃烧产生了与燃料不同的物质。","card_id":null})).unwrap();
    assert_eq!(run.state, "waiting");
    assert_eq!(run.steps.len(), 1);
    assert!(!run.goal_finished());
    assert_eq!(
        run.public()["goal_plan"]["objectives"][0]["verified"],
        false
    );
    feedback(&state.database, &mut run, "understood", None);
    call(&state.database, &mut run, "offer_quiz", quiz(1)).unwrap();
    assert_eq!(
        state.database.study_memories(&run.topic).unwrap(),
        json!([])
    );
}

#[test]
fn goal_network_retry_reuses_saved_plan_and_checkin_without_duplicate_lessons() {
    let (_dir, state, model, _) = crate::review_tests::setup(vec![
        crate::review_tests::tools(vec![("ctx", "get_learning_context", json!({}))]),
        crate::review_tests::tools(vec![("search", "search_cards", json!({"query":"变化"}))]),
        crate::review_tests::tools(vec![("plan", "plan_learning", plan(1))]),
        json!("NETWORK_ERROR"),
        crate::review_tests::tools(vec![("lesson", "present_lesson", lesson(1, "concept"))]),
    ]);
    let created = study_commands::start_goal_inner("理解变化".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let advance = || {
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap()
    };
    advance();
    let saved = state
        .database
        .study_goal_checkin(id, "我分不清两种变化")
        .unwrap();
    let failed = advance();
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["goal_plan"], saved.public()["goal_plan"]);
    let shown = advance();
    assert_eq!(shown["steps"].as_array().unwrap().len(), 1);
    assert_eq!(shown["goal_plan"]["checkin_reply"], "我分不清两种变化");
    assert_eq!(advance()["steps"].as_array().unwrap().len(), 1);
    assert_eq!(model.requests.lock().unwrap().len(), 5);
}

#[test]
fn goal_fields_default_for_legacy_records_and_reset_removes_pending_goals() {
    let (_dir, state, run) = prepare(1);
    let mut legacy = serde_json::to_value(&run).unwrap();
    legacy.as_object_mut().unwrap().remove("goal_mode");
    legacy.as_object_mut().unwrap().remove("goal_plan");
    let legacy: StudySession = serde_json::from_value(legacy).unwrap();
    assert!(!legacy.goal_mode);
    assert!(legacy.goal_plan.is_none());
    state.database.study_pause(&run.id, true).unwrap();
    assert_eq!(state.database.study_pending_goals().unwrap().len(), 1);
    state.database.study_reset().unwrap();
    assert!(state.database.study_pending_goals().unwrap().is_empty());
    assert!(state.database.study_load(&run.id).is_err());
}
