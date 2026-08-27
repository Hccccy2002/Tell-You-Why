use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustStatus {
    Verified,
    SourceGrounded,
    AiUnverified,
    DemoUnreviewed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Beginner,
    General,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub accessed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedBy {
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCard {
    pub id: String,
    pub schema_version: i64,
    pub language: String,
    pub topic_id: String,
    pub topic_label: String,
    pub tags: Vec<String>,
    pub question: String,
    pub short_answer: String,
    pub explanation: String,
    #[serde(default)]
    pub why_it_matters: Option<String>,
    pub difficulty: Difficulty,
    pub estimated_read_seconds: i64,
    #[serde(default)]
    pub source_refs: Vec<SourceRef>,
    pub trust_status: TrustStatus,
    pub content_fingerprint: String,
    #[serde(default)]
    pub generated_by: Option<GeneratedBy>,
    pub created_at: String,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub hidden_from_feed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicPreference {
    pub id: String,
    pub label: String,
    pub selected: bool,
    pub enabled: bool,
    pub custom: bool,
    pub rank: i64,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReminderPreset {
    Manual,
    WeekdayOnce,
    WeekdayTwice,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub always_on_top: bool,
    pub autostart: bool,
    pub personalization_enabled: bool,
    pub reminder_preset: ReminderPreset,
    pub reminder_times: Vec<String>,
    pub weekdays: Vec<u32>,
    pub quiet_start: String,
    pub quiet_end: String,
    pub pause_until: Option<String>,
    pub global_shortcut: String,
    pub daily_generation_limit: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            always_on_top: false,
            autostart: false,
            personalization_enabled: true,
            reminder_preset: ReminderPreset::Manual,
            reminder_times: vec!["11:00".to_string()],
            weekdays: vec![1, 2, 3, 4, 5],
            quiet_start: "18:00".to_string(),
            quiet_end: "09:00".to_string(),
            pause_until: None,
            global_shortcut: "Alt+Shift+Y".to_string(),
            daily_generation_limit: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingInput {
    pub selected_topic_ids: Vec<String>,
    pub custom_interests: Vec<String>,
    pub reminder_preset: ReminderPreset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub card: KnowledgeCard,
    pub viewed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub id: String,
    pub label: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRegion {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    pub id: String,
    pub label: String,
    pub regions: Vec<ProviderRegion>,
    pub models: Vec<ProviderModel>,
    pub selected_region: String,
    pub selected_model: String,
    pub key_configured: bool,
    pub key_last4: Option<String>,
    pub connection_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCapabilities {
    pub tray: bool,
    pub notifications: bool,
    pub secure_credentials: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub onboarding_complete: bool,
    pub card: Option<KnowledgeCard>,
    pub available_card_count: u32,
    pub topics: Vec<TopicPreference>,
    pub settings: AppSettings,
    pub providers: Vec<ProviderSpec>,
    pub generation_provider_id: String,
    pub desktop_capabilities: DesktopCapabilities,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderInput {
    pub provider_id: String,
    pub region: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationUsage {
    pub generated: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationBatchResult {
    pub cards: Vec<KnowledgeCard>,
    pub requested_count: usize,
    pub warning: Option<String>,
    pub provider_id: String,
    pub switched_from_provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub monitor_name: Option<String>,
}
