use crate::{
    commands::GenerationLock,
    providers::ProviderContext,
    review_agent::{self, LocalReviewLibrary, ReviewLibrary, ReviewRun, ReviewScope},
    AppState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartReview {
    pub kb: String,
    pub version: String,
    pub chapter: Option<String>,
    pub goal: String,
    pub provider: String,
    pub region: String,
    #[serde(default)]
    pub due_only: bool,
}

pub(crate) async fn start_inner(
    request: StartReview,
    state: &AppState,
    library: &dyn ReviewLibrary,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    if request.goal.trim().is_empty() || request.goal.chars().count() > 1000 {
        return Err("请输入 1–1000 字的复习目标".into());
    }
    let profile = state
        .database
        .provider_profile(&request.provider, &request.region)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified && p.key_last4.is_some())
        .ok_or("请先配置并测试模型通道")?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|e| e.to_string())?;
    let catalog=library.call(json!({"op":"learning_units","kb":request.kb,"version":request.version,"chapter":request.chapter})).await?;
    if catalog["kb"] != request.kb || catalog["version"] != request.version {
        return Err("资料版本已更新，请刷新后重试".into());
    }
    let scope = ReviewScope {
        kb: request.kb,
        version: request.version,
        filename: catalog["filename"].as_str().unwrap_or("教材").into(),
        chapter: request.chapter,
        chapter_path: serde_json::from_value(catalog["chapter_path"].clone())
            .map_err(|_| "无法读取章节范围")?,
    };
    let mut run = ReviewRun::new(
        scope,
        request.goal.trim().into(),
        &context,
        &profile.provider_id,
        &profile.region,
    );
    if request.due_only {
        let overview = state
            .database
            .review_memory_overview(&run.scope, chrono::Utc::now())
            .map_err(|e| e.to_string())?;
        let targets: Vec<_> = overview["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["is_due"] == true)
            .take(1)
            .cloned()
            .collect();
        if targets.is_empty() {
            return Err("当前范围内没有到期知识点，可以开始普通复习".into());
        }
        run.memory_ids = targets
            .iter()
            .filter_map(|m| m["id"].as_str().map(str::to_owned))
            .collect();
        run.due_memory_ids = run.memory_ids.clone();
        run.messages.push(json!({"role":"user","content":json!({"task":"按顺序复习这些到期知识点，每个最多一题。出题时带回对应memory_id，全部记录后结束。先重新查阅教材，不要直接复用旧题答案。","due_targets":targets}).to_string()}));
    }
    run.trace_state("created");
    state
        .database
        .review_insert(&run)
        .map_err(|e| e.to_string())?;
    Ok(run.public())
}

pub(crate) async fn continue_inner(
    id: String,
    state: &AppState,
    library: &dyn ReviewLibrary,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let existing = state.database.review_load(&id).map_err(|e| e.to_string())?;
    if existing.state == "completed" || existing.state == "waiting_answer" {
        return Ok(existing.public());
    }
    if ![review_agent::REVIEW_PROMPT_VERSION, "review-agent-v1"]
        .contains(&existing.prompt_version.as_str())
    {
        return Err("复习规则已更新，请开始新的复习".into());
    }
    let profile = state
        .database
        .provider_profile(&existing.provider, &existing.region)
        .map_err(|e| e.to_string())?
        .filter(|p| p.connection_verified && p.model == existing.model)
        .ok_or("模型设置已变化，请开始新的复习")?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|e| e.to_string())?;
    let key = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|e| e.to_string())?;
    library.call(json!({"op":"learning_version","kb":existing.scope.kb,"version":existing.scope.version})).await?;
    let mut run = state
        .database
        .review_claim(&id)
        .map_err(|e| e.to_string())?;
    run.trace_close_open("interrupted");
    run.trace_state(if existing.state == "ready" {
        "running"
    } else {
        "resumed"
    });
    state
        .database
        .review_save(&run, None)
        .map_err(|e| e.to_string())?;
    if let Err(error) = review_agent::drive(
        &state.database,
        library,
        state.http.as_ref(),
        &context,
        &key,
        &mut run,
    )
    .await
    {
        let current = state.database.review_load(&id).map_err(|e| e.to_string())?;
        let trace = std::mem::take(&mut run.trace);
        run = current;
        run.trace = trace;
        run.trace_close_open(if run.cancel_requested {
            "interrupted"
        } else {
            "failed"
        });
        run.state = if run.cancel_requested {
            "paused"
        } else {
            "failed"
        }
        .into();
        run.error = Some(if run.cancel_requested {
            "复习已暂停，已完成步骤已保存".into()
        } else {
            error
        });
        run.trace_state(&run.state.clone());
        if let Some(event) = run.trace.last_mut() {
            event.details["message"] = json!(run.error);
        }
        state
            .database
            .review_save(&run, None)
            .map_err(|e| e.to_string())?;
    }
    Ok(state
        .database
        .review_load(&id)
        .map_err(|e| e.to_string())?
        .public())
}

#[tauri::command]
pub async fn review_start(
    request: StartReview,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    start_inner(request, &state, &LocalReviewLibrary).await
}
#[tauri::command]
pub async fn review_continue(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    continue_inner(id, &state, &LocalReviewLibrary).await
}
#[tauri::command]
pub fn review_latest(kb: String, state: State<'_, AppState>) -> Result<Option<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .review_latest(&kb)
        .map(|r| r.map(|r| r.public()))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn review_read(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .review_load(&id)
        .map(|r| r.public())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn review_answer(
    id: String,
    question_id: String,
    selected: usize,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .review_answer(&id, &question_id, selected)
        .map(|r| r.public())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn review_cancel(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let _permit = state.persistence_gate.try_operation()?;
    state.database.review_cancel(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn review_trace(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .review_load(&id)
        .map(|run| run.trace_document())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn review_history(kb: String, state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .review_history(&kb)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn review_memory(
    kb: String,
    version: String,
    chapter: Option<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let catalog = LocalReviewLibrary
        .call(json!({"op":"learning_units","kb":kb,"version":version,"chapter":chapter}))
        .await?;
    if catalog["kb"] != kb || catalog["version"] != version {
        return Err("资料版本已更新，请刷新后重试".into());
    }
    let scope = ReviewScope {
        kb,
        version,
        chapter,
        filename: catalog["filename"].as_str().unwrap_or("教材").into(),
        chapter_path: serde_json::from_value(catalog["chapter_path"].clone())
            .map_err(|_| "无法读取章节范围")?,
    };
    state
        .database
        .review_memory_overview(&scope, chrono::Utc::now())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn review_export(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let document = {
        let _permit = state.persistence_gate.try_operation()?;
        let run = state.database.review_load(&id).map_err(|e| e.to_string())?;
        serde_json::to_vec_pretty(&run.trace_document()).map_err(|e| e.to_string())?
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title("导出复习执行记录")
            .add_filter("JSON", &["json"])
            .set_file_name(format!("review-trace-{id}.json"));
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(selected) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|_| "请选择本地文件路径")?;
        std::fs::write(&path, document).map_err(|_| "无法写入所选位置，请检查目录权限后重试")?;
        Ok(Some(path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}
