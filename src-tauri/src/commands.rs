use crate::content::parse_import;
use crate::db::{DbError, ProviderProfileRecord};
use crate::models::{
    AppSettings, BootstrapData, DesktopCapabilities, FollowUpMessage, FollowUpResponse,
    FollowUpRole, FollowUpTurn, GenerationBatchResult, GenerationUsage, ImportResult,
    KnowledgeCard, LibraryItem, OnboardingInput, ProviderSpec, ReminderPreset, SaveProviderInput,
    TopicPreference,
};
use crate::providers::{
    adapter, FollowUpRequest, GenerationRequest, ProviderContext, ProviderRegistry,
};
use crate::secret_store::{credential_ref, last_four};
use crate::AppState;
use chrono::{Duration, Local, TimeZone, Utc};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;
use uuid::Uuid;

type CommandResult<T> = Result<T, String>;

const MAX_FOLLOW_UP_QUESTION_CHARS: usize = 500;
const MAX_FOLLOW_UP_HISTORY_TURNS: usize = 6;
const MAX_FOLLOW_UP_ANSWER_HISTORY_CHARS: usize = 4_000;
const MODEL_OPERATION_IN_PROGRESS_ERROR: &str = "已有模型请求或敏感设置操作正在进行，请稍后重试";
const PROVIDER_KEY_REPLACEMENT_CONFIRMATION_REQUIRED: &str =
    "目标服务通道已有 API Key，请确认覆盖后重试";

pub(crate) struct GenerationLock<'a>(&'a AtomicBool);

impl<'a> GenerationLock<'a> {
    pub(crate) fn acquire(flag: &'a AtomicBool) -> CommandResult<Self> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self(flag))
            .map_err(|_| MODEL_OPERATION_IN_PROGRESS_ERROR.into())
    }
}

impl Drop for GenerationLock<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn bootstrap_app(
    record_shown: Option<bool>,
    state: State<'_, AppState>,
) -> CommandResult<BootstrapData> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let selected = if record_shown.unwrap_or(true) {
        state.database.next_card(None)
    } else {
        state.database.preview_card()
    };
    let card = match selected {
        Ok(card) => Some(card),
        Err(DbError::NoCards) => None,
        Err(error) => return Err(error.to_string()),
    };
    Ok(BootstrapData {
        onboarding_complete: state
            .database
            .onboarding_complete()
            .map_err(|error| error.to_string())?,
        card,
        available_card_count: state
            .database
            .available_card_count()
            .map_err(|error| error.to_string())?,
        topics: state.database.topics().map_err(|error| error.to_string())?,
        settings: state
            .database
            .settings()
            .map_err(|error| error.to_string())?,
        providers: ProviderRegistry::specs(&state.database).map_err(|error| error.to_string())?,
        generation_provider_id: state
            .database
            .generation_provider_id()
            .map_err(|error| error.to_string())?,
        desktop_capabilities: DesktopCapabilities {
            tray: true,
            notifications: true,
            secure_credentials: true,
        },
        notice: None,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_generation_provider(
    provider_id: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    if !matches!(provider_id.as_str(), "deepseek" | "kimi") {
        return Err("只能选择 DeepSeek 或 Kimi".into());
    }
    state
        .database
        .save_generation_provider_id(&provider_id)
        .map_err(|error| error.to_string())?;
    Ok(provider_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn next_card(current_id: String, state: State<'_, AppState>) -> CommandResult<KnowledgeCard> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .next_card(Some(&current_id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn available_card_count(state: State<'_, AppState>) -> CommandResult<u32> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .available_card_count()
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn record_interaction(
    card_id: String,
    kind: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .record_interaction(&card_id, &kind)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_onboarding(
    input: OnboardingInput,
    state: State<'_, AppState>,
) -> CommandResult<Vec<TopicPreference>> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .save_onboarding(&input)
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_interests(
    topics: Vec<TopicPreference>,
    personalization_enabled: bool,
    state: State<'_, AppState>,
) -> CommandResult<Vec<TopicPreference>> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .save_interests(&topics, personalization_enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_library(
    mode: String,
    topic_id: Option<String>,
    sort: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<LibraryItem>> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .list_library(&mode, topic_id.as_deref(), sort != "oldest")
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_library_card(card_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    state
        .database
        .delete_library_card(&card_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_settings(
    settings: AppSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    if settings.global_shortcut.trim().is_empty() {
        return Err("全局快捷键不能为空".into());
    }
    if !(1..=1000).contains(&settings.daily_generation_limit) {
        return Err("每日生成总数应为 1–1000 张".into());
    }
    if chrono::NaiveTime::parse_from_str(&settings.quiet_start, "%H:%M").is_err()
        || chrono::NaiveTime::parse_from_str(&settings.quiet_end, "%H:%M").is_err()
    {
        return Err("请设置有效的静默时段".into());
    }
    if settings.reminder_preset != ReminderPreset::Manual {
        if settings.reminder_times.is_empty()
            || settings.reminder_times.len() > 10
            || settings
                .reminder_times
                .iter()
                .any(|value| chrono::NaiveTime::parse_from_str(value, "%H:%M").is_err())
        {
            return Err("请设置有效的提醒时间".into());
        }
        if settings.weekdays.is_empty()
            || settings.weekdays.iter().any(|day| !(1..=7).contains(day))
        {
            return Err("请至少选择一个有效的提醒日".into());
        }
    }
    let previous = state
        .database
        .settings()
        .map_err(|error| error.to_string())?;
    if settings.global_shortcut != previous.global_shortcut {
        app.global_shortcut()
            .unregister_all()
            .map_err(|_| "无法更新全局快捷键".to_string())?;
        if app
            .global_shortcut()
            .register(settings.global_shortcut.as_str())
            .is_err()
        {
            let _ = app
                .global_shortcut()
                .register(previous.global_shortcut.as_str());
            return Err("这个快捷键已被其他应用占用，请换一个组合".into());
        }
    }
    if settings.autostart {
        app.autolaunch()
            .enable()
            .map_err(|_| "无法开启开机启动".to_string())?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|_| "无法关闭开机启动".to_string())?;
    }
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(settings.always_on_top)
            .map_err(|_| "无法更新置顶状态".to_string())?;
    }
    state
        .database
        .save_settings(&settings)
        .map_err(|error| error.to_string())?;
    state
        .auto_hide
        .set_enabled(settings.auto_hide_on_mouse_leave);
    Ok(settings)
}

#[tauri::command]
pub fn set_auto_hide_suspended(suspended: bool, state: State<'_, AppState>) {
    state.auto_hide.set_suspended(suspended);
}

#[tauri::command]
pub fn generation_usage(state: State<'_, AppState>) -> CommandResult<GenerationUsage> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let settings = state
        .database
        .settings()
        .map_err(|error| error.to_string())?;
    Ok(GenerationUsage {
        generated: state
            .database
            .generated_today()
            .map_err(|error| error.to_string())?,
        total: settings.daily_generation_limit,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub fn pause_reminders(mode: String, state: State<'_, AppState>) -> CommandResult<String> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let until = match mode.as_str() {
        "thirty_minutes" => Utc::now() + Duration::minutes(30),
        "today" => {
            let tomorrow = Local::now()
                .date_naive()
                .succ_opt()
                .ok_or_else(|| "无法计算暂停时间".to_string())?;
            let local_midnight = tomorrow
                .and_hms_opt(0, 0, 0)
                .and_then(|value| Local.from_local_datetime(&value).single())
                .ok_or_else(|| "无法计算暂停时间".to_string())?;
            local_midnight.with_timezone(&Utc)
        }
        _ => return Err("未知的暂停方式".into()),
    };
    let mut settings = state
        .database
        .settings()
        .map_err(|error| error.to_string())?;
    settings.pause_until = Some(until.to_rfc3339());
    state
        .database
        .save_settings(&settings)
        .map_err(|error| error.to_string())?;
    Ok(until.to_rfc3339())
}

#[tauri::command]
pub fn save_provider_profile(
    input: SaveProviderInput,
    state: State<'_, AppState>,
) -> CommandResult<ProviderSpec> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let _ = cleanup_pending_credentials(&state);
    save_provider_profile_inner(input, &state)
}

fn require_provider_key_replacement_confirmation(
    profile_exists: bool,
    has_new_key: bool,
    replacement_confirmed: bool,
) -> CommandResult<()> {
    if profile_exists && has_new_key && !replacement_confirmed {
        return Err(PROVIDER_KEY_REPLACEMENT_CONFIRMATION_REQUIRED.into());
    }
    Ok(())
}

fn cleanup_pending_credentials(state: &AppState) -> CommandResult<()> {
    let pending = state
        .database
        .pending_credential_deletions()
        .map_err(|error| error.to_string())?;
    let mut failures = 0usize;
    for credential_ref in pending {
        if state.secrets.delete(&credential_ref).is_err() {
            failures += 1;
            continue;
        }
        if state
            .database
            .complete_credential_deletion(&credential_ref)
            .is_err()
        {
            failures += 1;
        }
    }
    if failures == 0 {
        Ok(())
    } else {
        Err(format!(
            "{failures} 条系统凭据暂未清理，已登记并会在后续凭据操作时重试"
        ))
    }
}

fn save_provider_profile_inner(
    input: SaveProviderInput,
    state: &AppState,
) -> CommandResult<ProviderSpec> {
    let endpoint = ProviderRegistry::endpoint(&input.provider_id, &input.region)
        .map_err(|error| error.to_string())?;
    ProviderRegistry::validate_model(&endpoint, &input.model).map_err(|error| error.to_string())?;
    let existing = state
        .database
        .provider_profile(&input.provider_id, &input.region)
        .map_err(|error| error.to_string())?;
    let new_api_key = input
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let has_new_key = new_api_key.is_some();
    require_provider_key_replacement_confirmation(
        existing.is_some(),
        has_new_key,
        input.replace_existing_key,
    )?;

    let credential_base =
        credential_ref(&input.provider_id, &input.region).map_err(|error| error.to_string())?;
    let credential = if has_new_key {
        format!("{credential_base}:{}", Uuid::new_v4().simple())
    } else {
        existing
            .as_ref()
            .map(|profile| profile.credential_ref.clone())
            .ok_or_else(|| "请输入完整 API Key".to_string())?
    };
    let key_last4 = new_api_key.as_deref().map(last_four).or_else(|| {
        existing
            .as_ref()
            .and_then(|profile| profile.key_last4.clone())
    });
    let remains_verified = existing.as_ref().is_some_and(|profile| {
        profile.connection_verified && profile.model == input.model && !has_new_key
    });
    let profile = ProviderProfileRecord {
        provider_id: input.provider_id.clone(),
        region: input.region.clone(),
        model: input.model,
        credential_ref: credential,
        key_last4,
        connection_verified: remains_verified,
    };
    if let Some(api_key) = new_api_key.as_deref() {
        state
            .database
            .queue_credential_deletion(&profile.credential_ref)
            .map_err(|error| error.to_string())?;
        if let Err(error) = state.secrets.save(&profile.credential_ref, api_key) {
            let cleanup = cleanup_pending_credentials(state).err();
            return Err(match cleanup {
                Some(cleanup) => format!("{error}；{cleanup}"),
                None => error.to_string(),
            });
        }
        if let Err(error) = state.database.activate_provider_profile_credential(
            &profile,
            existing
                .as_ref()
                .map(|previous| previous.credential_ref.as_str()),
        ) {
            let cleanup = cleanup_pending_credentials(state).err();
            return Err(match cleanup {
                Some(cleanup) => format!("{error}；{cleanup}"),
                None => error.to_string(),
            });
        }
        let _ = cleanup_pending_credentials(state);
    } else {
        state
            .database
            .save_provider_profile(&profile)
            .map_err(|error| error.to_string())?;
    }

    ProviderRegistry::specs(&state.database)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|provider| provider.id == input.provider_id)
        .ok_or_else(|| "供应商配置没有正确保存".into())
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_provider_key(
    provider_id: String,
    region: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    state
        .database
        .detach_provider_profile_and_queue_credential(&provider_id, &region)
        .map_err(|error| error.to_string())?;
    cleanup_pending_credentials(&state)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn test_provider_connection(
    provider_id: String,
    region: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let _ = cleanup_pending_credentials(&state);
    let profile = required_profile(&state, &provider_id, &region)?;
    let secret = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|error| error.to_string())?;
    let context = ProviderContext::from_registry(&provider_id, &region, &profile.model)
        .map_err(|error| error.to_string())?;
    let provider = adapter(&provider_id).map_err(|error| error.to_string())?;
    let result = provider
        .test_connection(&context, &secret, state.http.as_ref())
        .await;
    match result {
        Ok(()) => {
            state
                .database
                .set_provider_verified(&provider_id, &region, true)
                .map_err(|error| error.to_string())?;
            Ok("连接成功，可以生成知识点。".into())
        }
        Err(error) => {
            let _ = state
                .database
                .set_provider_verified(&provider_id, &region, false);
            Err(error.to_string())
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn generate_same_topic(
    card_id: String,
    state: State<'_, AppState>,
) -> CommandResult<KnowledgeCard> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    generate_same_topic_inner(&state, &card_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn generate_topic_batch(
    topic_id: Option<String>,
    topic_label: String,
    count: u32,
    state: State<'_, AppState>,
) -> CommandResult<GenerationBatchResult> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let requested = usize::try_from(count).map_err(|_| "生成数量无效".to_string())?;
    let (topic_id, topic_label) = state
        .database
        .resolve_generation_topic(topic_id.as_deref(), &topic_label)
        .map_err(|error| error.to_string())?;
    generate_topic_batch_inner(&state, topic_id, topic_label, requested).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn generate_random_topic_batch(
    count: u32,
    state: State<'_, AppState>,
) -> CommandResult<GenerationBatchResult> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let requested = usize::try_from(count).map_err(|_| "生成数量无效".to_string())?;
    let (topic_id, topic_label) = state
        .database
        .random_generation_topic()
        .map_err(|error| error.to_string())?;
    generate_topic_batch_inner(&state, topic_id, topic_label, requested).await
}

#[tauri::command]
pub async fn generate_random_topic(state: State<'_, AppState>) -> CommandResult<KnowledgeCard> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let (topic_id, topic_label) = state
        .database
        .random_generation_topic()
        .map_err(|error| error.to_string())?;
    generate_one_topic(&state, topic_id, topic_label).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ask_follow_up(
    card_id: String,
    question: String,
    display_question: Option<String>,
    history: Vec<FollowUpTurn>,
    state: State<'_, AppState>,
) -> CommandResult<FollowUpResponse> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    let stored_question =
        normalize_follow_up_text(&question, "追问", MAX_FOLLOW_UP_QUESTION_CHARS)?;
    let display_question = normalize_follow_up_text(
        display_question.as_deref().unwrap_or(&stored_question),
        "显示问题",
        MAX_FOLLOW_UP_QUESTION_CHARS,
    )?;
    let response = ask_follow_up_inner(&state, &card_id, stored_question.clone(), history).await?;
    state
        .database
        .save_follow_up_exchange(
            card_id.trim(),
            &display_question,
            &stored_question,
            &response,
        )
        .map_err(|error| error.to_string())?;
    Ok(response)
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_card_follow_ups(
    card_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<FollowUpMessage>> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let card_id = card_id.trim();
    if card_id.is_empty() || card_id.chars().count() > 128 || contains_disallowed_control(card_id) {
        return Err("当前知识卡不存在或已删除".into());
    }
    state
        .database
        .card_by_id(card_id)
        .map_err(|error| match error {
            DbError::NoCards => "当前知识卡不存在或已删除".into(),
            error => error.to_string(),
        })?;
    state
        .database
        .card_follow_ups(card_id)
        .map_err(|error| error.to_string())
}

async fn ask_follow_up_inner(
    state: &AppState,
    card_id: &str,
    question: String,
    history: Vec<FollowUpTurn>,
) -> CommandResult<FollowUpResponse> {
    let card_id = card_id.trim();
    if card_id.is_empty() || card_id.chars().count() > 128 || contains_disallowed_control(card_id) {
        return Err("当前知识卡不存在或已删除".into());
    }
    let question = normalize_follow_up_text(&question, "追问", MAX_FOLLOW_UP_QUESTION_CHARS)?;
    let history = normalize_follow_up_history(history)?;
    let card = match state.database.card_by_id(card_id) {
        Ok(card) => card,
        Err(DbError::NoCards) => return Err("当前知识卡不存在或已删除".into()),
        Err(error) => return Err(error.to_string()),
    };
    let plan = generation_profiles(state)?;
    let request = FollowUpRequest {
        topic_label: card.topic_label,
        card_question: card.question,
        short_answer: card.short_answer,
        explanation: card.explanation,
        why_it_matters: card.why_it_matters,
        question,
        history,
    };
    let mut errors = Vec::new();
    for profile in &plan.profiles {
        match ask_follow_up_with_profile(state, profile, &request).await {
            Ok(answer) => {
                let switched_from_provider_id =
                    (profile.provider_id != plan.preferred_id).then(|| plan.preferred_id.clone());
                return Ok(FollowUpResponse {
                    answer,
                    provider_id: profile.provider_id.clone(),
                    model: profile.model.clone(),
                    switched_from_provider_id,
                });
            }
            Err(error) => errors.push(format!(
                "{}追问失败：{}",
                provider_label(&profile.provider_id),
                error
            )),
        }
    }
    Err(errors.join("；"))
}

fn normalize_follow_up_history(history: Vec<FollowUpTurn>) -> CommandResult<Vec<FollowUpTurn>> {
    let skip = history.len().saturating_sub(MAX_FOLLOW_UP_HISTORY_TURNS);
    history
        .into_iter()
        .skip(skip)
        .map(|mut turn| {
            let (label, max_chars) = match turn.role {
                FollowUpRole::User => ("历史问题", MAX_FOLLOW_UP_QUESTION_CHARS),
                FollowUpRole::Assistant => ("历史回答", MAX_FOLLOW_UP_ANSWER_HISTORY_CHARS),
            };
            turn.content = normalize_follow_up_text(&turn.content, label, max_chars)?;
            Ok(turn)
        })
        .collect()
}

fn normalize_follow_up_text(value: &str, label: &str, max_chars: usize) -> CommandResult<String> {
    let value = value.trim();
    let length = value.chars().count();
    if length == 0 {
        return Err(format!("{label}不能为空"));
    }
    if length > max_chars {
        return Err(format!("{label}不能超过 {max_chars} 个字符"));
    }
    if contains_disallowed_control(value) {
        return Err(format!("{label}包含不支持的控制字符"));
    }
    Ok(value.to_string())
}

fn contains_disallowed_control(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

async fn ask_follow_up_with_profile(
    state: &AppState,
    profile: &ProviderProfileRecord,
    request: &FollowUpRequest,
) -> CommandResult<String> {
    if !profile.connection_verified {
        return Err("请先完成连接测试".into());
    }
    let secret = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|error| error.to_string())?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|error| error.to_string())?;
    let provider = adapter(&profile.provider_id).map_err(|error| error.to_string())?;
    match provider
        .answer_follow_up(&context, &secret, request, state.http.as_ref())
        .await
    {
        Ok(answer) => Ok(answer),
        Err(error) => {
            if error.code() == "invalid_key" {
                let _ = state.database.set_provider_verified(
                    &profile.provider_id,
                    &profile.region,
                    false,
                );
            }
            Err(error.to_string())
        }
    }
}

async fn generate_same_topic_inner(
    state: &AppState,
    card_id: &str,
) -> CommandResult<KnowledgeCard> {
    let source_card = state
        .database
        .card_by_id(card_id)
        .map_err(|error| error.to_string())?;
    generate_one_topic(state, source_card.topic_id, source_card.topic_label).await
}

async fn generate_topic_batch_inner(
    state: &AppState,
    topic_id: String,
    topic_label: String,
    requested: usize,
) -> CommandResult<GenerationBatchResult> {
    if requested == 0 {
        return Err("生成数量至少为 1 条".into());
    }
    let available = remaining_generation_slots(state)?;
    if requested > available {
        return Err(format!("今天还可生成 {available} 条，请减少本次生成数量"));
    }
    let plan = generation_profiles(state)?;
    let topic_override = Some((topic_id, topic_label.clone()));
    let mut accepted = Vec::with_capacity(requested);
    let mut notices = Vec::new();
    let mut profile_index = 0;
    let mut successful_provider_id = plan.profiles[0].provider_id.clone();
    let mut switched_from_provider_id = None;
    if successful_provider_id != plan.preferred_id {
        notices.push(format!(
            "{}未配置或未通过连接测试，已自动切换到{}",
            provider_label(&plan.preferred_id),
            provider_label(&successful_provider_id)
        ));
        switched_from_provider_id = Some(plan.preferred_id.clone());
    }

    while accepted.len() < requested {
        let profile = &plan.profiles[profile_index];
        let batch_limit = adapter(&profile.provider_id)
            .map_err(|error| error.to_string())?
            .capabilities()
            .max_cards_per_batch
            .max(1);
        let batch_size = generation_batch_sizes(requested - accepted.len(), batch_limit)
            .into_iter()
            .next()
            .expect("remaining generation count is non-zero");
        let generated = generate_and_store(
            state,
            profile,
            vec![topic_label.clone()],
            batch_size,
            topic_override.clone(),
        )
        .await;
        let error = match generated {
            Ok((cards, imported)) => {
                if imported == 0 {
                    "本批内容与已有知识点重复".to_string()
                } else {
                    successful_provider_id.clone_from(&profile.provider_id);
                    accepted.extend(
                        cards
                            .into_iter()
                            .filter_map(|card| state.database.card_by_id(&card.id).ok()),
                    );
                    continue;
                }
            }
            Err(error) => error,
        };

        if let Some(next_profile) = plan.profiles.get(profile_index + 1) {
            notices.push(format!(
                "{}生成失败，已自动切换到{}",
                provider_label(&profile.provider_id),
                provider_label(&next_profile.provider_id)
            ));
            switched_from_provider_id.get_or_insert_with(|| profile.provider_id.clone());
            profile_index += 1;
            continue;
        }
        if accepted.is_empty() {
            let prefix = (!notices.is_empty()).then(|| format!("{}；", notices.join("；")));
            return Err(format!(
                "{}{}也无法生成：{}",
                prefix.unwrap_or_default(),
                provider_label(&profile.provider_id),
                error
            ));
        }
        notices.push(format!(
            "{}无法完成剩余内容：{}",
            provider_label(&profile.provider_id),
            error
        ));
        break;
    }

    let first = accepted
        .first()
        .ok_or_else(|| "生成内容与已有知识点重复，请稍后再试".to_string())?;
    state
        .database
        .record_interaction(&first.id, "shown")
        .map_err(|error| error.to_string())?;
    if accepted.len() < requested && notices.is_empty() {
        notices.push("模型返回的可用知识点少于请求数量".to_string());
    }
    Ok(GenerationBatchResult {
        cards: accepted,
        requested_count: requested,
        warning: (!notices.is_empty()).then(|| notices.join("；")),
        provider_id: successful_provider_id,
        switched_from_provider_id,
    })
}

fn generation_batch_sizes(mut requested: usize, batch_limit: usize) -> Vec<usize> {
    let batch_limit = batch_limit.max(1);
    let mut batches = Vec::new();
    while requested > 0 {
        let batch = requested.min(batch_limit);
        batches.push(batch);
        requested -= batch;
    }
    batches
}

async fn generate_one_topic(
    state: &AppState,
    topic_id: String,
    topic_label: String,
) -> CommandResult<KnowledgeCard> {
    remaining_generation_slots(state)?;
    let plan = generation_profiles(state)?;
    let topic_override = Some((topic_id, topic_label.clone()));
    let mut errors = Vec::new();
    for profile in &plan.profiles {
        match generate_and_store(
            state,
            profile,
            vec![topic_label.clone()],
            1,
            topic_override.clone(),
        )
        .await
        {
            Ok((mut cards, imported)) if imported > 0 => {
                let card = cards
                    .pop()
                    .ok_or_else(|| "模型没有返回可用的知识点".to_string())?;
                state
                    .database
                    .record_interaction(&card.id, "shown")
                    .map_err(|error| error.to_string())?;
                return Ok(card);
            }
            Ok(_) => errors.push(format!(
                "{}生成内容与已有知识点重复",
                provider_label(&profile.provider_id)
            )),
            Err(error) => errors.push(format!(
                "{}生成失败：{}",
                provider_label(&profile.provider_id),
                error
            )),
        }
    }
    Err(errors.join("；"))
}

fn remaining_generation_slots(state: &AppState) -> CommandResult<usize> {
    let settings = state
        .database
        .settings()
        .map_err(|error| error.to_string())?;
    let generated_today = state
        .database
        .generated_today()
        .map_err(|error| error.to_string())?;
    if settings.daily_generation_limit == 0 {
        return Err("请先在通用设置中填写生成数量上限".into());
    }
    if generated_today >= settings.daily_generation_limit {
        return Err("已达到今天的模型生成上限".into());
    }
    Ok((settings.daily_generation_limit - generated_today) as usize)
}

struct GenerationProfiles {
    preferred_id: String,
    profiles: Vec<ProviderProfileRecord>,
}

fn generation_profiles(state: &AppState) -> CommandResult<GenerationProfiles> {
    let preferred_id = state
        .database
        .generation_provider_id()
        .map_err(|error| error.to_string())?;
    let profiles = state
        .database
        .provider_profiles()
        .map_err(|error| error.to_string())?;
    let has_configured = profiles.iter().any(|profile| profile.key_last4.is_some());
    let ordered = ordered_ready_profiles(profiles, &preferred_id);
    if ordered.is_empty() {
        return Err(if has_configured {
            "请先完成至少一个模型的连接测试".into()
        } else {
            "请先配置模型哦~".into()
        });
    }
    Ok(GenerationProfiles {
        preferred_id,
        profiles: ordered,
    })
}

fn ordered_ready_profiles(
    profiles: Vec<ProviderProfileRecord>,
    preferred_id: &str,
) -> Vec<ProviderProfileRecord> {
    let mut ready = profiles
        .into_iter()
        .filter(|profile| profile.key_last4.is_some() && profile.connection_verified)
        .collect::<Vec<_>>();
    ready.sort_by_key(|profile| usize::from(profile.provider_id != preferred_id));
    ready
}

fn provider_label(provider_id: &str) -> &'static str {
    match provider_id {
        "deepseek" => "DeepSeek",
        "kimi" => "Kimi",
        _ => "所选模型",
    }
}

async fn generate_and_store(
    state: &AppState,
    profile: &ProviderProfileRecord,
    topics: Vec<String>,
    count: usize,
    topic_override: Option<(String, String)>,
) -> CommandResult<(Vec<KnowledgeCard>, usize)> {
    if !profile.connection_verified {
        return Err("请先完成连接测试".into());
    }
    let secret = state
        .secrets
        .get(&profile.credential_ref)
        .map_err(|error| error.to_string())?;
    let context =
        ProviderContext::from_registry(&profile.provider_id, &profile.region, &profile.model)
            .map_err(|error| error.to_string())?;
    let provider = adapter(&profile.provider_id).map_err(|error| error.to_string())?;
    if count > provider.capabilities().max_cards_per_batch {
        return Err("单批生成数量超过供应商适配器限制".into());
    }
    let job_id = Uuid::new_v4().to_string();
    state
        .database
        .start_generation_job(&job_id, &profile.provider_id, count)
        .map_err(|error| error.to_string())?;
    let generated = provider
        .generate_knowledge_cards(
            &context,
            &secret,
            &GenerationRequest { topics, count },
            state.http.as_ref(),
        )
        .await;
    match generated {
        Ok(mut cards) => {
            cards.truncate(count);
            if let Some((topic_id, topic_label)) = topic_override {
                for card in &mut cards {
                    card.topic_id.clone_from(&topic_id);
                    card.topic_label.clone_from(&topic_label);
                }
            }
            let imported = state
                .database
                .insert_cards(&cards, false, true)
                .map_err(|error| error.to_string())?;
            state
                .database
                .finish_generation_job(&job_id, imported.imported, None)
                .map_err(|error| error.to_string())?;
            Ok((cards, imported.imported))
        }
        Err(error) => {
            let _ = state
                .database
                .finish_generation_job(&job_id, 0, Some(error.code()));
            if error.code() == "invalid_key" {
                let _ = state.database.set_provider_verified(
                    &profile.provider_id,
                    &profile.region,
                    false,
                );
            }
            Err(error.to_string())
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn import_cards_file(path: String, state: State<'_, AppState>) -> CommandResult<ImportResult> {
    let _operation = state
        .persistence_gate
        .try_operation()
        .map_err(str::to_owned)?;
    let path = Path::new(&path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "文件扩展名无效".to_string())?;
    if !matches!(extension.to_ascii_lowercase().as_str(), "json" | "csv") {
        return Err("只能导入 JSON 或 CSV 文件".into());
    }
    let metadata = fs::metadata(path).map_err(|_| "无法读取所选文件".to_string())?;
    if metadata.len() > 5 * 1024 * 1024 {
        return Err("导入文件不能超过 5 MB".into());
    }
    let bytes = fs::read(path).map_err(|_| "无法读取所选文件".to_string())?;
    let cards = parse_import(extension, &bytes).map_err(|error| error.to_string())?;
    state
        .database
        .insert_cards(&cards, false, false)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_data(
    scope: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let _reset = state.persistence_gate.try_reset().map_err(str::to_owned)?;
    let _lock = GenerationLock::acquire(&state.generation_in_progress)?;
    state
        .database
        .clear_data(&scope)
        .map_err(|error| error.to_string())?;
    if scope == "all" {
        state.auto_hide.set_enabled(false);
        let system_result = restore_default_system_integrations(&app);
        let credential_result = cleanup_pending_credentials(&state);
        return Ok(match (system_result, credential_result) {
            (Ok(()), Ok(())) => None,
            (Err(system), Ok(())) => Some(system),
            (Ok(()), Err(credentials)) => Some(format!("本地数据已清除，但{credentials}")),
            (Err(system), Err(credentials)) => Some(format!("{system}；同时{credentials}")),
        });
    }
    Ok(None)
}

fn restore_default_system_integrations(app: &AppHandle) -> CommandResult<()> {
    let defaults = AppSettings::default();
    let mut failures = Vec::new();

    if app.autolaunch().disable().is_err() {
        failures.push("关闭开机启动");
    }

    if app.global_shortcut().unregister_all().is_err() {
        failures.push("取消旧全局快捷键");
    } else if app
        .global_shortcut()
        .register(defaults.global_shortcut.as_str())
        .is_err()
    {
        failures.push("注册默认全局快捷键");
    }

    match app.get_webview_window("main") {
        Some(window) if window.set_always_on_top(false).is_err() => {
            failures.push("关闭窗口置顶");
        }
        None => failures.push("恢复主窗口置顶状态"),
        _ => {}
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "本地数据已清除，但以下系统设置未能恢复默认值：{}",
            failures.join("、")
        ))
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn open_source_url(url: String, app: AppHandle) -> CommandResult<()> {
    let parsed = Url::parse(&url).map_err(|_| "来源链接无效".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("只允许打开 HTTPS 来源链接".into());
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|_| "无法打开来源链接".to_string())
}

#[tauri::command]
pub fn exit_application(app: AppHandle, state: State<'_, AppState>) {
    let Ok(_operation) = state.persistence_gate.try_operation() else {
        return;
    };
    state
        .exiting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

fn required_profile(
    state: &AppState,
    provider_id: &str,
    region: &str,
) -> CommandResult<ProviderProfileRecord> {
    state
        .database
        .provider_profile(provider_id, region)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "请先保存供应商配置".into())
}

#[cfg(test)]
mod tests {
    use super::{
        ask_follow_up_inner, cleanup_pending_credentials, generate_topic_batch_inner,
        generation_batch_sizes, ordered_ready_profiles,
        require_provider_key_replacement_confirmation, save_provider_profile_inner, GenerationLock,
        MODEL_OPERATION_IN_PROGRESS_ERROR, PROVIDER_KEY_REPLACEMENT_CONFIRMATION_REQUIRED,
    };
    use crate::db::ProviderProfileRecord;
    use crate::models::{FollowUpRole, FollowUpTurn, SaveProviderInput};
    use crate::providers::{ProviderError, ProviderTransport, TransportResponse};
    use crate::secret_store::tests_support::MemorySecretStore;
    use crate::secret_store::{SecretError, SecretStore, SecretValue};
    use crate::AppState;
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration as StdDuration;
    use url::Url;

    #[derive(Clone, Copy)]
    enum DeleteBehavior {
        Succeed,
        FailBeforeDelete,
        FailAfterDelete,
    }

    struct FaultInjectingSecretStore {
        inner: MemorySecretStore,
        delete_behaviors: Mutex<VecDeque<DeleteBehavior>>,
    }

    impl FaultInjectingSecretStore {
        fn new(delete_behaviors: impl IntoIterator<Item = DeleteBehavior>) -> Self {
            Self {
                inner: MemorySecretStore::default(),
                delete_behaviors: Mutex::new(delete_behaviors.into_iter().collect()),
            }
        }
    }

    impl SecretStore for FaultInjectingSecretStore {
        fn save(&self, credential_ref: &str, secret: &str) -> Result<(), SecretError> {
            self.inner.save(credential_ref, secret)
        }

        fn get(&self, credential_ref: &str) -> Result<SecretValue, SecretError> {
            self.inner.get(credential_ref)
        }

        fn delete(&self, credential_ref: &str) -> Result<(), SecretError> {
            let behavior = self
                .delete_behaviors
                .lock()
                .map_err(|_| SecretError::Unavailable)?
                .pop_front()
                .unwrap_or(DeleteBehavior::Succeed);
            match behavior {
                DeleteBehavior::Succeed => self.inner.delete(credential_ref),
                DeleteBehavior::FailBeforeDelete => Err(SecretError::Unavailable),
                DeleteBehavior::FailAfterDelete => {
                    self.inner.delete(credential_ref)?;
                    Err(SecretError::Unavailable)
                }
            }
        }
    }

    fn test_state(database: crate::db::Database, secrets: Arc<dyn SecretStore>) -> AppState {
        AppState {
            database,
            secrets,
            http: Arc::new(FailoverTransport {
                hosts: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
            }),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        }
    }

    #[test]
    fn generation_count_is_split_by_provider_batch_limit() {
        assert_eq!(generation_batch_sizes(12, 5), vec![5, 5, 2]);
        assert_eq!(generation_batch_sizes(6, 2), vec![2, 2, 2]);
        assert_eq!(generation_batch_sizes(1, 5), vec![1]);
        assert!(generation_batch_sizes(0, 5).is_empty());
    }

    #[test]
    fn model_operations_share_one_exclusive_lock() {
        let flag = AtomicBool::new(false);
        let lock = GenerationLock::acquire(&flag).expect("first model operation lock");
        assert_eq!(
            GenerationLock::acquire(&flag)
                .err()
                .expect("concurrent operation is rejected"),
            MODEL_OPERATION_IN_PROGRESS_ERROR
        );
        drop(lock);
        assert!(GenerationLock::acquire(&flag).is_ok());
    }

    #[test]
    fn provider_key_replacement_requires_explicit_confirmation() {
        assert_eq!(
            require_provider_key_replacement_confirmation(true, true, false)
                .expect_err("replacement without confirmation is rejected"),
            PROVIDER_KEY_REPLACEMENT_CONFIRMATION_REQUIRED
        );
        assert!(require_provider_key_replacement_confirmation(true, true, true).is_ok());
        assert!(require_provider_key_replacement_confirmation(false, true, false).is_ok());
        assert!(require_provider_key_replacement_confirmation(true, false, false).is_ok());
    }

    #[test]
    fn preferred_ready_provider_is_first_and_unverified_profiles_are_skipped() {
        let profile = |provider_id: &str, verified: bool| ProviderProfileRecord {
            provider_id: provider_id.into(),
            region: if provider_id == "kimi" {
                "cn".into()
            } else {
                "default".into()
            },
            model: "mock-model".into(),
            credential_ref: format!("credential-{provider_id}"),
            key_last4: Some("1234".into()),
            connection_verified: verified,
        };
        let ordered = ordered_ready_profiles(
            vec![
                profile("deepseek", true),
                profile("kimi", true),
                profile("unverified", false),
            ],
            "kimi",
        );
        assert_eq!(
            ordered
                .iter()
                .map(|item| item.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kimi", "deepseek"]
        );
    }

    struct FailoverTransport {
        hosts: Mutex<Vec<String>>,
        bodies: Mutex<Vec<Value>>,
    }

    #[async_trait::async_trait]
    impl ProviderTransport for FailoverTransport {
        async fn post_json(
            &self,
            endpoint: &Url,
            _api_key: &SecretValue,
            body: &Value,
            _timeout: StdDuration,
        ) -> Result<TransportResponse, ProviderError> {
            let host = endpoint.host_str().unwrap_or_default().to_string();
            self.hosts.lock().expect("host lock").push(host.clone());
            self.bodies.lock().expect("body lock").push(body.clone());
            if host == "api.deepseek.com" {
                return Err(ProviderError::Unavailable);
            }
            if body.get("response_format").is_none() {
                return Ok(TransportResponse {
                    status: 200,
                    body: json!({
                        "choices": [{
                            "finish_reason": "stop",
                            "message": { "content": "这是一个补充回答。" }
                        }]
                    })
                    .to_string(),
                });
            }
            let generated = json!({
                "cards": [{
                    "topicId": "natural_science",
                    "topicLabel": "自然科学",
                    "tags": ["水", "晶体"],
                    "question": "为什么水结冰以后体积反而会变得更大？",
                    "shortAnswer": "水分子结冰时会形成带有规则空隙的晶体结构，所以同样质量的冰会占据更大的体积，密度也因此低于液态水并通常浮在水面。",
                    "explanation": "液态水中的分子仍可移动并相对紧密地排列。温度下降到冰点附近时，氢键把水分子固定到带有规则空隙的晶格中。晶格占据的空间更大，因此水结冰时体积增加，密度也低于液态水。这个现象同时解释了冰为什么通常会浮在水面，也影响了寒冷地区的岩石风化与水体生态。水的密度还会随温度改变，实际结冰过程也会受到溶质、压力和成核条件影响，因此这个规律需要在具体环境中理解。此外，水分子排列并非瞬间完成，冷却速度也会影响晶体形成方式。",
                    "whyItMatters": "这会影响湖泊结冰方式和寒冷地区的自然环境。",
                    "difficulty": "beginner",
                    "estimatedReadSeconds": 50
                }]
            });
            Ok(TransportResponse {
                status: 200,
                body: json!({
                    "choices": [{
                        "finish_reason": "stop",
                        "message": { "content": generated.to_string() }
                    }]
                })
                .to_string(),
            })
        }
    }

    #[test]
    fn confirmed_provider_key_replacement_switches_refs_before_removing_the_old_secret() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = crate::db::Database::new(directory.path().join("key-replacement.db"));
        database.initialize().expect("database");
        let old_credential_ref = "deepseek:default:old";
        database
            .save_provider_profile(&ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: old_credential_ref.into(),
                key_last4: Some("-old".into()),
                connection_verified: true,
            })
            .expect("old provider profile");
        let secrets = Arc::new(MemorySecretStore::default());
        secrets
            .save(old_credential_ref, "sk-secret-old")
            .expect("old secret");
        let state = AppState {
            database,
            secrets: secrets.clone(),
            http: Arc::new(FailoverTransport {
                hosts: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
            }),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };
        let input = |replace_existing_key| SaveProviderInput {
            provider_id: "deepseek".into(),
            region: "default".into(),
            model: "deepseek-v4-flash".into(),
            api_key: Some("sk-secret-new".into()),
            replace_existing_key,
        };

        assert_eq!(
            save_provider_profile_inner(input(false), &state).expect_err("unconfirmed replacement"),
            PROVIDER_KEY_REPLACEMENT_CONFIRMATION_REQUIRED
        );
        assert_eq!(
            state
                .database
                .provider_profile("deepseek", "default")
                .expect("profile lookup")
                .expect("existing profile")
                .credential_ref,
            old_credential_ref
        );
        assert_eq!(
            secrets
                .get(old_credential_ref)
                .expect("old secret is untouched")
                .expose(),
            "sk-secret-old"
        );

        save_provider_profile_inner(input(true), &state).expect("confirmed replacement");
        let replaced = state
            .database
            .provider_profile("deepseek", "default")
            .expect("profile lookup")
            .expect("replaced profile");
        assert_ne!(replaced.credential_ref, old_credential_ref);
        assert!(replaced.credential_ref.starts_with("deepseek:default:"));
        assert_eq!(
            secrets
                .get(&replaced.credential_ref)
                .expect("new secret")
                .expose(),
            "sk-secret-new"
        );
        assert!(matches!(
            secrets.get(old_credential_ref),
            Err(SecretError::NotFound)
        ));
        assert!(state
            .database
            .pending_credential_deletions()
            .expect("cleanup queue")
            .is_empty());
    }

    #[test]
    fn ambiguous_old_key_deletion_keeps_new_key_active_and_retriable() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = crate::db::Database::new(directory.path().join("ambiguous-delete.db"));
        database.initialize().expect("database");
        let old_credential_ref = "deepseek:default:old";
        database
            .save_provider_profile(&ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: old_credential_ref.into(),
                key_last4: Some("-old".into()),
                connection_verified: true,
            })
            .expect("old profile");
        let secrets = Arc::new(FaultInjectingSecretStore::new([
            DeleteBehavior::FailAfterDelete,
        ]));
        secrets
            .save(old_credential_ref, "sk-secret-old")
            .expect("old secret");
        let state = test_state(database, secrets.clone());

        save_provider_profile_inner(
            SaveProviderInput {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                api_key: Some("sk-secret-new".into()),
                replace_existing_key: true,
            },
            &state,
        )
        .expect("replacement remains successful");

        let active = state
            .database
            .provider_profile("deepseek", "default")
            .expect("profile lookup")
            .expect("active profile");
        assert_ne!(active.credential_ref, old_credential_ref);
        assert_eq!(
            secrets
                .get(&active.credential_ref)
                .expect("new active secret")
                .expose(),
            "sk-secret-new"
        );
        assert!(matches!(
            secrets.get(old_credential_ref),
            Err(SecretError::NotFound)
        ));
        assert_eq!(
            state
                .database
                .pending_credential_deletions()
                .expect("pending cleanup"),
            vec![old_credential_ref.to_string()]
        );

        cleanup_pending_credentials(&state).expect("retry observes missing old key as deleted");
        assert!(state
            .database
            .pending_credential_deletions()
            .expect("cleanup queue after retry")
            .is_empty());
        assert_eq!(
            secrets
                .get(&active.credential_ref)
                .expect("new key remains active")
                .expose(),
            "sk-secret-new"
        );
    }

    #[test]
    fn failed_profile_activation_and_failed_temp_cleanup_preserve_old_key_and_queue_temp_ref() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database_path = directory.path().join("failed-activation.db");
        let database = crate::db::Database::new(database_path.clone());
        database.initialize().expect("database");
        let old_credential_ref = "deepseek:default:old";
        database
            .save_provider_profile(&ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: old_credential_ref.into(),
                key_last4: Some("-old".into()),
                connection_verified: true,
            })
            .expect("old profile");
        let connection = rusqlite::Connection::open(&database_path).expect("failure connection");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_provider_profile_activation
                 BEFORE UPDATE OF credential_ref ON provider_profiles
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated profile activation failure');
                 END;",
            )
            .expect("activation failure trigger");
        drop(connection);
        let secrets = Arc::new(FaultInjectingSecretStore::new([
            DeleteBehavior::FailBeforeDelete,
        ]));
        secrets
            .save(old_credential_ref, "sk-secret-old")
            .expect("old secret");
        let state = test_state(database, secrets.clone());

        save_provider_profile_inner(
            SaveProviderInput {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                api_key: Some("sk-secret-new".into()),
                replace_existing_key: true,
            },
            &state,
        )
        .expect_err("profile activation fails");

        let active = state
            .database
            .provider_profile("deepseek", "default")
            .expect("profile lookup")
            .expect("old profile remains active");
        assert_eq!(active.credential_ref, old_credential_ref);
        assert_eq!(
            secrets
                .get(old_credential_ref)
                .expect("old active key")
                .expose(),
            "sk-secret-old"
        );
        let pending = state
            .database
            .pending_credential_deletions()
            .expect("temporary cleanup ref");
        assert_eq!(pending.len(), 1);
        assert_ne!(pending[0], old_credential_ref);
        assert_eq!(
            secrets
                .get(&pending[0])
                .expect("temporary key remains locatable")
                .expose(),
            "sk-secret-new"
        );

        cleanup_pending_credentials(&state).expect("later cleanup succeeds");
        assert!(matches!(
            secrets.get(&pending[0]),
            Err(SecretError::NotFound)
        ));
        assert!(state
            .database
            .pending_credential_deletions()
            .expect("queue after cleanup")
            .is_empty());
        assert_eq!(
            secrets
                .get(old_credential_ref)
                .expect("old key remains after cleanup")
                .expose(),
            "sk-secret-old"
        );
    }

    #[test]
    fn delete_and_clear_keep_failed_secret_deletions_queued_without_dangling_profiles() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = crate::db::Database::new(directory.path().join("queued-deletions.db"));
        database.initialize().expect("database");
        for (provider_id, region, credential_ref) in [
            ("deepseek", "default", "cleanup:a"),
            ("kimi", "cn", "cleanup:b"),
        ] {
            database
                .save_provider_profile(&ProviderProfileRecord {
                    provider_id: provider_id.into(),
                    region: region.into(),
                    model: "mock-model".into(),
                    credential_ref: credential_ref.into(),
                    key_last4: Some("mock".into()),
                    connection_verified: true,
                })
                .expect("provider profile");
        }
        let secrets = Arc::new(FaultInjectingSecretStore::new([
            DeleteBehavior::FailBeforeDelete,
            DeleteBehavior::Succeed,
            DeleteBehavior::FailBeforeDelete,
        ]));
        secrets
            .save("cleanup:a", "sk-secret-a")
            .expect("first secret");
        secrets
            .save("cleanup:b", "sk-secret-b")
            .expect("second secret");
        let state = test_state(database, secrets.clone());

        state
            .database
            .detach_provider_profile_and_queue_credential("deepseek", "default")
            .expect("profile detaches atomically");
        cleanup_pending_credentials(&state).expect_err("single secret deletion is deferred");
        assert!(state
            .database
            .provider_profile("deepseek", "default")
            .expect("detached profile lookup")
            .is_none());
        assert_eq!(
            secrets
                .get("cleanup:a")
                .expect("failed deletion leaves key for retry")
                .expose(),
            "sk-secret-a"
        );
        assert_eq!(
            state
                .database
                .pending_credential_deletions()
                .expect("single pending cleanup"),
            vec!["cleanup:a".to_string()]
        );

        cleanup_pending_credentials(&state).expect("single deletion retry");
        assert!(matches!(
            secrets.get("cleanup:a"),
            Err(SecretError::NotFound)
        ));

        state.database.clear_data("all").expect("database clear");
        cleanup_pending_credentials(&state).expect_err("second clear deletion is deferred");
        assert!(state
            .database
            .provider_profiles()
            .expect("profiles after clear")
            .is_empty());
        assert_eq!(
            secrets
                .get("cleanup:b")
                .expect("failed nth deletion remains for retry")
                .expose(),
            "sk-secret-b"
        );
        assert_eq!(
            state
                .database
                .pending_credential_deletions()
                .expect("pending nth deletion"),
            vec!["cleanup:b".to_string()]
        );

        cleanup_pending_credentials(&state).expect("nth deletion retry");
        assert!(matches!(
            secrets.get("cleanup:b"),
            Err(SecretError::NotFound)
        ));
        assert!(state
            .database
            .pending_credential_deletions()
            .expect("empty cleanup queue")
            .is_empty());
    }

    #[test]
    fn generation_falls_back_once_to_the_other_ready_provider() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = crate::db::Database::new(directory.path().join("failover.db"));
        database.initialize().expect("database");
        database
            .save_generation_provider_id("deepseek")
            .expect("preferred provider");
        for profile in [
            ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: "deepseek:default".into(),
                key_last4: Some("mock".into()),
                connection_verified: true,
            },
            ProviderProfileRecord {
                provider_id: "kimi".into(),
                region: "cn".into(),
                model: "kimi-k3".into(),
                credential_ref: "kimi:cn".into(),
                key_last4: Some("mock".into()),
                connection_verified: true,
            },
        ] {
            database
                .save_provider_profile(&profile)
                .expect("provider profile");
        }
        let secrets = Arc::new(MemorySecretStore::default());
        secrets
            .save("deepseek:default", "sk-deepseek-mock")
            .expect("deepseek secret");
        secrets
            .save("kimi:cn", "sk-kimi-mock")
            .expect("kimi secret");
        let transport = Arc::new(FailoverTransport {
            hosts: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
        });
        let state = AppState {
            database,
            secrets,
            http: transport.clone(),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };

        let result = tauri::async_runtime::block_on(generate_topic_batch_inner(
            &state,
            "natural_science".into(),
            "自然科学".into(),
            1,
        ))
        .expect("fallback generation");

        assert_eq!(result.cards.len(), 1);
        assert_eq!(result.provider_id, "kimi");
        assert_eq!(
            result.switched_from_provider_id.as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            transport.hosts.lock().expect("host lock").as_slice(),
            ["api.deepseek.com", "api.moonshot.cn"]
        );
    }

    #[test]
    fn follow_up_uses_card_context_and_falls_back_without_counting_generation() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = crate::db::Database::new(directory.path().join("follow-up-failover.db"));
        database.initialize().expect("database");
        database
            .save_generation_provider_id("deepseek")
            .expect("preferred provider");
        let card = database.next_card(None).expect("seed card");
        for profile in [
            ProviderProfileRecord {
                provider_id: "deepseek".into(),
                region: "default".into(),
                model: "deepseek-v4-flash".into(),
                credential_ref: "deepseek:default".into(),
                key_last4: Some("mock".into()),
                connection_verified: true,
            },
            ProviderProfileRecord {
                provider_id: "kimi".into(),
                region: "cn".into(),
                model: "kimi-k3".into(),
                credential_ref: "kimi:cn".into(),
                key_last4: Some("mock".into()),
                connection_verified: true,
            },
        ] {
            database
                .save_provider_profile(&profile)
                .expect("provider profile");
        }
        let secrets = Arc::new(MemorySecretStore::default());
        secrets
            .save("deepseek:default", "sk-deepseek-mock")
            .expect("deepseek secret");
        secrets
            .save("kimi:cn", "sk-kimi-mock")
            .expect("kimi secret");
        let transport = Arc::new(FailoverTransport {
            hosts: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
        });
        let state = AppState {
            database,
            secrets,
            http: transport.clone(),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };
        let history = (0..8)
            .map(|index| FollowUpTurn {
                role: FollowUpRole::User,
                content: format!("历史问题 {index}"),
            })
            .collect();

        let result = tauri::async_runtime::block_on(ask_follow_up_inner(
            &state,
            &card.id,
            "还能举个例子吗？".into(),
            history,
        ))
        .expect("fallback follow-up");

        assert_eq!(result.answer, "这是一个补充回答。");
        assert_eq!(result.provider_id, "kimi");
        assert_eq!(result.model, "kimi-k3");
        assert_eq!(
            result.switched_from_provider_id.as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            transport.hosts.lock().expect("host lock").as_slice(),
            ["api.deepseek.com", "api.moonshot.cn"]
        );
        assert_eq!(state.database.generated_today().expect("usage"), 0);
        let bodies = transport.bodies.lock().expect("body lock");
        let payload: Value = serde_json::from_str(
            bodies[1]["messages"][1]["content"]
                .as_str()
                .expect("follow-up context"),
        )
        .expect("context JSON");
        let retained = payload["recentHistory"].as_array().expect("history");
        assert_eq!(retained.len(), 6);
        assert_eq!(retained[0]["content"], "历史问题 2");
        assert_eq!(payload["knowledgeCard"]["question"], card.question);
        assert_eq!(payload["currentQuestion"], "还能举个例子吗？");
    }
}
