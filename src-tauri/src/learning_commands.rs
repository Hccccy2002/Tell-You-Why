use crate::{
    learning::StartLearning,
    learning_store::{LearningEvent, LearningSession},
    AppState,
};
use chrono::Utc;
use serde_json::{json, Value};
use tauri::State;

fn view(state: &AppState, kb: &str, session: Option<LearningSession>) -> Result<Value, String> {
    let card_id = session
        .as_ref()
        .and_then(|s| s.cursor.and_then(|i| s.history.get(i)))
        .map(|p| p.card_id.as_str());
    let card = card_id
        .map(|id| state.database.learning_card(id))
        .transpose()
        .map_err(|e| e.to_string())?;
    let learning = card_id
        .map(|id| state.database.learning_state(id))
        .transpose()
        .map_err(|e| e.to_string())?;
    let summary = state
        .database
        .learning_summary(kb, Utc::now())
        .map_err(|e| e.to_string())?;
    Ok(json!({"session":session,"card":card,"state":learning,"summary":summary,"reason":null}))
}

#[tauri::command]
pub fn learning_start(request: StartLearning, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let kb = request.kb.clone();
    let session = state
        .database
        .learning_start(request, Utc::now())
        .map_err(|e| e.to_string())?;
    view(&state, &kb, Some(session))
}
#[tauri::command]
pub fn learning_resume(kb: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let session = state
        .database
        .learning_resume(&kb)
        .map_err(|e| e.to_string())?;
    view(&state, &kb, session)
}
#[tauri::command]
pub fn learning_next(
    session_id: String,
    revision: u64,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let seed = uuid::Uuid::new_v4().as_u128() as u64;
    let selected = state
        .database
        .learning_next(&session_id, revision, Utc::now(), seed)
        .map_err(|e| e.to_string())?;
    let kb = selected.session.kb.clone();
    let mut result = view(&state, &kb, Some(selected.session))?;
    result["reason"] = json!(selected.reason);
    result["scoped"] = json!(selected.scoped);
    result["eligible"] = json!(selected.eligible);
    Ok(result)
}
#[tauri::command]
pub fn learning_previous(
    session_id: String,
    revision: u64,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let session = state
        .database
        .learning_previous(&session_id, revision, Utc::now())
        .map_err(|e| e.to_string())?;
    let kb = session.kb.clone();
    view(&state, &kb, Some(session))
}
#[tauri::command]
pub fn learning_record_event(
    event: LearningEvent,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .learning_record(&event, Utc::now())
        .map_err(|e| e.to_string())?;
    let session = crate::learning_store::read_session(
        &state.database.connect().map_err(|e| e.to_string())?,
        &event.session_id,
    )
    .map_err(|e| e.to_string())?;
    let kb = session.kb.clone();
    view(&state, &kb, Some(session))
}
#[tauri::command]
pub fn learning_reset(kb: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_reset()?;
    state
        .database
        .learning_reset(&kb)
        .map_err(|e| e.to_string())?;
    view(&state, &kb, None)
}
