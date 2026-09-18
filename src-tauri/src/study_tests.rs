use crate::{
    db::Database,
    providers::ProviderContext,
    study::{StudyQuiz, StudySession, StudyStep},
};

fn setup() -> (tempfile::TempDir, Database, StudySession) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("study.db"));
    db.initialize().unwrap();
    let context =
        ProviderContext::from_registry("deepseek", "default", "deepseek-v4-flash").unwrap();
    let session = StudySession::new(
        "了解网络".into(),
        "网络".into(),
        "deepseek",
        "default",
        &context,
    );
    db.study_insert(&session).unwrap();
    (dir, db, session)
}
fn step(quiz: bool) -> StudyStep {
    StudyStep {
        id: "step1".into(),
        kind: if quiz { "quiz" } else { "concept" }.into(),
        title: "DNS".into(),
        text: "DNS 的作用是什么？".into(),
        reason: "从基础开始".into(),
        card_id: None,
        feedback: None,
        concept_key: None,
        quiz: quiz.then(|| StudyQuiz {
            options: vec!["查询地址".into(), "压缩文件".into()],
            correct_index: 0,
            explanation: "DNS 将域名解析到地址等记录。".into(),
            selected: None,
        }),
    }
}
#[test]
fn study_source_navigation_survives_restart_and_deleted_cards() {
    let (dir, state, _, _) = crate::review_tests::setup(vec![]);
    let cards = state.database.study_search_cards("").unwrap();
    let created =
        crate::study_commands::start_card_view_inner(cards[0].id.clone(), true, &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let reopened = Database::new(dir.path().join("test.db"));
    reopened.initialize().unwrap();
    assert_eq!(
        reopened.study_source_card(id).unwrap().unwrap().id,
        cards[0].id
    );
    assert_eq!(
        reopened.study_load(id).unwrap().public()["source_expanded"],
        true
    );
    assert_eq!(
        reopened.study_card_sessions(&cards[0].id).unwrap()[0]["id"],
        id
    );
    assert!(reopened
        .study_card_sessions(&cards[1].id)
        .unwrap()
        .is_empty());
    reopened.study_pause(id, true).unwrap();
    reopened.delete_library_card(&cards[0].id).unwrap();
    assert!(reopened.study_source_card(id).unwrap().is_none());
    assert_eq!(reopened.study_card_sessions(&cards[0].id).unwrap().len(), 1);
    assert!(reopened.study_source_card("missing-session").is_err());
    assert_eq!(
        reopened.study_load(id).unwrap().source_card.unwrap().title,
        cards[0].question
    );
    drop(dir);
}
#[test]
fn study_home_returns_only_unfinished_progress_without_answers_or_model_calls() {
    let (_dir, db, mut session) = setup();
    session.steps.push(step(true));
    session.state = "waiting".into();
    db.study_save(&mut session).unwrap();
    let home = db.study_home().unwrap();
    assert_eq!(home["active"]["id"], session.id);
    assert_eq!(home["active"]["step_count"], 1);
    assert!(!home.to_string().contains("correct_index"));
    assert_eq!(db.study_load(&session.id).unwrap().model_calls, 0);
    db.study_pause(&session.id, true).unwrap();
    assert!(db.study_home().unwrap()["active"].is_null());
}

#[test]
fn study_highlights_save_canonical_content_once_and_survive_restart_without_grading() {
    let (dir, db, mut run) = setup();
    let card = db.study_search_cards("").unwrap().remove(0);
    run.source_card = Some(card.clone().into());
    run.steps.push(step(false));
    run.questions.push(serde_json::from_value(serde_json::json!({"id":"q1","step_id":"step1","question":"域名是什么？","created_at":"2026-09-16","answer":{"kind":"explanation","text":"域名是便于人记忆的名称。","card_id":null}})).unwrap());
    run.state = "waiting".into();
    db.study_save(&mut run).unwrap();
    let before = db.study_load(&run.id).unwrap().public();
    let first = db.study_save_highlight(&run.id, "step", "step1").unwrap();
    assert_eq!(
        db.study_save_highlight(&run.id, "step", "step1").unwrap(),
        first
    );
    let answer = db.study_save_highlight(&run.id, "question", "q1").unwrap();
    assert!(db.study_save_highlight(&run.id, "step", "forged").is_err());
    assert!(db.study_save_highlight(&run.id, "invalid", "q1").is_err());
    assert_eq!(db.study_load(&run.id).unwrap().public(), before);
    assert_eq!(
        db.study_memories(&run.topic).unwrap(),
        serde_json::json!([])
    );
    let reopened = Database::new(dir.path().join("study.db"));
    reopened.initialize().unwrap();
    let notes = reopened.study_highlights(None, Some(&card.id)).unwrap();
    assert_eq!(notes.len(), 2);
    assert!(notes
        .iter()
        .any(|n| n["text"] == "域名是便于人记忆的名称。"));
    reopened.delete_library_card(&card.id).unwrap();
    assert_eq!(
        reopened
            .study_highlights(Some(&run.id), None)
            .unwrap()
            .len(),
        2
    );
    reopened.study_remove_highlight(&answer).unwrap();
    reopened.study_remove_highlight(&answer).unwrap();
    assert_eq!(
        reopened
            .study_highlights(Some(&run.id), None)
            .unwrap()
            .len(),
        1
    );
    assert!(reopened.study_load(&run.id).unwrap().questions[0]
        .answer
        .is_some());
    reopened.study_reset().unwrap();
    assert!(reopened
        .study_highlights(Some(&run.id), None)
        .unwrap()
        .is_empty());
}

#[test]
fn study_highlights_reject_unanswered_content_and_clear_with_all_data() {
    let (_dir, db, mut run) = setup();
    run.steps.push(step(true));
    run.questions.push(serde_json::from_value(serde_json::json!({"id":"pending","step_id":"step1","question":"问题","created_at":"2026-09-16","answer":null})).unwrap());
    db.study_save(&mut run).unwrap();
    assert!(db.study_save_highlight(&run.id, "step", "step1").is_err());
    assert!(db
        .study_save_highlight(&run.id, "question", "pending")
        .is_err());
    assert!(db.study_highlights(None, None).is_err());
    run.steps[0] = step(false);
    db.study_save(&mut run).unwrap();
    db.study_save_highlight(&run.id, "step", "step1").unwrap();
    db.clear_data("all").unwrap();
    assert!(db.study_highlights(Some(&run.id), None).unwrap().is_empty());
}

#[test]
fn study_display_reasons_never_infer_mastery_from_continue_or_model_claims() {
    let (_dir, db, mut session) = setup();
    let mut first = step(false);
    first.feedback = Some("continue".into());
    let mut next = step(true);
    next.id = "quiz".into();
    next.reason = "你已经理解了，证明你掌握了 DNS".into();
    session.steps = vec![first, next];
    db.study_save(&mut session).unwrap();
    let loaded = db.study_load(&session.id).unwrap().public();
    assert_eq!(
        loaded["steps"][1]["reason"],
        "用一道可跳过的小题回顾刚才的内容，提交后才记录练习结果。"
    );
    assert_eq!(loaded["summary"]["answered"], 0);
    assert_eq!(db.study_memories("网络").unwrap(), serde_json::json!([]));
}

#[test]
fn study_persists_recovers_and_rejects_stale_network_writes() {
    let (dir, db, session) = setup();
    let mut in_flight = db.study_claim(&session.id).unwrap();
    assert!(db.study_claim(&session.id).is_err());
    let reopened = Database::new(dir.path().join("study.db"));
    reopened.initialize().unwrap();
    reopened.study_recover().unwrap();
    assert_eq!(reopened.study_latest().unwrap().unwrap().state, "paused");
    in_flight.steps.push(step(false));
    assert!(db.study_save(&mut in_flight).is_err());
    let mut resumed = db.study_claim(&session.id).unwrap();
    db.study_pause(&session.id, true).unwrap();
    resumed.state = "waiting".into();
    assert!(db.study_save(&mut resumed).is_err());
    assert_eq!(db.study_load(&session.id).unwrap().state, "completed");
}
#[test]
fn study_real_answers_are_atomic_idempotent_and_hidden_until_submission() {
    let (_dir, db, session) = setup();
    let mut session = db.study_claim(&session.id).unwrap();
    session.steps.push(step(true));
    session.state = "waiting".into();
    db.study_save(&mut session).unwrap();
    assert!(session.public()["steps"][0]["quiz"]["correct_index"].is_null());
    assert!(db
        .study_feedback(&session.id, "step1", "answer", Some(4))
        .is_err());
    assert!(db
        .study_feedback(&session.id, "step1", "continue", None)
        .is_err());
    let graded = db
        .study_feedback(&session.id, "step1", "answer", Some(1))
        .unwrap();
    assert_eq!(graded.public()["summary"]["answered"], 1);
    assert_eq!(graded.public()["steps"][0]["quiz"]["correct"], false);
    db.study_feedback(&session.id, "step1", "answer", Some(1))
        .unwrap();
    assert!(db
        .study_feedback(&session.id, "step1", "answer", Some(0))
        .is_err());
    let memory = db.study_memories("网络").unwrap();
    assert_eq!(memory[0]["attempts"], 1);
    assert_eq!(memory[0]["correct_count"], 0);
}
#[test]
fn study_reading_and_skipping_never_claim_mastery_and_reset_removes_records() {
    let (_dir, db, session) = setup();
    let mut session = db.study_claim(&session.id).unwrap();
    session.steps.push(step(false));
    session.state = "waiting".into();
    db.study_save(&mut session).unwrap();
    db.study_feedback(&session.id, "step1", "confused", None)
        .unwrap();
    assert_eq!(db.study_memories("网络").unwrap(), serde_json::json!([]));
    assert!(db.study_insert(&session).is_err());
    db.clear_data("all").unwrap();
    assert!(db.study_latest().unwrap().is_none());
}

fn action(id: &str, name: &str, args: serde_json::Value) -> serde_json::Value {
    crate::review_tests::tools(vec![(id, name, args)])
}
fn lesson(kind: &str) -> serde_json::Value {
    serde_json::json!({"title":"理解 DNS","text":"DNS 帮助我们用域名查找服务器地址。","reason":"先从一个基础概念开始。","kind":kind,"card_id":null})
}

#[test]
fn study_card_entry_persists_exact_source_and_requires_reading_it_before_teaching() {
    use crate::{study::StudyCall, study_agent, study_commands};
    use serde_json::json;
    let (dir, state, model, _) = crate::review_tests::setup(vec![]);
    let cards = state.database.study_search_cards("").unwrap();
    let card = &cards[0];
    assert!(study_commands::start_card_inner("missing-card".into(), &state).is_err());
    let created = study_commands::start_card_inner(card.id.clone(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["source_card"]["card_id"], card.id);
    assert!(study_commands::start_card_inner(cards[1].id.clone(), &state).is_err());
    assert!(model.requests.lock().unwrap().is_empty());
    let mut run = state.database.study_load(id).unwrap();
    let call = |name: &str, args: serde_json::Value| StudyCall {
        id: "check".into(),
        name: name.into(),
        arguments: args.to_string(),
    };
    study_agent::execute(
        &state.database,
        &mut run,
        &call("get_learning_context", json!({})),
    )
    .unwrap();
    assert!(study_agent::execute(
        &state.database,
        &mut run,
        &call("present_lesson", lesson("concept"))
    )
    .is_err());
    assert!(study_agent::execute(
        &state.database,
        &mut run,
        &call("read_card", json!({"card_id":cards[1].id}))
    )
    .is_err());
    let read = study_agent::execute(
        &state.database,
        &mut run,
        &call("read_card", json!({"card_id":card.id})),
    )
    .unwrap();
    assert_eq!(read["explanation"], card.explanation);
    assert_eq!(read["trust_status"], json!(card.trust_status));
    state.database.study_save(&mut run).unwrap();
    model.replies.lock().unwrap().extend([
        action("lecture", "present_lesson", lesson("concept")),
        action("quiz", "offer_quiz", quiz()),
    ]);
    let first =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    let reopened = Database::new(dir.path().join("test.db"));
    assert_eq!(
        reopened
            .study_load(id)
            .unwrap()
            .source_card
            .unwrap()
            .explanation,
        card.explanation
    );
    state
        .database
        .study_feedback(
            id,
            first["steps"][0]["id"].as_str().unwrap(),
            "continue",
            None,
        )
        .unwrap();
    assert_eq!(state.database.study_home().unwrap()["practice_count"], 0);
    let second =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    state
        .database
        .study_feedback(
            id,
            second["steps"][1]["id"].as_str().unwrap(),
            "answer",
            Some(0),
        )
        .unwrap();
    let key: String = state
        .database
        .connect()
        .unwrap()
        .query_row("SELECT concept_key FROM study_memory", [], |r| r.get(0))
        .unwrap();
    assert_eq!(key, card.id); // Host owns identity even if the model omits card_id or renames a question.
    let mut legacy = serde_json::to_value(run).unwrap();
    legacy.as_object_mut().unwrap().remove("source_card");
    assert!(serde_json::from_value::<StudySession>(legacy)
        .unwrap()
        .source_card
        .is_none());
}
fn quiz() -> serde_json::Value {
    serde_json::json!({"title":"DNS","question":"DNS 用来做什么？","reason":"可以用一道小题检查理解，也可以跳过。","options":["查找地址","压缩文件"],"correct_index":0,"explanation":"DNS 查询域名对应的记录。","card_id":null})
}

fn prepare_question_session(state: &crate::AppState, is_quiz: bool) -> StudySession {
    let created =
        crate::study_commands::start_inner("理解 DNS".into(), "网络".into(), state).unwrap();
    let mut run = state
        .database
        .study_load(created["id"].as_str().unwrap())
        .unwrap();
    run.steps.push(step(is_quiz));
    run.context_read = true;
    run.searched = true;
    run.state = "waiting".into();
    state.database.study_save(&mut run).unwrap();
    run
}

fn question_answer(id: &str) -> serde_json::Value {
    serde_json::json!({"request_id":id,"kind":"comparison","text":"域名像姓名，IP 地址像住址；名字可以不变，地址可以更新。","card_id":null})
}

fn answered_question() -> crate::study::StudyQuestion {
    serde_json::from_value(serde_json::json!({"id":"q-original","step_id":"step1","question":"域名和地址有什么区别？","created_at":"2026-09-16","answer":{"kind":"comparison","text":"域名像姓名，IP 地址像住址；名字可以不变，地址可以更新。","card_id":null}})).unwrap()
}

#[test]
fn study_doubt_feedback_reexplains_once_rejects_identical_answers_and_never_grades() {
    use crate::study_commands;
    use serde_json::json;
    let (_dir, state, model, _) = crate::review_tests::setup(vec![]);
    let mut run = prepare_question_session(&state, false);
    run.questions.push(answered_question());
    state.database.study_save(&mut run).unwrap();
    let retry = state
        .database
        .study_question_feedback(&run.id, "q-original", "unresolved")
        .unwrap();
    assert_eq!(retry.questions.len(), 2);
    let question = retry.pending_question().unwrap();
    assert_eq!(question.previous_answers.len(), 1);
    assert_eq!(retry.steps.len(), 1);
    assert_eq!(
        state
            .database
            .study_question_feedback(&run.id, "q-original", "unresolved")
            .unwrap()
            .revision,
        retry.revision
    );
    model.replies.lock().unwrap().extend([
        action("old-id", "answer_question", question_answer("q-original")),
        action("repeat", "answer_question", question_answer(&question.id)),
        action("different", "answer_question", json!({"request_id":question.id,"kind":"example","text":"把网站换到另一台服务器时，域名仍可保持不变，而解析记录需要指向新地址。","card_id":null})),
    ]);
    let answered =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(answered["state"], "waiting");
    assert_eq!(answered["questions"][1]["answer"]["kind"], "example");
    assert_eq!(
        state.database.study_home().unwrap()["doubts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(model.requests.lock().unwrap().len(), 3);
    {
        let requests = model.requests.lock().unwrap();
        for request in requests.iter() {
            assert_eq!(
                request["tool_choice"]["function"]["name"],
                "answer_question"
            );
            let messages = request["messages"].as_array().unwrap();
            let current: serde_json::Value =
                serde_json::from_str(messages.last().unwrap()["content"].as_str().unwrap())
                    .unwrap();
            assert_eq!(current["request_id"], question.id);
            assert_eq!(current["pending_question"]["id"], question.id);
        }
        assert!(requests[1]["messages"].to_string().contains(&format!(
            "当前唯一待回答问题的 request_id 是 {}",
            question.id
        )));
    }
    assert!(model.requests.lock().unwrap()[0]["messages"]
        .to_string()
        .contains("previous_answers"));
    assert!(state
        .database
        .study_question_feedback(&run.id, "q-original", "understood")
        .is_err());
    let resolved = state
        .database
        .study_question_feedback(&run.id, &question.id, "understood")
        .unwrap();
    assert_eq!(resolved.public()["summary"]["answered"], 0);
    assert_eq!(state.database.study_home().unwrap()["doubts"], json!([]));
    assert_eq!(
        state.database.study_memories(&run.topic).unwrap(),
        json!([])
    );
    assert_eq!(
        state
            .database
            .study_question_feedback(&run.id, &question.id, "understood")
            .unwrap()
            .revision,
        resolved.revision
    );
}

#[test]
fn study_doubts_resume_exact_context_across_sessions_respect_privacy_and_clear() {
    use crate::study_commands;
    use serde_json::json;
    let (dir, state, model, _) = crate::review_tests::setup(vec![]);
    let mut old = prepare_question_session(&state, false);
    let card = state.database.study_search_cards("").unwrap().remove(0);
    old.source_card = Some(card.clone().into());
    old.source_expanded = true;
    old.questions.push(answered_question());
    old.state = "completed".into();
    state.database.study_save(&mut old).unwrap();
    state
        .database
        .study_question_feedback(&old.id, "q-original", "unresolved")
        .unwrap();
    let reopened = Database::new(dir.path().join("test.db"));
    reopened.initialize().unwrap();
    let doubt_id = reopened.study_home().unwrap()["doubts"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let started = study_commands::start_doubt_inner(doubt_id.clone(), &state).unwrap();
    let id = started["id"].as_str().unwrap().to_string();
    let q = started["questions"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(started["source_card"]["card_id"], card.id);
    assert_eq!(started["source_expanded"], true);
    assert_eq!(
        started["questions"][0]["question"],
        old.questions[0].question
    );
    assert_eq!(
        started["questions"][0]["previous_answers"][0]["text"],
        old.questions[0].answer.as_ref().unwrap().text
    );
    assert!(model.requests.lock().unwrap().is_empty());
    assert!(study_commands::start_doubt_inner(doubt_id.clone(), &state).is_err());
    let mut settings = state.database.settings().unwrap();
    settings.personalization_enabled = false;
    state.database.save_settings(&settings).unwrap();
    assert_eq!(state.database.study_home().unwrap()["doubts"], json!([]));
    assert!(
        tauri::async_runtime::block_on(study_commands::continue_inner(id.clone(), &state)).is_err()
    );
    assert!(model.requests.lock().unwrap().is_empty());
    settings.personalization_enabled = true;
    state.database.save_settings(&settings).unwrap();
    model.replies.lock().unwrap().push_back(
        action("reply", "answer_question", json!({"request_id":q,"kind":"clarification","text":"你是想知道域名为什么不直接等于 IP，还是想知道地址变化时域名如何保持不变？","card_id":card.id})),
    );
    tauri::async_runtime::block_on(study_commands::continue_inner(id.clone(), &state)).unwrap();
    {
        let requests = model.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]["tool_choice"]["function"]["name"],
            "answer_question"
        );
        assert_eq!(requests[0]["tools"].as_array().unwrap().len(), 1);
        let local_reads: Vec<_> = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|message| {
                serde_json::from_str::<serde_json::Value>(message["content"].as_str()?).ok()
            })
            .filter(|message| message["kind"] == "required_question_context")
            .map(|message| message["tool"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(local_reads, ["get_learning_context", "read_card"]);
        let loaded = state.database.study_load(&id).unwrap();
        assert!(loaded.context_read);
        assert!(loaded
            .discovered_cards
            .contains(&format!("read:{}", card.id)));
        assert_eq!(loaded.tool_calls, 3);
    }
    assert_eq!(
        state.database.study_home().unwrap()["doubts"]
            .as_array()
            .unwrap()
            .len(),
        1
    ); // Model cannot resolve a doubt.
    state
        .database
        .study_question_feedback(&id, &q, "understood")
        .unwrap();
    assert_eq!(reopened.study_home().unwrap()["doubts"], json!([]));
    state.database.study_pause(&id, true).unwrap();
    assert!(study_commands::start_doubt_inner(doubt_id, &state).is_err());
    assert_eq!(state.database.study_home().unwrap()["practice_count"], 0);
    state.database.study_reset().unwrap();
    assert!(state.database.study_open_doubts().unwrap().is_empty());
}

#[test]
fn study_doubt_feedback_at_budget_limit_stays_local_and_abandoned_reply_can_resume() {
    use crate::study_commands;
    let (_dir, state, model, _) = crate::review_tests::setup(vec![]);
    let mut run = prepare_question_session(&state, false);
    run.questions.push(answered_question());
    run.model_calls = run.control.policy.max_model_calls;
    state.database.study_save(&mut run).unwrap();
    assert!(state
        .database
        .study_question_feedback(&run.id, "missing", "unresolved")
        .is_err());
    assert!(state
        .database
        .study_question_feedback(&run.id, "q-original", "invented")
        .is_err());
    let noted = state
        .database
        .study_question_feedback(&run.id, "q-original", "unresolved")
        .unwrap();
    assert_eq!(noted.questions.len(), 1);
    assert_eq!(noted.state, "waiting");
    let doubt_id = noted.questions[0].doubt_id.clone().unwrap();
    state.database.study_pause(&run.id, true).unwrap();
    let pending = study_commands::start_doubt_inner(doubt_id.clone(), &state).unwrap();
    let id = pending["id"].as_str().unwrap();
    assert!(state
        .database
        .study_question_feedback(
            id,
            pending["questions"][0]["id"].as_str().unwrap(),
            "understood"
        )
        .is_err());
    state.database.study_pause(id, true).unwrap();
    let again = study_commands::start_doubt_inner(doubt_id, &state).unwrap();
    assert_eq!(
        again["questions"][0]["previous_answers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(model.requests.lock().unwrap().is_empty());
    state.database.clear_data("all").unwrap();
    assert!(state.database.study_open_doubts().unwrap().is_empty());
}

#[test]
fn study_doubt_clarification_keeps_identity_context_and_resolution_after_restart() {
    use crate::study_commands;
    use serde_json::json;
    let (dir, state, model, _) = crate::review_tests::setup(vec![]);
    let mut run = prepare_question_session(&state, false);
    run.questions.push(answered_question());
    state.database.study_save(&mut run).unwrap();
    let pending = state
        .database
        .study_question_feedback(&run.id, "q-original", "unresolved")
        .unwrap();
    let clarification_id = pending.pending_question().unwrap().id.clone();
    let doubt_id = pending
        .pending_question()
        .unwrap()
        .doubt_id
        .clone()
        .unwrap();
    let prompt = "你卡在名称和地址的区别，还是地址改变后的查找过程？";
    model.replies.lock().unwrap().push_back(action(
        "clarify",
        "answer_question",
        json!({
            "request_id":clarification_id,"kind":"clarification","text":prompt,"card_id":null
        }),
    ));
    tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state)).unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    let reply = "我不明白地址变了以后，怎么还能用原来的名字找到它。";
    let clarified = state
        .database
        .study_ask_with_reply(
            &run.id,
            "step1",
            reply,
            &request_id,
            Some(&clarification_id),
        )
        .unwrap();
    let question = clarified.pending_question().unwrap();
    assert_eq!(question.question, run.questions[0].question);
    assert_eq!(question.doubt_id.as_deref(), Some(doubt_id.as_str()));
    assert_eq!(question.previous_answers.len(), 2);
    assert_eq!(question.clarification_replies[0].prompt, prompt);
    assert_eq!(question.clarification_replies[0].reply, reply);
    assert_eq!(
        state
            .database
            .study_ask_with_reply(
                &run.id,
                "step1",
                reply,
                &request_id,
                Some(&clarification_id)
            )
            .unwrap()
            .revision,
        clarified.revision
    );
    assert!(state
        .database
        .study_ask_with_reply(
            &run.id,
            "step1",
            "覆盖补充",
            &request_id,
            Some(&clarification_id)
        )
        .is_err());
    assert!(state
        .database
        .study_ask(&run.id, "step1", reply, &request_id)
        .is_err());
    model.replies.lock().unwrap().extend([
        json!("NETWORK_ERROR"),
        action("reply", "answer_question", json!({"request_id":request_id,"kind":"example","text":"像通讯录：朋友搬家后，你用同一个姓名查到更新后的住址；DNS 查询的也是更新后的地址记录。","card_id":null})),
    ]);
    let failed =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(failed["state"], "failed");
    let reopened = Database::new(dir.path().join("test.db"));
    reopened.initialize().unwrap();
    assert_eq!(
        reopened
            .study_load(&run.id)
            .unwrap()
            .pending_question()
            .unwrap()
            .clarification_replies[0]
            .reply,
        reply
    );
    let explained =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(explained["steps"].as_array().unwrap().len(), 1);
    assert_eq!(explained["questions"][2]["answer"]["kind"], "example");
    assert!(state
        .database
        .study_question_feedback(&run.id, &clarification_id, "understood")
        .is_err());
    assert!(state
        .database
        .study_ask_with_reply(
            &run.id,
            "step1",
            reply,
            &uuid::Uuid::new_v4().to_string(),
            Some(&clarification_id)
        )
        .is_err());
    state.database.study_pause(&run.id, true).unwrap();
    let home = reopened.study_home().unwrap();
    assert_eq!(home["doubts"].as_array().unwrap().len(), 1);
    assert_eq!(home["doubts"][0]["id"], doubt_id);
    assert_eq!(home["doubts"][0]["question"], run.questions[0].question);
    assert_eq!(
        home["doubts"][0]["reason"],
        "你上次对“域名和地址有什么区别？”反馈“还没懂”。"
    );
    let resumed = study_commands::start_doubt_inner(doubt_id.clone(), &state).unwrap();
    assert_eq!(
        resumed["questions"][0]["clarification_replies"][0]["reply"],
        reply
    );
    assert_eq!(
        resumed["questions"][0]["previous_answers"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        resumed["doubt_target"]["reason"],
        home["doubts"][0]["reason"]
    );
    let new_id = resumed["id"].as_str().unwrap();
    let new_question_id = resumed["questions"][0]["id"].as_str().unwrap();
    model.replies.lock().unwrap().push_back(action("next-session", "answer_question", json!({
        "request_id":new_question_id,"kind":"explanation","text":"先区分名称与记录：名称保持不变，名称指向的记录可更新。再次查询会取得更新后的地址；缓存过期前可能仍得到旧记录。","card_id":null
    })));
    tauri::async_runtime::block_on(study_commands::continue_inner(new_id.into(), &state)).unwrap();
    {
        let requests = model.requests.lock().unwrap();
        let current: serde_json::Value = serde_json::from_str(
            requests.last().unwrap()["messages"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            current["pending_question"]["clarification_replies"][0]["reply"],
            reply
        );
        assert_eq!(
            current["pending_question"]["question"],
            run.questions[0].question
        );
    }
    assert_eq!(
        reopened.study_home().unwrap()["doubts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let resolved = reopened
        .study_question_feedback(new_id, new_question_id, "understood")
        .unwrap();
    assert_eq!(resolved.public()["summary"]["answered"], 0);
    assert_eq!(reopened.study_home().unwrap()["doubts"], json!([]));
    assert_eq!(reopened.study_home().unwrap()["practice_count"], 0);
    assert!(reopened
        .study_memories(&run.topic)
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn study_clarification_requires_current_context_and_never_infers_unresolved_feedback() {
    use serde_json::json;
    let (_dir, state, _, _) = crate::review_tests::setup(vec![]);
    let mut run = prepare_question_session(&state, false);
    let mut original = answered_question();
    original.answer.as_mut().unwrap().kind = "clarification".into();
    run.questions.push(original);
    state.database.study_save(&mut run).unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    for parent in ["missing", "from-another-session"] {
        assert!(state
            .database
            .study_ask_with_reply(&run.id, "step1", "卡在地址变化", &request_id, Some(parent))
            .is_err());
    }
    assert_eq!(
        state.database.study_load(&run.id).unwrap().revision,
        run.revision
    );
    let mut replied = state
        .database
        .study_ask_with_reply(
            &run.id,
            "step1",
            "卡在地址变化",
            &request_id,
            Some("q-original"),
        )
        .unwrap();
    assert_eq!(state.database.study_home().unwrap()["doubts"], json!([]));
    replied.questions.last_mut().unwrap().answer = Some(crate::study::StudyAnswer {
        kind: "explanation".into(),
        text: "用原名查更新后的地址记录。".into(),
        card_id: None,
    });
    replied.state = "waiting".into();
    state.database.study_save(&mut replied).unwrap();
    assert!(state
        .database
        .study_question_feedback(&run.id, "q-original", "unresolved")
        .is_err());
    assert!(state
        .database
        .study_ask_with_reply(
            &run.id,
            "step1",
            "再补充",
            &uuid::Uuid::new_v4().to_string(),
            Some(&request_id)
        )
        .is_err());
    let unrelated = state
        .database
        .study_ask(
            &run.id,
            "step1",
            "HTTP 是什么？",
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
    assert!(unrelated.pending_question().unwrap().doubt_id.is_none());
    assert!(unrelated
        .pending_question()
        .unwrap()
        .clarification_replies
        .is_empty());
    assert_eq!(state.database.study_home().unwrap()["doubts"], json!([]));
}

#[test]
fn study_questions_survive_failure_retry_and_restart_without_advancing_or_grading() {
    use crate::study_commands;
    use serde_json::json;
    let request_id = uuid::Uuid::new_v4().to_string();
    let (dir, state, model, _) = crate::review_tests::setup(vec![
        json!("NETWORK_ERROR"),
        action("reply", "answer_question", question_answer(&request_id)),
    ]);
    let run = prepare_question_session(&state, false);
    assert!(state
        .database
        .study_ask(&run.id, "other-step", "为什么？", &request_id)
        .is_err());
    let saved = state
        .database
        .study_ask(&run.id, "step1", "域名和地址有什么区别？", &request_id)
        .unwrap();
    let duplicate = state
        .database
        .study_ask(&run.id, "step1", "域名和地址有什么区别？", &request_id)
        .unwrap();
    assert_eq!(saved.revision, duplicate.revision);
    assert!(state
        .database
        .study_ask(&run.id, "step1", "覆盖问题", &request_id)
        .is_err());
    assert!(state
        .database
        .study_ask(
            &run.id,
            "step1",
            "第二个问题",
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
    assert!(model.requests.lock().unwrap().is_empty());
    let failed =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(failed["state"], "failed");
    let reopened = Database::new(dir.path().join("test.db"));
    assert_eq!(
        reopened
            .study_load(&run.id)
            .unwrap()
            .pending_question()
            .unwrap()
            .question,
        "域名和地址有什么区别？"
    );
    let answered =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(answered["state"], "waiting");
    assert_eq!(answered["steps"].as_array().unwrap().len(), 1);
    assert!(answered["steps"][0]["feedback"].is_null());
    assert_eq!(answered["questions"].as_array().unwrap().len(), 1);
    assert_eq!(answered["questions"][0]["answer"]["kind"], "comparison");
    assert_eq!(answered["summary"]["answered"], 0);
    assert_eq!(state.database.study_home().unwrap()["practice_count"], 0);
    assert_eq!(model.requests.lock().unwrap().len(), 2);
    assert!(model.requests.lock().unwrap()[1]["messages"]
        .to_string()
        .contains("域名和地址有什么区别"));
    assert_eq!(reopened.study_load(&run.id).unwrap().questions.len(), 1);
    let mut legacy = serde_json::to_value(&run).unwrap();
    legacy.as_object_mut().unwrap().remove("questions");
    assert!(serde_json::from_value::<StudySession>(legacy)
        .unwrap()
        .questions
        .is_empty());
}

#[test]
fn study_questions_block_premature_progress_and_enforce_identity_limits_and_quiz_rules() {
    use crate::{study::StudyCall, study_agent};
    use serde_json::json;
    let (_dir, state, _, _) = crate::review_tests::setup(vec![]);
    let run = prepare_question_session(&state, false);
    let call = |name: &str, args: serde_json::Value| StudyCall {
        id: "tool".into(),
        name: name.into(),
        arguments: args.to_string(),
    };
    assert!(state
        .database
        .study_ask(&run.id, "step1", " ", &uuid::Uuid::new_v4().to_string())
        .is_err());
    assert!(state
        .database
        .study_ask(
            &run.id,
            "step1",
            &"a".repeat(301),
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
    for _ in 0..4 {
        let request_id = uuid::Uuid::new_v4().to_string();
        let mut current = state
            .database
            .study_ask(&run.id, "step1", "我还是没懂", &request_id)
            .unwrap();
        for (name, args) in [
            ("present_lesson", lesson("example")),
            ("offer_quiz", quiz()),
            ("finish_learning", json!({"next_topic":"新主题"})),
        ] {
            assert!(
                study_agent::execute(&state.database, &mut current, &call(name, args)).is_err()
            );
        }
        assert!(study_agent::execute(
            &state.database,
            &mut current,
            &call("answer_question", question_answer("wrong-request"))
        )
        .is_err());
        let mut forged = question_answer(&request_id);
        forged["card_id"] = json!("fabricated");
        assert!(study_agent::execute(
            &state.database,
            &mut current,
            &call("answer_question", forged)
        )
        .is_err());
        study_agent::execute(
            &state.database,
            &mut current,
            &call("answer_question", question_answer(&request_id)),
        )
        .unwrap();
        assert!(study_agent::execute(
            &state.database,
            &mut current,
            &call("answer_question", question_answer(&request_id))
        )
        .is_err());
        state.database.study_save(&mut current).unwrap();
    }
    assert!(!state.database.study_load(&run.id).unwrap().can_ask());
    assert!(state
        .database
        .study_ask(
            &run.id,
            "step1",
            "第五个问题",
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
    state.database.study_pause(&run.id, true).unwrap();
    let mut quiz_run = prepare_question_session(&state, true);
    assert!(state
        .database
        .study_ask(
            &quiz_run.id,
            "step1",
            "告诉我答案",
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
    assert!(quiz_run.public()["steps"][0]["quiz"]["correct_index"].is_null());
    quiz_run.model_calls = quiz_run.control.policy.max_model_calls;
    quiz_run.steps = vec![step(false)];
    state.database.study_save(&mut quiz_run).unwrap();
    assert!(state
        .database
        .study_ask(
            &quiz_run.id,
            "step1",
            "预算耗尽的问题",
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
}

#[test]
fn study_question_checkpoint_replays_after_pause_even_at_final_step_and_call_budget() {
    use crate::{study::StudyCall, study_commands};
    let (_dir, state, model, _) = crate::review_tests::setup(vec![]);
    let mut run = prepare_question_session(&state, false);
    for i in 1..6 {
        let mut s = step(false);
        s.id = format!("step{}", i + 1);
        run.steps.push(s);
    }
    state.database.study_save(&mut run).unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    state
        .database
        .study_ask(&run.id, "step6", "最后一段有疑问", &request_id)
        .unwrap();
    let mut active = state.database.study_claim(&run.id).unwrap();
    active.model_calls = active.control.policy.max_model_calls;
    active.pending = Some(StudyCall {
        id: "saved-reply".into(),
        name: "answer_question".into(),
        arguments: question_answer(&request_id).to_string(),
    });
    state.database.study_save(&mut active).unwrap();
    state.database.study_pause(&run.id, false).unwrap();
    assert!(state.database.study_save(&mut active).is_err());
    let resumed =
        tauri::async_runtime::block_on(study_commands::continue_inner(run.id.clone(), &state))
            .unwrap();
    assert_eq!(resumed["state"], "waiting");
    assert_eq!(resumed["questions"][0]["answer"]["kind"], "comparison");
    assert_eq!(resumed["steps"].as_array().unwrap().len(), 6);
    assert!(model.requests.lock().unwrap().is_empty());
}

#[test]
fn study_question_after_consolidation_answer_preserves_schedule_and_allows_local_finish() {
    use crate::study_commands;
    let request_id = uuid::Uuid::new_v4().to_string();
    let (_dir, state, model, _) = crate::review_tests::setup(vec![action(
        "answer",
        "answer_question",
        question_answer(&request_id),
    )]);
    let key = due_practice(&state.database);
    let created = study_commands::start_review_inner(key.clone(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let mut run = state.database.study_load(id).unwrap();
    let mut s = step(true);
    s.concept_key = Some(key.clone());
    run.steps.push(s);
    run.context_read = true;
    run.searched = true;
    run.state = "waiting".into();
    state.database.study_save(&mut run).unwrap();
    state
        .database
        .study_feedback(id, "step1", "answer", Some(0))
        .unwrap();
    let before = state.database.study_memories("网络").unwrap();
    state
        .database
        .study_ask(id, "step1", "为什么不选另一个？", &request_id)
        .unwrap();
    let replied =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    assert_eq!(replied["state"], "ready");
    assert_eq!(replied["summary"]["answered"], 1);
    assert_eq!(before, state.database.study_memories("网络").unwrap());
    state
        .database
        .set_provider_verified("deepseek", "default", false)
        .unwrap();
    let finished =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    assert_eq!(finished["state"], "completed");
    assert_eq!(model.requests.lock().unwrap().len(), 1);
    state.database.study_reset().unwrap();
    assert!(state.database.study_history().unwrap().is_empty());
}

fn due_practice(db: &Database) -> String {
    let context =
        ProviderContext::from_registry("deepseek", "default", "deepseek-v4-flash").unwrap();
    let mut run = StudySession::new(
        "了解 DNS".into(),
        "网络".into(),
        "deepseek",
        "default",
        &context,
    );
    db.study_insert(&run).unwrap();
    run.steps.push(step(true));
    run.state = "waiting".into();
    db.study_save(&mut run).unwrap();
    db.study_feedback(&run.id, "step1", "answer", Some(1))
        .unwrap();
    db.study_pause(&run.id, true).unwrap();
    assert_eq!(db.study_home().unwrap()["due_count"], 0);
    db.connect()
        .unwrap()
        .execute(
            "UPDATE study_memory SET due_at=?",
            [chrono::Utc::now().timestamp() - 1],
        )
        .unwrap();
    db.study_home().unwrap()["due"][0]["concept_key"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn study_consolidation_uses_actual_answer_and_keeps_identity_when_question_title_changes() {
    use crate::{study_agent, study_commands};
    use serde_json::json;
    let mut changed = quiz();
    changed["title"] = json!("换一个标题问域名");
    changed["question"] = json!("服务器迁移后应该更新哪一类记录？");
    let (_dir, state, transport, _) = crate::review_tests::setup(vec![
        action("context", "get_learning_context", json!({})),
        action("search", "search_cards", json!({"query":"DNS"})),
        action("lesson", "present_lesson", lesson("example")),
        action("quiz", "offer_quiz", changed),
    ]);
    let key = due_practice(&state.database);
    let home = state.database.study_home().unwrap();
    assert_eq!(home["due_count"], 1);
    assert_eq!(home["due"][0]["last_correct"], false);
    let created = study_commands::start_review_inner(key.clone(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    assert!(study_commands::start_review_inner(key.clone(), &state).is_err());
    let mut saved = state.database.study_load(id).unwrap();
    let context = study_agent::execute(
        &state.database,
        &mut saved,
        &crate::study::StudyCall {
            id: "check".into(),
            name: "get_learning_context".into(),
            arguments: "{}".into(),
        },
    )
    .unwrap();
    assert_eq!(context["review_target"]["previous_quiz"]["selected"], 1);
    assert_eq!(
        context["review_target"]["previous_quiz"]["correct_index"],
        0
    );
    let first =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    let mut checkpoint = state.database.study_load(id).unwrap();
    let call = |name: &str, args: serde_json::Value| crate::study::StudyCall {
        id: "constraint-check".into(),
        name: name.into(),
        arguments: args.to_string(),
    };
    let mut repeated = quiz();
    repeated["question"] =
        serde_json::json!(checkpoint.review_target.as_ref().unwrap().previous_question);
    assert!(study_agent::execute(
        &state.database,
        &mut checkpoint,
        &call("offer_quiz", repeated)
    )
    .is_err());
    assert!(study_agent::execute(
        &state.database,
        &mut checkpoint,
        &call("finish_learning", serde_json::json!({"next_topic":"DNS"}))
    )
    .is_err());
    state
        .database
        .study_feedback(
            id,
            first["steps"][0]["id"].as_str().unwrap(),
            "continue",
            None,
        )
        .unwrap();
    let second =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    assert!(second["steps"][1]["quiz"]["correct_index"].is_null());
    let step_id = second["steps"][1]["id"].as_str().unwrap();
    state
        .database
        .study_feedback(id, step_id, "answer", Some(0))
        .unwrap();
    state
        .database
        .study_feedback(id, step_id, "answer", Some(0))
        .unwrap();
    // Completing after submission must work even if the model configuration is now unavailable.
    state
        .database
        .set_provider_verified("deepseek", "default", false)
        .unwrap();
    let done =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    assert_eq!(done["state"], "completed");
    assert_eq!(transport.requests.lock().unwrap().len(), 4);
    let conn = state.database.connect().unwrap();
    let values: (i64, i64, String) = conn
        .query_row(
            "SELECT attempts,correct_count,title FROM study_memory WHERE concept_key=?",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(values, (2, 1, "DNS".into()));
    assert_eq!(state.database.study_home().unwrap()["practice_count"], 1);
    assert_eq!(state.database.study_home().unwrap()["due_count"], 0);
}

#[test]
fn study_consolidation_skip_privacy_and_missing_records_do_not_fabricate_results() {
    use crate::study_commands;
    let (_dir, state, transport, _) = crate::review_tests::setup(vec![]);
    let key = due_practice(&state.database);
    assert!(study_commands::start_review_inner("forged".into(), &state).is_err());
    let mut settings = state.database.settings().unwrap();
    settings.personalization_enabled = false;
    state.database.save_settings(&settings).unwrap();
    assert_eq!(
        state.database.study_home().unwrap()["due"],
        serde_json::json!([])
    );
    assert!(study_commands::start_review_inner(key.clone(), &state).is_err());
    settings.personalization_enabled = true;
    state.database.save_settings(&settings).unwrap();
    let created = study_commands::start_review_inner(key.clone(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let mut run = state.database.study_load(id).unwrap();
    let mut question = step(true);
    question.concept_key = Some(key);
    run.steps.push(question);
    run.state = "waiting".into();
    state.database.study_save(&mut run).unwrap();
    state
        .database
        .study_feedback(id, "step1", "skip", None)
        .unwrap();
    let done =
        tauri::async_runtime::block_on(study_commands::continue_inner(id.into(), &state)).unwrap();
    assert_eq!(done["state"], "completed");
    assert_eq!(done["summary"]["answered"], 0);
    assert_eq!(state.database.study_home().unwrap()["due_count"], 1);
    assert_eq!(
        state.database.study_home().unwrap()["due"][0]["attempts"],
        1
    );
    assert!(transport.requests.lock().unwrap().is_empty());
    state.database.study_reset().unwrap();
    assert_eq!(state.database.study_home().unwrap()["practice_count"], 0);
}

#[test]
fn study_legacy_session_records_remain_readable_and_due_targets_require_original_answers() {
    let (_dir, db, session) = setup();
    let mut raw = serde_json::to_value(&session).unwrap();
    raw.as_object_mut().unwrap().remove("review_target");
    let mut old_step = serde_json::to_value(step(true)).unwrap();
    old_step.as_object_mut().unwrap().remove("concept_key");
    raw["steps"] = serde_json::json!([old_step]);
    let old: StudySession = serde_json::from_value(raw).unwrap();
    assert!(old.review_target.is_none());
    assert_eq!(old.steps[0].memory_key("网络"), "网络:dns");
    db.study_pause(&session.id, true).unwrap();
    let key = due_practice(&db);
    db.connect()
        .unwrap()
        .execute("DELETE FROM study_sessions", [])
        .unwrap();
    assert!(db.study_review_target(&key).is_err());
}

#[test]
fn study_agent_uses_feedback_tools_and_real_answers_to_finish_a_session() {
    use serde_json::json;
    let (_dir, state, model, _) = crate::review_tests::setup(vec![
        action("ctx", "get_learning_context", json!({})),
        action("search", "search_cards", json!({"query":"DNS"})),
        action("lesson", "present_lesson", lesson("concept")),
        // A premature quiz must be rejected after 'confused'; the model then repairs it.
        action("badquiz", "offer_quiz", quiz()),
        action("example", "present_lesson", lesson("example")),
        action("quiz", "offer_quiz", quiz()),
        action(
            "finish",
            "finish_learning",
            json!({"next_topic":"DNS 缓存"}),
        ),
    ]);
    let created =
        crate::study_commands::start_inner("了解网络".into(), "网络".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    let advance = || {
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.clone(), &state))
            .unwrap()
    };
    let first = advance();
    assert_eq!(first["state"], "waiting");
    assert_eq!(first["steps"].as_array().unwrap().len(), 1);
    state
        .database
        .study_feedback(
            &id,
            first["steps"][0]["id"].as_str().unwrap(),
            "confused",
            None,
        )
        .unwrap();
    let second = advance();
    assert_eq!(second["steps"][1]["kind"], "example");
    assert_eq!(state.database.study_memories("网络").unwrap(), json!([]));
    state
        .database
        .study_feedback(
            &id,
            second["steps"][1]["id"].as_str().unwrap(),
            "continue",
            None,
        )
        .unwrap();
    let third = advance();
    assert!(third["steps"][2]["quiz"]["correct_index"].is_null());
    let step = third["steps"][2]["id"].as_str().unwrap();
    state
        .database
        .study_feedback(&id, step, "answer", Some(0))
        .unwrap();
    let final_run = advance();
    assert_eq!(final_run["state"], "completed");
    assert_eq!(final_run["summary"]["correct"], 1);
    assert_eq!(final_run["next_topic"], "DNS 缓存");
    let calls = model.requests.lock().unwrap();
    assert!(calls[3]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "user" && m["content"].as_str().unwrap_or("").contains("confused")));
    assert!(calls[4]["messages"].as_array().unwrap().iter().any(
        |m| m["role"] == "tool" && m["content"].as_str().unwrap_or("").contains("用户希望补讲")
    ));
}

#[test]
fn study_saved_pending_tool_resumes_without_another_model_call() {
    let (_dir, state, model, _) = crate::review_tests::setup(vec![]);
    let value =
        crate::study_commands::start_inner("自然科学".into(), "自然科学".into(), &state).unwrap();
    let id = value["id"].as_str().unwrap();
    let mut run = state.database.study_claim(id).unwrap();
    run.context_read = true;
    run.searched = true;
    // The final allowed model request already returned: replaying its saved tool is free.
    run.model_calls = run.control.policy.max_model_calls;
    run.pending = Some(crate::study::StudyCall {
        id: "pending".into(),
        name: "present_lesson".into(),
        arguments: lesson("concept").to_string(),
    });
    state.database.study_save(&mut run).unwrap();
    state.database.study_recover().unwrap();
    let result =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(result["state"], "waiting");
    assert_eq!(result["steps"].as_array().unwrap().len(), 1);
    assert!(model.requests.lock().unwrap().is_empty());
}

#[test]
fn study_rejected_key_is_not_retried_or_left_verified() {
    use crate::providers::{ProviderError, ProviderTransport, TransportResponse};
    struct Rejected;
    #[async_trait::async_trait]
    impl ProviderTransport for Rejected {
        async fn post_json(
            &self,
            _: &url::Url,
            _: &crate::secret_store::SecretValue,
            _: &serde_json::Value,
            _: std::time::Duration,
        ) -> Result<TransportResponse, ProviderError> {
            Ok(TransportResponse {
                status: 401,
                body: "{}".into(),
            })
        }
    }
    let (_dir, mut state, _, _) = crate::review_tests::setup(vec![]);
    state.http = std::sync::Arc::new(Rejected);
    let created = crate::study_commands::start_inner("网络".into(), "网络".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let result =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(result["state"], "failed");
    assert!(result["error"].as_str().unwrap().contains("API Key"));
    assert_eq!(state.database.study_load(id).unwrap().model_calls, 1);
    assert!(
        !state
            .database
            .provider_profile("deepseek", "default")
            .unwrap()
            .unwrap()
            .connection_verified
    );
}

#[test]
fn study_cannot_forge_cards_or_grade_via_model_tools_and_honors_personalization() {
    let (_dir, db, mut run) = setup();
    run.context_read = true;
    run.searched = true;
    let mut args = lesson("concept");
    args["card_id"] = serde_json::json!("fabricated");
    let call = crate::study::StudyCall {
        id: "x".into(),
        name: "present_lesson".into(),
        arguments: args.to_string(),
    };
    assert!(crate::study_agent::execute(&db, &mut run, &call).is_err());
    assert!(run.steps.is_empty());
    let call = crate::study::StudyCall {
        id: "y".into(),
        name: "record_quiz_result".into(),
        arguments: "{}".into(),
    };
    assert!(crate::study_agent::execute(&db, &mut run, &call).is_err());
    let mut settings = db.settings().unwrap();
    settings.personalization_enabled = false;
    db.save_settings(&settings).unwrap();
    let context = db.study_context("网络").unwrap();
    assert_eq!(context["personalization_enabled"], false);
    assert_eq!(context["interests"], serde_json::json!([]));
}

#[test]
fn study_network_failure_preserves_progress_and_can_retry_without_duplicate_steps() {
    use serde_json::json;
    let (_dir, state, model, _) = crate::review_tests::setup(vec![
        json!("NETWORK_ERROR"),
        action("ctx", "get_learning_context", json!({})),
        action("s", "search_cards", json!({"query":"DNS"})),
        action("l", "present_lesson", lesson("concept")),
    ]);
    let created = crate::study_commands::start_inner("网络".into(), "网络".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let failed =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["can_resume"], true);
    let recovered =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(recovered["state"], "waiting");
    assert_eq!(recovered["steps"].as_array().unwrap().len(), 1);
    assert_eq!(model.requests.lock().unwrap().len(), 4);
}

#[test]
fn study_call_budget_blocks_network_and_skipped_quiz_has_no_score() {
    let (_dir, state, model, _) = crate::review_tests::setup(vec![]);
    let created = crate::study_commands::start_inner("网络".into(), "网络".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let mut run = state.database.study_claim(id).unwrap();
    run.steps.push(step(true));
    run.state = "waiting".into();
    state.database.study_save(&mut run).unwrap();
    let mut run = state
        .database
        .study_feedback(id, "step1", "skip", None)
        .unwrap();
    assert_eq!(run.public()["summary"]["answered"], 0);
    run.model_calls = run.control.policy.max_model_calls;
    state.database.study_save(&mut run).unwrap();
    assert!(
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .is_err()
    );
    assert!(model.requests.lock().unwrap().is_empty());
}

#[test]
fn study_history_and_reset_do_not_delete_cards_or_favorites() {
    let (_dir, db, run) = setup();
    let card = db.preview_card().unwrap();
    db.record_interaction(&card.id, "favorited").unwrap();
    db.study_claim(&run.id).unwrap();
    assert!(db.study_reset().is_err());
    db.study_pause(&run.id, false).unwrap();
    assert_eq!(db.study_history().unwrap().len(), 1);
    db.study_reset().unwrap();
    assert!(db.study_history().unwrap().is_empty());
    assert!(db.card_by_id(&card.id).unwrap().is_favorite);
}

#[test]
fn study_running_request_is_cancelled_and_late_content_cannot_be_committed() {
    use crate::providers::{ProviderError, ProviderTransport, TransportResponse};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Pending {
        db: Database,
        id: String,
        dropped: Arc<AtomicBool>,
    }
    struct DropMark(Arc<AtomicBool>);
    impl Drop for DropMark {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    #[async_trait::async_trait]
    impl ProviderTransport for Pending {
        async fn post_json(
            &self,
            _: &url::Url,
            _: &crate::secret_store::SecretValue,
            _: &serde_json::Value,
            _: std::time::Duration,
        ) -> Result<TransportResponse, ProviderError> {
            let _drop = DropMark(self.dropped.clone());
            self.db.study_pause(&self.id, true).unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            panic!("the cancelled request must be dropped");
        }
    }
    let (dir, mut state, _, _) = crate::review_tests::setup(vec![]);
    let created = crate::study_commands::start_inner("网络".into(), "网络".into(), &state).unwrap();
    let id = created["id"].as_str().unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    state.http = Arc::new(Pending {
        db: Database::new(dir.path().join("test.db")),
        id: id.into(),
        dropped: dropped.clone(),
    });
    let started = std::time::Instant::now();
    let result =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(result["state"], "completed");
    assert!(result["steps"].as_array().unwrap().is_empty());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
#[ignore = "opt-in configured provider smoke; isolated database, at most 8 requests"]
fn study_live_provider_smoke() {
    use crate::{
        db::ProviderProfileRecord,
        providers::{ProviderError, ProviderTransport, RestrictedHttpClient, TransportResponse},
        secret_store::{SecretStore, SecretValue, WindowsCredentialStore},
        AppState,
    };
    use std::sync::{atomic::AtomicBool, Arc};
    // Only status and fixed classifications are logged. Never print response bodies or keys.
    struct LiveTransport(RestrictedHttpClient);
    #[async_trait::async_trait]
    impl ProviderTransport for LiveTransport {
        async fn post_json(
            &self,
            endpoint: &url::Url,
            key: &SecretValue,
            body: &serde_json::Value,
            timeout: std::time::Duration,
        ) -> Result<TransportResponse, ProviderError> {
            let response = self.0.post_json(endpoint, key, body, timeout).await?;
            let parsed = serde_json::from_str::<serde_json::Value>(&response.body).ok();
            println!(
                "live HTTP status={}, JSON={}, provider_error={}",
                response.status,
                parsed.is_some(),
                parsed
                    .as_ref()
                    .is_some_and(|value| value.get("error").is_some())
            );
            Ok(response)
        }
    }
    let uri = std::env::var("TELLWHY_STUDY_LIVE_PROFILE_URI")
        .expect("explicit read-only profile URI required");
    let conn = rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let profile=conn.query_row("SELECT provider_id,region,model,credential_ref,key_last4,connection_verified FROM provider_profiles WHERE provider_id='deepseek' AND connection_verified=1 AND key_last4 IS NOT NULL LIMIT 1",[],|r|Ok(ProviderProfileRecord {provider_id:r.get(0)?,region:r.get(1)?,model:r.get(2)?,credential_ref:r.get(3)?,key_last4:r.get(4)?,connection_verified:r.get(5)?})).unwrap();
    let key = WindowsCredentialStore.get(&profile.credential_ref).unwrap();
    assert!(
        profile.key_last4.as_deref() == Some(&crate::secret_store::last_four(key.expose())),
        "credential does not match the saved profile"
    );
    let dir = tempfile::tempdir().unwrap();
    let database = Database::new(dir.path().join("study-live.db"));
    database.initialize().unwrap();
    database.save_provider_profile(&profile).unwrap();
    let state = AppState {
        database,
        secrets: Arc::new(WindowsCredentialStore),
        http: Arc::new(LiveTransport(RestrictedHttpClient::new().unwrap())),
        exiting: AtomicBool::new(false),
        generation_in_progress: AtomicBool::new(false),
        auto_hide: Default::default(),
        persistence_gate: Default::default(),
    };
    let created = crate::study_commands::start_inner(
        "我想了解 DNS，先解释它解决什么问题".into(),
        "计算机与互联网".into(),
        &state,
    )
    .unwrap();
    let id = created["id"].as_str().unwrap();
    let mut run = state.database.study_load(id).unwrap();
    run.control.policy.max_model_calls = 8;
    run.control.policy.max_active_ms = 180_000;
    state.database.study_save(&mut run).unwrap();
    let first =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(
        first["state"], "waiting",
        "first step error: {}",
        first["error"]
    );
    assert_eq!(first["steps"].as_array().unwrap().len(), 1);
    state
        .database
        .study_feedback(
            id,
            first["steps"][0]["id"].as_str().unwrap(),
            "confused",
            None,
        )
        .unwrap();
    let second =
        tauri::async_runtime::block_on(crate::study_commands::continue_inner(id.into(), &state))
            .unwrap();
    assert_eq!(
        second["state"], "waiting",
        "feedback step error: {}",
        second["error"]
    );
    let steps = second["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert!(["prerequisite", "example"].contains(&steps[1]["kind"].as_str().unwrap()));
    let done = state.database.study_pause(id, true).unwrap();
    println!(
        "{}",
        serde_json::json!({"model":done.model,"model_calls":done.model_calls,"tool_calls":done.tool_calls,"first_title":steps[0]["title"],"feedback_kind":steps[1]["kind"],"feedback_title":steps[1]["title"],"state":done.state})
    );
}
