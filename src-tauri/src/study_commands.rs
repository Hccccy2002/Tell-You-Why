use crate::{
    commands::GenerationLock,
    providers::ProviderContext,
    study::{StudySession, PROMPT_VERSION},
    study_agent, AppState,
};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub fn study_home(state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state.database.study_home().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn study_save_highlight(
    id: String,
    kind: String,
    source_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_save_highlight(&id, &kind, &source_id)
        .map_err(|e| e.to_string())
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_highlights(
    session_id: Option<String>,
    card_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_highlights(session_id.as_deref(), card_id.as_deref())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_remove_highlight(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_remove_highlight(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn study_history(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state.database.study_history().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_reset(state: State<'_, AppState>) -> Result<(), String> {
    let _permit = state.persistence_gate.try_reset()?;
    state.database.study_reset().map_err(|e| e.to_string())
}

pub(crate) fn start_inner(goal: String, topic: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let run = new_session(goal, topic, state)?;
    state
        .database
        .study_insert(&run)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}

pub(crate) fn start_goal_inner(goal: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let topic: String = goal.trim().chars().take(80).collect();
    let mut run = new_session(goal, topic, state)?;
    run.goal_mode = true;
    state
        .database
        .study_insert(&run)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}

#[tauri::command]
pub fn study_start_goal(goal: String, state: State<'_, AppState>) -> Result<Value, String> {
    start_goal_inner(goal, &state)
}

#[tauri::command]
pub fn study_goal_checkin(
    id: String,
    reply: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_goal_checkin(&id, &reply)
        .map(|r| r.public())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn study_reopen_goal(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_reopen_goal(&id)
        .map(|r| r.public())
        .map_err(|e| e.to_string())
}

pub(crate) fn start_review_inner(concept_key: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let target = state
        .database
        .study_review_target(&concept_key)
        .map_err(|e| e.to_string())?;
    let mut run = new_session(
        format!("巩固：{}", target.title),
        target.topic.clone(),
        state,
    )?;
    run.review_target = Some(target);
    state
        .database
        .study_insert(&run)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}

pub(crate) fn start_card_inner(card_id: String, state: &AppState) -> Result<Value, String> {
    start_card_view_inner(card_id, false, state)
}

pub(crate) fn start_card_view_inner(
    card_id: String,
    expanded: bool,
    state: &AppState,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let card = state
        .database
        .card_by_id(&card_id)
        .map_err(|_| "这张卡片已不存在，请返回小窗刷新")?;
    let goal = format!(
        "围绕这张卡学习：{}",
        card.question.chars().take(180).collect::<String>()
    );
    let mut run = new_session(goal, card.topic_label.clone(), state)?;
    run.source_card = Some(card.into());
    run.source_expanded = expanded;
    state
        .database
        .study_insert(&run)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}

fn new_session(goal: String, topic: String, state: &AppState) -> Result<StudySession, String> {
    let goal = goal.trim();
    let topic = topic.trim();
    if goal.is_empty()
        || goal.chars().count() > 200
        || topic.is_empty()
        || topic.chars().count() > 80
    {
        return Err("请输入学习主题（最多80字）和目标（最多200字）".into());
    }
    let preferred = state
        .database
        .generation_provider_id()
        .map_err(|e| e.to_string())?;
    let profiles = state
        .database
        .provider_profiles()
        .map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .filter(|p| p.connection_verified && p.key_last4.is_some())
        .min_by_key(|p| usize::from(p.provider_id != preferred))
        .ok_or("请先在模型设置中配置并测试一个模型")?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|e| e.to_string())?;
    let run = StudySession::new(
        goal.into(),
        topic.into(),
        &profile.provider_id,
        &profile.region,
        &context,
    );
    Ok(run)
}

pub(crate) async fn continue_inner(id: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let mut existing = state.database.study_load(&id).map_err(|e| e.to_string())?;
    if ["waiting", "completed"].contains(&existing.state.as_str()) {
        return Ok(existing.public());
    }
    if (existing.goal_mode && existing.goal_finished())
        || (existing.review_target.is_some()
            && existing.pending_question().is_none()
            && existing
                .steps
                .iter()
                .any(|s| s.quiz.is_some() && s.feedback.is_some()))
    {
        existing.state = "completed".into();
        existing.next_topic = None;
        existing.pending = None;
        existing.error = None;
        state
            .database
            .study_save(&mut existing)
            .map_err(|e| e.to_string())?;
        return Ok(existing.public());
    }
    if existing.prompt_version != PROMPT_VERSION {
        return Err("学习规则已更新，请结束本次学习并重新开始".into());
    }
    if (existing.review_target.is_some() || existing.doubt_target.is_some())
        && !state
            .database
            .settings()
            .map_err(|e| e.to_string())?
            .personalization_enabled
    {
        return Err(
            "个性化已关闭，本次历史学习暂停；可以结束本次学习或重新开启个性化后继续".into(),
        );
    }
    let profile = state
        .database
        .provider_profile(&existing.provider, &existing.region)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified && p.model == existing.model)
        .ok_or("本次学习的模型设置已变化，请恢复原设置或结束后重新开始")?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|e| e.to_string())?;
    let key = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|e| e.to_string())?;
    let mut run = state.database.study_claim(&id).map_err(|e| e.to_string())?;
    if let Err(error) = study_agent::drive(
        &state.database,
        state.http.as_ref(),
        &context,
        &key,
        &mut run,
    )
    .await
    {
        let latest = state.database.study_load(&id).map_err(|e| e.to_string())?;
        if latest.state == "running" && latest.revision == run.revision {
            run.state = "failed".into();
            run.error = Some(error.clone());
            study_agent::failure(&mut run.control, &error);
            state
                .database
                .study_save(&mut run)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(state
        .database
        .study_load(&id)
        .map_err(|e| e.to_string())?
        .public())
}

#[tauri::command]
pub fn study_start(
    goal: String,
    topic: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    start_inner(goal, topic, &state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn study_start_review(
    concept_key: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    start_review_inner(concept_key, &state)
}
pub(crate) fn start_doubt_inner(doubt_id: String, state: &AppState) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let mut run = new_session("接着解决疑问".into(), "学习疑问".into(), state)?;
    state
        .database
        .study_insert_doubt(&mut run, &doubt_id)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_start_doubt(doubt_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    start_doubt_inner(doubt_id, &state)
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_question_feedback(
    id: String,
    question_id: String,
    feedback: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_question_feedback(&id, &question_id, &feedback)
        .map(|run| run.public())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn study_continue(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    continue_inner(id, &state).await
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_start_card(card_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    start_card_inner(card_id, &state)
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_start_card_view(
    card_id: String,
    expanded: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    start_card_view_inner(card_id, expanded, &state)
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_card_sessions(
    card_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_card_sessions(&card_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_source_card(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::models::KnowledgeCard>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_source_card(&id)
        .map_err(|e| e.to_string())
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_ask(
    id: String,
    step_id: String,
    question: String,
    request_id: String,
    reply_to_question_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let result = if let Some(reply_to) = reply_to_question_id {
        state
            .database
            .study_ask_with_reply(&id, &step_id, &question, &request_id, Some(&reply_to))
    } else {
        state
            .database
            .study_ask(&id, &step_id, &question, &request_id)
    };
    result.map(|r| r.public()).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_latest(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_latest()
        .map(|s| s.map(|s| s.public()))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_read(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_load(&id)
        .map(|s| s.public())
        .map_err(|e| e.to_string())
}
#[tauri::command(rename_all = "camelCase")]
pub fn study_feedback(
    id: String,
    step_id: String,
    feedback: String,
    selected: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_feedback(&id, &step_id, &feedback, selected)
        .map(|s| s.public())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn study_pause(id: String, finish: bool, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .study_pause(&id, finish)
        .map(|s| s.public())
        .map_err(|e| e.to_string())
}
