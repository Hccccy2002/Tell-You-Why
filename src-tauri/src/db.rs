use crate::content::{fingerprint, questions_are_similar, validate_card};
use crate::models::{
    AppSettings, Difficulty, ImportResult, KnowledgeCard, LibraryItem, OnboardingInput,
    TopicPreference, TrustStatus, WindowState,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

const CARD_COLUMNS: &str = "c.id, c.schema_version, c.language, c.topic_id, c.topic_label,
    c.tags_json, c.question, c.short_answer, c.explanation, c.why_it_matters,
    c.difficulty, c.estimated_read_seconds, c.source_refs_json, c.trust_status,
    c.content_fingerprint, c.generated_by_json, c.created_at,
    COALESCE(s.favorite, 0), COALESCE(s.hidden, 0)";

const PRESET_TOPICS: &[(&str, &str)] = &[
    ("natural_science", "自然科学"),
    ("space_earth", "宇宙与地球"),
    ("history_civilization", "历史与文明"),
    ("language_writing", "语言与文字"),
    ("computing_internet", "计算机与互联网"),
    ("business_economics", "商业与经济常识"),
    ("daily_principles", "日常生活原理"),
    ("arts_culture", "艺术与文化"),
];

#[derive(Debug, Error)]
pub enum DbError {
    #[error("本地数据库暂时不可用")]
    Sql(#[from] rusqlite::Error),
    #[error("本地数据目录无法访问")]
    Io(#[from] std::io::Error),
    #[error("本地数据格式异常")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Validation(String),
    #[error("没有可显示的本地知识卡")]
    NoCards,
}

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfileRecord {
    pub provider_id: String,
    pub region: String,
    pub model: String,
    pub credential_ref: String,
    pub key_last4: Option<String>,
    pub connection_verified: bool,
}

impl Database {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn initialize(&self) -> Result<(), DbError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut connection = self.connect()?;
        self.migrate(&mut connection)?;
        self.seed(&mut connection)?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection, DbError> {
        let connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.busy_timeout(std::time::Duration::from_secs(3))?;
        Ok(connection)
    }

    fn migrate(&self, connection: &mut Connection) -> Result<(), DbError> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;
        let version: i64 = connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        if version < 1 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE cards (
                    id TEXT PRIMARY KEY,
                    schema_version INTEGER NOT NULL,
                    language TEXT NOT NULL,
                    topic_id TEXT NOT NULL,
                    topic_label TEXT NOT NULL,
                    tags_json TEXT NOT NULL,
                    question TEXT NOT NULL,
                    short_answer TEXT NOT NULL,
                    explanation TEXT NOT NULL,
                    why_it_matters TEXT,
                    difficulty TEXT NOT NULL,
                    estimated_read_seconds INTEGER NOT NULL,
                    source_refs_json TEXT NOT NULL,
                    trust_status TEXT NOT NULL,
                    content_fingerprint TEXT NOT NULL UNIQUE,
                    generated_by_json TEXT,
                    created_at TEXT NOT NULL,
                    built_in INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX idx_cards_topic ON cards(topic_id);
                CREATE TABLE interactions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                    kind TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE INDEX idx_interactions_card_kind ON interactions(card_id, kind, created_at DESC);
                CREATE TABLE card_user_state (
                    card_id TEXT PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
                    liked INTEGER NOT NULL DEFAULT 0,
                    disliked INTEGER NOT NULL DEFAULT 0,
                    known INTEGER NOT NULL DEFAULT 0,
                    favorite INTEGER NOT NULL DEFAULT 0,
                    hidden INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE topic_preferences (
                    topic_id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    selected INTEGER NOT NULL DEFAULT 0,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    custom INTEGER NOT NULL DEFAULT 0,
                    explicit_rank INTEGER NOT NULL DEFAULT 0,
                    weight INTEGER NOT NULL DEFAULT 0,
                    cooldown_until TEXT
                );
                CREATE TABLE provider_profiles (
                    provider_id TEXT NOT NULL,
                    region TEXT NOT NULL,
                    model TEXT NOT NULL,
                    credential_ref TEXT NOT NULL,
                    key_last4 TEXT,
                    connection_verified INTEGER NOT NULL DEFAULT 0,
                    auto_generate_enabled INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(provider_id, region)
                );
                CREATE TABLE generation_jobs (
                    id TEXT PRIMARY KEY,
                    provider_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    requested_count INTEGER NOT NULL,
                    generated_count INTEGER NOT NULL DEFAULT 0,
                    error_code TEXT,
                    retry_at TEXT,
                    created_at TEXT NOT NULL,
                    finished_at TEXT
                );
                CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO schema_migrations(version, applied_at)
                VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }
        if version < 2 {
            let transaction = connection.transaction()?;
            transaction.execute(
                "ALTER TABLE cards ADD COLUMN cache_managed INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at)
                 VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )?;
            transaction.commit()?;
        }
        if version < 3 {
            let transaction = connection.transaction()?;
            transaction.execute(
                "UPDATE card_user_state SET hidden = 1 WHERE disliked = 1",
                [],
            )?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at)
                 VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )?;
            transaction.commit()?;
        }
        if version < 4 {
            let transaction = connection.transaction()?;
            transaction.execute(
                "ALTER TABLE card_user_state
                 ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, applied_at)
                 VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )?;
            transaction.commit()?;
        }
        connection.execute(
            "UPDATE card_user_state
             SET hidden = 1
             WHERE disliked = 1 AND hidden = 0",
            [],
        )?;
        Ok(())
    }

    fn seed(&self, connection: &mut Connection) -> Result<(), DbError> {
        let transaction = connection.transaction()?;
        for (rank, (id, label)) in PRESET_TOPICS.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO topic_preferences
                 (topic_id, label, selected, enabled, custom, explicit_rank, weight)
                 VALUES (?1, ?2, 0, 1, 0, ?3, 0)",
                params![id, label, rank as i64],
            )?;
        }

        let cards: Vec<KnowledgeCard> =
            serde_json::from_str(include_str!("../resources/demo-cards.json"))?;
        for card in cards {
            validate_card(&card).map_err(|error| DbError::Validation(error.to_string()))?;
            insert_card(&transaction, &card, true, false)?;
        }
        if self
            .get_json_with_connection::<AppSettings>(&transaction, "settings")?
            .is_none()
        {
            set_json_with_connection(&transaction, "settings", &AppSettings::default())?;
        }
        if self
            .get_json_with_connection::<bool>(&transaction, "onboarding_complete")?
            .is_none()
        {
            set_json_with_connection(&transaction, "onboarding_complete", &false)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn settings(&self) -> Result<AppSettings, DbError> {
        let connection = self.connect()?;
        Ok(self
            .get_json_with_connection(&connection, "settings")?
            .unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), DbError> {
        let connection = self.connect()?;
        set_json_with_connection(&connection, "settings", settings)
    }

    pub fn generation_provider_id(&self) -> Result<String, DbError> {
        let connection = self.connect()?;
        Ok(self
            .get_json_with_connection(&connection, "generation_provider_id")?
            .unwrap_or_else(|| "deepseek".to_string()))
    }

    pub fn save_generation_provider_id(&self, provider_id: &str) -> Result<(), DbError> {
        let connection = self.connect()?;
        set_json_with_connection(&connection, "generation_provider_id", &provider_id)
    }

    pub fn onboarding_complete(&self) -> Result<bool, DbError> {
        let connection = self.connect()?;
        Ok(self
            .get_json_with_connection(&connection, "onboarding_complete")?
            .unwrap_or(false))
    }

    pub fn topics(&self) -> Result<Vec<TopicPreference>, DbError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT topic_id, label, selected, enabled, custom, explicit_rank, weight
             FROM topic_preferences ORDER BY explicit_rank, label",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TopicPreference {
                id: row.get(0)?,
                label: row.get(1)?,
                selected: row.get::<_, i64>(2)? != 0,
                enabled: row.get::<_, i64>(3)? != 0,
                custom: row.get::<_, i64>(4)? != 0,
                rank: row.get(5)?,
                weight: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn available_card_count(&self) -> Result<u32, DbError> {
        let connection = self.connect()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*)
             FROM cards c
             LEFT JOIN card_user_state s ON s.card_id = c.id
             WHERE COALESCE(s.hidden, 0) = 0
               AND COALESCE(s.deleted, 0) = 0",
            [],
            |row| row.get(0),
        )?;
        u32::try_from(count).map_err(|_| DbError::Validation("可展示知识卡数量超出支持范围".into()))
    }

    pub fn next_card(&self, current_id: Option<&str>) -> Result<KnowledgeCard, DbError> {
        let connection = self.connect()?;
        let selected_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM topic_preferences WHERE selected = 1 AND enabled = 1",
            [],
            |row| row.get(0),
        )?;
        let shown_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM interactions WHERE kind = 'shown'",
            [],
            |row| row.get(0),
        )?;
        let explore = shown_count % 5 == 4;
        let recent_topics = {
            let mut statement = connection.prepare(
                "SELECT c.topic_id
                 FROM interactions i
                 JOIN cards c ON c.id = i.card_id
                 WHERE i.kind = 'shown'
                 ORDER BY i.id DESC
                 LIMIT 2",
            )?;
            let topics = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            topics
        };
        let blocked_topic = (recent_topics.len() == 2 && recent_topics[0] == recent_topics[1])
            .then(|| recent_topics[0].as_str());
        let query = format!(
            "SELECT {CARD_COLUMNS}, MAX(i.created_at) AS last_shown,
                    COALESCE(tp.weight, 0) AS topic_weight,
                    COALESCE(tp.selected, 0) AS topic_selected
             FROM cards c
             LEFT JOIN card_user_state s ON s.card_id = c.id
             LEFT JOIN interactions i ON i.card_id = c.id AND i.kind = 'shown'
             LEFT JOIN topic_preferences tp ON tp.topic_id = c.topic_id
             WHERE COALESCE(s.hidden, 0) = 0
               AND COALESCE(s.deleted, 0) = 0
               AND (?1 IS NULL OR c.id <> ?1)
             GROUP BY c.id
             ORDER BY
               CASE
                 WHEN ?2 = 0 THEN 0
                 WHEN COALESCE(tp.selected, 0) = 1 THEN 0
                 ELSE 1
               END,
               CASE WHEN ?4 IS NOT NULL AND c.topic_id = ?4 THEN 1 ELSE 0 END,
               COALESCE(s.known, 0),
               last_shown IS NOT NULL,
               last_shown ASC,
               CASE WHEN ?3 = 1 THEN COALESCE(tp.weight, 0) ELSE 0 END ASC,
               CASE WHEN ?3 = 0 THEN COALESCE(tp.weight, 0) ELSE 0 END DESC,
               c.created_at ASC
             LIMIT 1"
        );
        let card = connection
            .query_row(
                &query,
                params![current_id, selected_count, explore, blocked_topic],
                card_from_row,
            )
            .optional()?
            .or_else(|| {
                let fallback = format!(
                    "SELECT {CARD_COLUMNS} FROM cards c
                     LEFT JOIN card_user_state s ON s.card_id = c.id
                     WHERE COALESCE(s.hidden, 0) = 0
                       AND COALESCE(s.deleted, 0) = 0
                     ORDER BY c.created_at LIMIT 1"
                );
                connection
                    .query_row(&fallback, [], card_from_row)
                    .optional()
                    .ok()
                    .flatten()
            })
            .ok_or(DbError::NoCards)?;
        self.record_interaction(&card.id, "shown")?;
        Ok(card)
    }

    pub fn card_by_id(&self, card_id: &str) -> Result<KnowledgeCard, DbError> {
        let connection = self.connect()?;
        let query = format!(
            "SELECT {CARD_COLUMNS}
             FROM cards c
             LEFT JOIN card_user_state s ON s.card_id = c.id
             WHERE c.id = ?1
               AND COALESCE(s.deleted, 0) = 0
             LIMIT 1"
        );
        connection
            .query_row(&query, [card_id], card_from_row)
            .optional()?
            .ok_or(DbError::NoCards)
    }

    pub fn record_interaction(&self, card_id: &str, kind: &str) -> Result<(), DbError> {
        const ALLOWED: &[&str] = &[
            "shown",
            "revealed",
            "expanded",
            "liked",
            "disliked",
            "known",
            "favorited",
            "unfavorited",
            "reported",
        ];
        if !ALLOWED.contains(&kind) {
            return Err(DbError::Validation("未知的反馈类型".into()));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO interactions(card_id, kind, created_at) VALUES (?1, ?2, ?3)",
            params![card_id, kind, now],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO card_user_state(card_id, updated_at) VALUES (?1, ?2)",
            params![card_id, now],
        )?;
        match kind {
            "liked" => {
                transaction.execute(
                    "UPDATE card_user_state SET liked = 1, disliked = 0, updated_at = ?2 WHERE card_id = ?1",
                    params![card_id, now],
                )?;
                adjust_topic_weight(&transaction, card_id, 2)?;
            }
            "disliked" => {
                transaction.execute(
                    "UPDATE card_user_state
                     SET disliked = 1, liked = 0, hidden = 1, updated_at = ?2
                     WHERE card_id = ?1",
                    params![card_id, now],
                )?;
                adjust_topic_weight(&transaction, card_id, -3)?;
            }
            "expanded" => adjust_topic_weight(&transaction, card_id, 1)?,
            "known" => {
                transaction.execute(
                    "UPDATE card_user_state
                     SET known = 1, favorite = 0, hidden = 1, updated_at = ?2
                     WHERE card_id = ?1",
                    params![card_id, now],
                )?;
            }
            "favorited" => {
                transaction.execute(
                    "UPDATE card_user_state SET favorite = 1, updated_at = ?2 WHERE card_id = ?1",
                    params![card_id, now],
                )?;
                adjust_topic_weight(&transaction, card_id, 3)?;
            }
            "unfavorited" => {
                transaction.execute(
                    "UPDATE card_user_state SET favorite = 0, updated_at = ?2 WHERE card_id = ?1",
                    params![card_id, now],
                )?;
            }
            "reported" => {
                transaction.execute(
                    "UPDATE card_user_state SET hidden = 1, updated_at = ?2 WHERE card_id = ?1",
                    params![card_id, now],
                )?;
            }
            _ => {}
        }
        if kind == "shown" {
            transaction.execute(
                "DELETE FROM interactions
                 WHERE kind = 'shown' AND id NOT IN (
                   SELECT id FROM interactions WHERE kind = 'shown'
                   ORDER BY created_at DESC, id DESC LIMIT 500
                 )",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn get_json_with_connection<T: for<'de> Deserialize<'de>>(
        &self,
        connection: &Connection,
        key: &str,
    ) -> Result<Option<T>, DbError> {
        connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| serde_json::from_str(&value).map_err(DbError::from))
            .transpose()
    }
}

fn set_json_with_connection<T: Serialize>(
    connection: &Connection,
    key: &str,
    value: &T,
) -> Result<(), DbError> {
    let value = serde_json::to_string(value)?;
    connection.execute(
        "INSERT INTO app_settings(key, value_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, value, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

impl Database {
    pub fn save_onboarding(
        &self,
        input: &OnboardingInput,
    ) -> Result<Vec<TopicPreference>, DbError> {
        let selected_ids = input
            .selected_topic_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let custom_labels = input
            .custom_interests
            .iter()
            .map(|label| label.trim())
            .collect::<HashSet<_>>();
        if selected_ids.len() + custom_labels.len() < 3 {
            return Err(DbError::Validation("首次使用至少选择 3 个兴趣".into()));
        }
        if selected_ids
            .iter()
            .any(|id| !PRESET_TOPICS.iter().any(|(preset, _)| preset == id))
        {
            return Err(DbError::Validation("包含未知的预设兴趣".into()));
        }
        for custom in &custom_labels {
            validate_custom_interest(custom)?;
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE topic_preferences SET selected = 0 WHERE custom = 0",
            [],
        )?;
        for topic_id in &input.selected_topic_ids {
            transaction.execute(
                "UPDATE topic_preferences SET selected = 1, enabled = 1 WHERE topic_id = ?1",
                [topic_id],
            )?;
        }
        let next_rank: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(explicit_rank), -1) + 1 FROM topic_preferences",
            [],
            |row| row.get(0),
        )?;
        for (offset, label) in input.custom_interests.iter().enumerate() {
            let id = format!("custom-{}", uuid::Uuid::new_v4());
            transaction.execute(
                "INSERT INTO topic_preferences(
                    topic_id, label, selected, enabled, custom, explicit_rank, weight
                 ) VALUES (?1, ?2, 1, 1, 1, ?3, 0)",
                params![id, label.trim(), next_rank + offset as i64],
            )?;
        }
        let mut settings: AppSettings = self
            .get_json_with_connection(&transaction, "settings")?
            .unwrap_or_default();
        settings.reminder_preset = input.reminder_preset.clone();
        settings.reminder_times = match input.reminder_preset {
            crate::models::ReminderPreset::WeekdayTwice => {
                vec!["11:00".into(), "16:00".into()]
            }
            _ => vec!["11:00".into()],
        };
        set_json_with_connection(&transaction, "settings", &settings)?;
        set_json_with_connection(&transaction, "onboarding_complete", &true)?;
        transaction.commit()?;
        self.topics()
    }

    pub fn save_interests(
        &self,
        topics: &[TopicPreference],
        personalization_enabled: bool,
    ) -> Result<Vec<TopicPreference>, DbError> {
        let mut seen_ids = HashSet::new();
        for topic in topics {
            if !seen_ids.insert(topic.id.as_str()) {
                return Err(DbError::Validation("兴趣列表包含重复项".into()));
            }
            if topic.custom {
                validate_custom_interest(&topic.label)?;
            } else if !PRESET_TOPICS
                .iter()
                .any(|(id, label)| *id == topic.id.as_str() && *label == topic.label.as_str())
            {
                return Err(DbError::Validation("预设兴趣不能改名或替换".into()));
            }
        }
        let selected = topics
            .iter()
            .filter(|topic| topic.selected && topic.enabled)
            .count();
        if selected < 3 {
            return Err(DbError::Validation("请至少保留 3 个已启用兴趣".into()));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM topic_preferences WHERE custom = 1", [])?;
        for (rank, topic) in topics.iter().enumerate() {
            if topic.label.trim().is_empty() || topic.label.chars().count() > 30 {
                return Err(DbError::Validation("兴趣名称不符合长度要求".into()));
            }
            transaction.execute(
                "INSERT INTO topic_preferences(
                    topic_id, label, selected, enabled, custom, explicit_rank, weight
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(topic_id) DO UPDATE SET
                    label = excluded.label,
                    selected = excluded.selected,
                    enabled = excluded.enabled,
                    custom = excluded.custom,
                    explicit_rank = excluded.explicit_rank",
                params![
                    topic.id,
                    topic.label.trim(),
                    topic.selected,
                    topic.enabled,
                    topic.custom,
                    rank as i64,
                    topic.weight
                ],
            )?;
        }
        let mut settings: AppSettings = self
            .get_json_with_connection(&transaction, "settings")?
            .unwrap_or_default();
        settings.personalization_enabled = personalization_enabled;
        set_json_with_connection(&transaction, "settings", &settings)?;
        transaction.commit()?;
        self.topics()
    }

    pub fn list_library(
        &self,
        mode: &str,
        topic_id: Option<&str>,
        newest_first: bool,
    ) -> Result<Vec<LibraryItem>, DbError> {
        if mode != "favorites" && mode != "history" {
            return Err(DbError::Validation("未知的列表类型".into()));
        }
        let connection = self.connect()?;
        let favorite_clause = if mode == "favorites" {
            "AND COALESCE(s.favorite, 0) = 1"
        } else {
            ""
        };
        let having_clause = if mode == "history" {
            "HAVING MAX(i.created_at) IS NOT NULL"
        } else {
            ""
        };
        let order = if newest_first { "DESC" } else { "ASC" };
        let query = format!(
            "SELECT {CARD_COLUMNS},
                    COALESCE(MAX(i.created_at), MAX(s.updated_at), c.created_at) AS viewed_at
             FROM cards c
             LEFT JOIN interactions i ON i.card_id = c.id AND i.kind = 'shown'
             LEFT JOIN card_user_state s ON s.card_id = c.id
             WHERE (?1 IS NULL OR c.topic_id = ?1)
               AND COALESCE(s.deleted, 0) = 0
               {favorite_clause}
             GROUP BY c.id
             {having_clause}
             ORDER BY viewed_at {order}
             LIMIT 500"
        );
        let mut statement = connection.prepare(&query)?;
        let rows = statement.query_map([topic_id], |row| {
            Ok(LibraryItem {
                card: card_from_row(row)?,
                viewed_at: row.get(19)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn delete_library_card(&self, card_id: &str) -> Result<(), DbError> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let built_in = transaction
            .query_row(
                "SELECT built_in FROM cards WHERE id = ?1",
                [card_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or(DbError::NoCards)?;
        if built_in == 0 {
            transaction.execute("DELETE FROM cards WHERE id = ?1", [card_id])?;
        } else {
            let now = chrono::Utc::now().to_rfc3339();
            transaction.execute(
                "INSERT OR IGNORE INTO card_user_state(card_id, updated_at)
                 VALUES (?1, ?2)",
                params![card_id, now],
            )?;
            transaction.execute(
                "UPDATE card_user_state
                 SET favorite = 0, hidden = 1, deleted = 1, updated_at = ?2
                 WHERE card_id = ?1",
                params![card_id, now],
            )?;
            transaction.execute("DELETE FROM interactions WHERE card_id = ?1", [card_id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn resolve_generation_topic(
        &self,
        topic_id: Option<&str>,
        topic_label: &str,
    ) -> Result<(String, String), DbError> {
        let label = topic_label.trim();
        validate_custom_interest(label)?;
        let connection = self.connect()?;
        if let Some(topic_id) = topic_id {
            return connection
                .query_row(
                    "SELECT topic_id, label
                     FROM topic_preferences
                     WHERE topic_id = ?1 AND enabled = 1",
                    [topic_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| DbError::Validation("所选领域已不可用".into()));
        }
        if let Some(existing) = connection
            .query_row(
                "SELECT topic_id, label
                 FROM topic_preferences
                 WHERE label = ?1 AND enabled = 1
                 LIMIT 1",
                [label],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        {
            return Ok(existing);
        }
        let digest = fingerprint(label, "topic");
        let generated_id = format!("custom_generated_{}", &digest[..16]);
        let next_rank: i64 = connection.query_row(
            "SELECT COALESCE(MAX(explicit_rank), -1) + 1 FROM topic_preferences",
            [],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT OR IGNORE INTO topic_preferences
             (topic_id, label, selected, enabled, custom, explicit_rank, weight)
             VALUES (?1, ?2, 0, 1, 1, ?3, 0)",
            params![generated_id, label, next_rank],
        )?;
        Ok((generated_id, label.to_string()))
    }

    pub fn random_generation_topic(&self) -> Result<(String, String), DbError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT topic_id, label, selected, enabled
             FROM topic_preferences
             ORDER BY explicit_rank, label",
        )?;
        let candidates = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .collect::<Result<Vec<(String, String, bool, bool)>, _>>()?;
        let weighted = generation_topic_weights(candidates);
        let total_weight = weighted.iter().map(|item| item.2).sum::<u32>();
        if total_weight == 0 {
            return Err(DbError::Validation("请先选择兴趣领域".into()));
        }
        let ticket: i64 = connection.query_row(
            "SELECT (random() & 9223372036854775807) % ?1",
            [i64::from(total_weight)],
            |row| row.get(0),
        )?;
        let mut cursor = ticket as u32;
        for (id, label, weight) in weighted {
            if cursor < weight {
                return Ok((id, label));
            }
            cursor -= weight;
        }
        Err(DbError::Validation("没有可用于生成的兴趣领域".into()))
    }

    pub fn provider_profile(
        &self,
        provider_id: &str,
        region: &str,
    ) -> Result<Option<ProviderProfileRecord>, DbError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT provider_id, region, model, credential_ref, key_last4,
                        connection_verified
                 FROM provider_profiles WHERE provider_id = ?1 AND region = ?2",
                params![provider_id, region],
                |row| {
                    Ok(ProviderProfileRecord {
                        provider_id: row.get(0)?,
                        region: row.get(1)?,
                        model: row.get(2)?,
                        credential_ref: row.get(3)?,
                        key_last4: row.get(4)?,
                        connection_verified: row.get::<_, i64>(5)? != 0,
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn provider_profiles(&self) -> Result<Vec<ProviderProfileRecord>, DbError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT provider_id, region, model, credential_ref, key_last4,
                    connection_verified
             FROM provider_profiles",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ProviderProfileRecord {
                provider_id: row.get(0)?,
                region: row.get(1)?,
                model: row.get(2)?,
                credential_ref: row.get(3)?,
                key_last4: row.get(4)?,
                connection_verified: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn save_provider_profile(&self, profile: &ProviderProfileRecord) -> Result<(), DbError> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO provider_profiles(
                provider_id, region, model, credential_ref, key_last4,
                connection_verified, auto_generate_enabled, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
             ON CONFLICT(provider_id, region) DO UPDATE SET
                model = excluded.model,
                credential_ref = excluded.credential_ref,
                key_last4 = excluded.key_last4,
                connection_verified = excluded.connection_verified,
                auto_generate_enabled = 0,
                updated_at = excluded.updated_at",
            params![
                profile.provider_id,
                profile.region,
                profile.model,
                profile.credential_ref,
                profile.key_last4,
                profile.connection_verified,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn set_provider_verified(
        &self,
        provider_id: &str,
        region: &str,
        verified: bool,
    ) -> Result<(), DbError> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE provider_profiles
             SET connection_verified = ?3,
                 auto_generate_enabled = 0,
                 updated_at = ?4
             WHERE provider_id = ?1 AND region = ?2",
            params![
                provider_id,
                region,
                verified,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn delete_provider_profile(&self, provider_id: &str, region: &str) -> Result<(), DbError> {
        let connection = self.connect()?;
        connection.execute(
            "DELETE FROM provider_profiles WHERE provider_id = ?1 AND region = ?2",
            params![provider_id, region],
        )?;
        Ok(())
    }

    pub fn insert_cards(
        &self,
        cards: &[KnowledgeCard],
        built_in: bool,
        cache_managed: bool,
    ) -> Result<ImportResult, DbError> {
        let mut result = ImportResult {
            imported: 0,
            duplicates: 0,
            rejected: 0,
            errors: Vec::new(),
        };
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let existing_questions = {
            let mut statement = transaction.prepare("SELECT question FROM cards")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut accepted_questions = Vec::<String>::new();
        for card in cards {
            if let Err(error) = validate_card(card) {
                result.rejected += 1;
                result.errors.push(error.to_string());
                continue;
            }
            let duplicate = existing_questions
                .iter()
                .chain(accepted_questions.iter())
                .any(|question| questions_are_similar(question, &card.question));
            if duplicate {
                result.duplicates += 1;
                continue;
            }
            if insert_card(&transaction, card, built_in, cache_managed)? {
                accepted_questions.push(card.question.clone());
                result.imported += 1;
            } else {
                result.duplicates += 1;
            }
        }
        if cache_managed {
            transaction.execute(
                "DELETE FROM cards
                 WHERE cache_managed = 1 AND id NOT IN (
                    SELECT id FROM cards WHERE cache_managed = 1
                    ORDER BY created_at DESC LIMIT 30
                 )",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn clear_data(&self, scope: &str) -> Result<(), DbError> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        match scope {
            "history" => {
                transaction.execute(
                    "DELETE FROM interactions WHERE kind IN ('shown', 'revealed', 'expanded')",
                    [],
                )?;
            }
            "preferences" => {
                transaction.execute("DELETE FROM topic_preferences WHERE custom = 1", [])?;
                transaction.execute(
                    "UPDATE topic_preferences SET weight = 0, cooldown_until = NULL",
                    [],
                )?;
            }
            "all" => {
                transaction.execute("DELETE FROM interactions", [])?;
                transaction.execute("DELETE FROM card_user_state", [])?;
                transaction.execute("DELETE FROM generation_jobs", [])?;
                transaction.execute("DELETE FROM provider_profiles", [])?;
                transaction.execute("DELETE FROM cards WHERE built_in = 0", [])?;
                transaction.execute("DELETE FROM topic_preferences", [])?;
                transaction.execute("DELETE FROM app_settings", [])?;
            }
            _ => return Err(DbError::Validation("未知的数据清除范围".into())),
        }
        transaction.commit()?;
        if scope == "all" {
            let mut connection = self.connect()?;
            self.seed(&mut connection)?;
        }
        Ok(())
    }

    pub fn window_state(&self) -> Result<Option<WindowState>, DbError> {
        let connection = self.connect()?;
        self.get_json_with_connection(&connection, "window_state")
    }

    pub fn save_window_state(&self, state: &WindowState) -> Result<(), DbError> {
        let connection = self.connect()?;
        set_json_with_connection(&connection, "window_state", state)
    }

    pub fn reminder_marker(&self) -> Result<Option<String>, DbError> {
        let connection = self.connect()?;
        self.get_json_with_connection(&connection, "last_reminder_marker")
    }

    pub fn save_reminder_marker(&self, marker: &str) -> Result<(), DbError> {
        let connection = self.connect()?;
        set_json_with_connection(&connection, "last_reminder_marker", &marker)
    }

    pub fn close_tip_shown(&self) -> Result<bool, DbError> {
        let connection = self.connect()?;
        Ok(self
            .get_json_with_connection(&connection, "close_tip_shown")?
            .unwrap_or(false))
    }

    pub fn mark_close_tip_shown(&self) -> Result<(), DbError> {
        let connection = self.connect()?;
        set_json_with_connection(&connection, "close_tip_shown", &true)
    }

    pub fn start_generation_job(
        &self,
        id: &str,
        provider_id: &str,
        requested_count: usize,
    ) -> Result<(), DbError> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO generation_jobs(
                id, provider_id, status, requested_count, created_at
             ) VALUES (?1, ?2, 'running', ?3, ?4)",
            params![
                id,
                provider_id,
                requested_count as i64,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn finish_generation_job(
        &self,
        id: &str,
        generated_count: usize,
        error_code: Option<&str>,
    ) -> Result<(), DbError> {
        let connection = self.connect()?;
        let status = if error_code.is_some() {
            "failed"
        } else {
            "completed"
        };
        connection.execute(
            "UPDATE generation_jobs
             SET status = ?2, generated_count = ?3, error_code = ?4,
                 retry_at = NULL, finished_at = ?5
             WHERE id = ?1",
            params![
                id,
                status,
                generated_count as i64,
                error_code,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn generated_today(&self) -> Result<u32, DbError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT COALESCE(SUM(generated_count), 0)
                 FROM generation_jobs
                 WHERE status = 'completed'
                   AND date(created_at, 'localtime') = date('now', 'localtime')",
                [],
                |row| row.get(0),
            )
            .map_err(DbError::from)
    }
}

fn generation_topic_weights(
    candidates: Vec<(String, String, bool, bool)>,
) -> Vec<(String, String, u32)> {
    let count = candidates.len() as u32;
    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(index, (id, label, selected, enabled))| {
            (selected && enabled).then_some((id, label, count - index as u32))
        })
        .collect()
}

fn adjust_topic_weight(
    transaction: &Transaction<'_>,
    card_id: &str,
    delta: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "UPDATE topic_preferences
         SET weight = MAX(-20, MIN(20, weight + ?2))
         WHERE topic_id = (SELECT topic_id FROM cards WHERE id = ?1)",
        params![card_id, delta],
    )?;
    Ok(())
}

fn validate_custom_interest(label: &str) -> Result<(), DbError> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 30 {
        return Err(DbError::Validation(
            "自定义兴趣不能为空且不能超过 30 个字符".into(),
        ));
    }
    if [
        "医疗诊断",
        "法律建议",
        "投资建议",
        "实时政治",
        "博彩",
        "成人内容",
    ]
    .iter()
    .any(|term| label.contains(term))
    {
        return Err(DbError::Validation(
            "这个自定义兴趣不在 MVP 的安全内容范围内".into(),
        ));
    }
    Ok(())
}

fn insert_card(
    transaction: &Transaction<'_>,
    card: &KnowledgeCard,
    built_in: bool,
    cache_managed: bool,
) -> Result<bool, DbError> {
    let tags_json = serde_json::to_string(&card.tags)?;
    let sources_json = serde_json::to_string(&card.source_refs)?;
    let generated_json = card
        .generated_by
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO cards(
            id, schema_version, language, topic_id, topic_label, tags_json,
            question, short_answer, explanation, why_it_matters, difficulty,
            estimated_read_seconds, source_refs_json, trust_status,
            content_fingerprint, generated_by_json, created_at, built_in,
            cache_managed
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19
         )",
        params![
            card.id,
            card.schema_version,
            card.language,
            card.topic_id,
            card.topic_label,
            tags_json,
            card.question,
            card.short_answer,
            card.explanation,
            card.why_it_matters,
            difficulty_to_str(&card.difficulty),
            card.estimated_read_seconds,
            sources_json,
            trust_to_str(&card.trust_status),
            card.content_fingerprint,
            generated_json,
            card.created_at,
            built_in,
            cache_managed
        ],
    )?;
    Ok(inserted > 0)
}

fn card_from_row(row: &Row<'_>) -> rusqlite::Result<KnowledgeCard> {
    let tags_json: String = row.get(5)?;
    let source_refs_json: String = row.get(12)?;
    let generated_by_json: Option<String> = row.get(15)?;
    Ok(KnowledgeCard {
        id: row.get(0)?,
        schema_version: row.get(1)?,
        language: row.get(2)?,
        topic_id: row.get(3)?,
        topic_label: row.get(4)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        question: row.get(6)?,
        short_answer: row.get(7)?,
        explanation: row.get(8)?,
        why_it_matters: row.get(9)?,
        difficulty: str_to_difficulty(&row.get::<_, String>(10)?),
        estimated_read_seconds: row.get(11)?,
        source_refs: serde_json::from_str(&source_refs_json).unwrap_or_default(),
        trust_status: str_to_trust(&row.get::<_, String>(13)?),
        content_fingerprint: row.get(14)?,
        generated_by: generated_by_json.and_then(|value| serde_json::from_str(&value).ok()),
        created_at: row.get(16)?,
        is_favorite: row.get::<_, i64>(17)? != 0,
        hidden_from_feed: row.get::<_, i64>(18)? != 0,
    })
}

fn difficulty_to_str(value: &Difficulty) -> &'static str {
    match value {
        Difficulty::Beginner => "beginner",
        Difficulty::General => "general",
        Difficulty::Advanced => "advanced",
    }
}

fn str_to_difficulty(value: &str) -> Difficulty {
    match value {
        "beginner" => Difficulty::Beginner,
        "advanced" => Difficulty::Advanced,
        _ => Difficulty::General,
    }
}

fn trust_to_str(value: &TrustStatus) -> &'static str {
    match value {
        TrustStatus::Verified => "verified",
        TrustStatus::SourceGrounded => "source_grounded",
        TrustStatus::AiUnverified => "ai_unverified",
        TrustStatus::DemoUnreviewed => "demo_unreviewed",
    }
}

fn str_to_trust(value: &str) -> TrustStatus {
    match value {
        "verified" => TrustStatus::Verified,
        "source_grounded" => TrustStatus::SourceGrounded,
        "ai_unverified" => TrustStatus::AiUnverified,
        _ => TrustStatus::DemoUnreviewed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_database() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = Database::new(directory.path().join("test.db"));
        database.initialize().expect("database initializes");
        (directory, database)
    }

    #[test]
    fn migrations_seed_offline_content_and_defaults() {
        let (_directory, database) = test_database();
        assert!(!database.onboarding_complete().expect("onboarding flag"));
        assert_eq!(database.topics().expect("topics").len(), 8);
        let card = database.next_card(None).expect("offline card");
        assert!(!card.question.is_empty());
        assert_eq!(
            database.card_by_id(&card.id).expect("card lookup").id,
            card.id
        );
        assert_eq!(
            database
                .generation_provider_id()
                .expect("default generation provider"),
            "deepseek"
        );
        database
            .save_generation_provider_id("kimi")
            .expect("save generation provider");
        assert_eq!(
            database
                .generation_provider_id()
                .expect("saved generation provider"),
            "kimi"
        );
    }

    #[test]
    fn onboarding_and_feedback_persist_between_connections() {
        let (_directory, database) = test_database();
        database
            .save_onboarding(&OnboardingInput {
                selected_topic_ids: vec![
                    "natural_science".into(),
                    "space_earth".into(),
                    "computing_internet".into(),
                ],
                custom_interests: Vec::new(),
                reminder_preset: crate::models::ReminderPreset::Manual,
            })
            .expect("save onboarding");
        let card = database.next_card(None).expect("card");
        database
            .record_interaction(&card.id, "favorited")
            .expect("favorite");
        assert!(database.onboarding_complete().expect("onboarding flag"));
        let favorites = database
            .list_library("favorites", None, true)
            .expect("favorites");
        assert_eq!(favorites.len(), 1);
        assert!(favorites[0].card.is_favorite);
    }

    #[test]
    fn clearing_history_does_not_delete_favorites() {
        let (_directory, database) = test_database();
        let card = database.next_card(None).expect("card");
        database
            .record_interaction(&card.id, "favorited")
            .expect("favorite");
        database.clear_data("history").expect("clear history");
        assert!(database
            .list_library("history", None, true)
            .expect("history")
            .is_empty());
        assert_eq!(
            database
                .list_library("favorites", None, true)
                .expect("favorites")
                .len(),
            1
        );
    }

    #[test]
    fn disliked_cards_stay_hidden_while_browsing() {
        let (_directory, database) = test_database();
        let initial_count = database
            .available_card_count()
            .expect("initial available count");
        let disliked = database.next_card(None).expect("card");
        database
            .record_interaction(&disliked.id, "disliked")
            .expect("dislike");
        assert_eq!(
            database.available_card_count().expect("available count"),
            initial_count - 1
        );

        let connection = database.connect().expect("connection");
        let hidden: i64 = connection
            .query_row(
                "SELECT hidden FROM card_user_state WHERE card_id = ?1",
                [&disliked.id],
                |row| row.get(0),
            )
            .expect("hidden state");
        assert_eq!(hidden, 1);
        drop(connection);

        let mut current_id = disliked.id.clone();
        for _ in 0..24 {
            let card = database.next_card(Some(&current_id)).expect("next card");
            assert_ne!(card.id, disliked.id);
            current_id = card.id;
        }
        let history = database
            .list_library("history", None, true)
            .expect("history");
        let retained = history
            .iter()
            .find(|item| item.card.id == disliked.id)
            .expect("disliked card remains in history");
        assert!(retained.card.hidden_from_feed);
    }

    #[test]
    fn hidden_history_card_appears_in_favorites_without_returning_to_the_feed() {
        let (_directory, database) = test_database();
        let initial_count = database
            .available_card_count()
            .expect("initial available count");
        let card = database.next_card(None).expect("card");
        database
            .record_interaction(&card.id, "disliked")
            .expect("dislike");
        assert_eq!(
            database.available_card_count().expect("available count"),
            initial_count - 1
        );

        let history_card = database
            .list_library("history", None, true)
            .expect("history")
            .into_iter()
            .find(|item| item.card.id == card.id)
            .expect("hidden card remains in history");
        assert!(history_card.card.hidden_from_feed);

        database
            .record_interaction(&card.id, "favorited")
            .expect("favorite from history");
        let favorite = database
            .list_library("favorites", None, true)
            .expect("favorites")
            .into_iter()
            .find(|item| item.card.id == card.id)
            .expect("hidden favorite is listed");

        assert!(favorite.card.is_favorite);
        assert!(favorite.card.hidden_from_feed);
        assert_eq!(
            database.available_card_count().expect("available count"),
            initial_count - 1
        );
    }

    #[test]
    fn known_cards_leave_the_feed_but_remain_in_history() {
        let (_directory, database) = test_database();
        let known = database.next_card(None).expect("card");
        database
            .record_interaction(&known.id, "known")
            .expect("known");

        assert_ne!(
            database.next_card(Some(&known.id)).expect("next card").id,
            known.id
        );
        assert!(database
            .list_library("history", None, true)
            .expect("history")
            .iter()
            .any(|item| item.card.id == known.id));
    }

    #[test]
    fn deleting_a_history_card_removes_it_from_the_system() {
        let (_directory, database) = test_database();
        let card = database.next_card(None).expect("card");
        database
            .record_interaction(&card.id, "disliked")
            .expect("dislike");
        database
            .delete_library_card(&card.id)
            .expect("delete history card");

        assert!(database
            .list_library("history", None, true)
            .expect("history")
            .iter()
            .all(|item| item.card.id != card.id));
        assert!(matches!(
            database.card_by_id(&card.id),
            Err(DbError::NoCards)
        ));
        assert_ne!(database.next_card(None).expect("next card").id, card.id);
    }

    #[test]
    fn custom_generation_topics_are_validated_and_reused() {
        let (_directory, database) = test_database();
        let first = database
            .resolve_generation_topic(None, "建筑与城市")
            .expect("custom topic");
        let second = database
            .resolve_generation_topic(None, "建筑与城市")
            .expect("existing topic");
        assert_eq!(first, second);
        assert!(first.0.starts_with("custom_generated_"));
        assert!(database.resolve_generation_topic(None, "实时政治").is_err());
    }

    #[test]
    fn generation_topic_weights_descend_with_saved_order() {
        let weighted = generation_topic_weights(vec![
            ("first".into(), "第一".into(), true, true),
            ("second".into(), "第二".into(), true, true),
            ("third".into(), "第三".into(), true, true),
        ]);
        assert_eq!(
            weighted
                .iter()
                .map(|(id, _, weight)| (id.as_str(), *weight))
                .collect::<Vec<_>>(),
            vec![("first", 3), ("second", 2), ("third", 1)]
        );
    }

    #[test]
    fn generation_topic_weights_follow_absolute_list_positions() {
        let weighted = generation_topic_weights(vec![
            ("first".into(), "第一".into(), true, true),
            ("not-selected".into(), "未选择".into(), false, true),
            ("third".into(), "第三".into(), true, true),
        ]);
        assert_eq!(
            weighted
                .iter()
                .map(|(id, _, weight)| (id.as_str(), *weight))
                .collect::<Vec<_>>(),
            vec![("first", 3), ("third", 1)]
        );
    }

    #[test]
    fn random_generation_topic_uses_only_selected_interests() {
        let (_directory, database) = test_database();
        database
            .save_onboarding(&OnboardingInput {
                selected_topic_ids: vec![
                    "natural_science".into(),
                    "space_earth".into(),
                    "computing_internet".into(),
                ],
                custom_interests: Vec::new(),
                reminder_preset: crate::models::ReminderPreset::Manual,
            })
            .expect("save onboarding");
        let selected = ["natural_science", "space_earth", "computing_internet"];
        for _ in 0..20 {
            let (topic_id, _) = database.random_generation_topic().expect("weighted topic");
            assert!(selected.contains(&topic_id.as_str()));
        }
    }

    #[test]
    fn generated_today_counts_only_successful_imports() {
        let (_directory, database) = test_database();
        database
            .start_generation_job("completed-job", "deepseek", 5)
            .expect("start completed job");
        database
            .finish_generation_job("completed-job", 3, None)
            .expect("finish completed job");
        database
            .start_generation_job("failed-job", "deepseek", 5)
            .expect("start failed job");
        database
            .finish_generation_job("failed-job", 0, Some("timeout"))
            .expect("finish failed job");

        assert_eq!(database.generated_today().expect("today usage"), 3);
    }

    #[test]
    fn migration_hides_cards_disliked_before_version_three() {
        let (_directory, database) = test_database();
        let card = database.next_card(None).expect("card");
        let connection = database.connect().expect("connection");
        connection
            .execute(
                "UPDATE card_user_state
                 SET disliked = 1, hidden = 0
                 WHERE card_id = ?1",
                [&card.id],
            )
            .expect("legacy dislike state");
        connection
            .execute("DELETE FROM schema_migrations WHERE version = 3", [])
            .expect("rewind migration");
        drop(connection);

        database.initialize().expect("upgrade database");
        let connection = database.connect().expect("connection");
        let hidden: i64 = connection
            .query_row(
                "SELECT hidden FROM card_user_state WHERE card_id = ?1",
                [&card.id],
                |row| row.get(0),
            )
            .expect("hidden state");
        assert_eq!(hidden, 1);
    }

    #[test]
    fn reviewed_imports_are_not_evicted_with_the_ai_cache() {
        let (_directory, database) = test_database();
        let template = serde_json::from_str::<Vec<KnowledgeCard>>(include_str!(
            "../resources/demo-cards.json"
        ))
        .expect("demo cards")
        .remove(0);
        let make_cards = |prefix: &str, count: usize| {
            (0..count)
                .map(|index| {
                    let mut card = template.clone();
                    card.id = format!("{prefix}-{index}");
                    card.question = format!(
                        "{prefix}{:08X} 的结构为何会呈现这种独特变化过程？",
                        index.wrapping_mul(2_654_435_761)
                    );
                    card.content_fingerprint =
                        crate::content::fingerprint(&card.question, &card.short_answer);
                    card
                })
                .collect::<Vec<_>>()
        };

        let reviewed = make_cards("reviewed", 4);
        let generated = make_cards("generated", 31);
        assert_eq!(
            database
                .insert_cards(&reviewed, false, false)
                .expect("reviewed import")
                .imported,
            4
        );
        database
            .insert_cards(&generated, false, true)
            .expect("generated cache");

        let connection = database.connect().expect("connection");
        let reviewed_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM cards
                 WHERE built_in = 0 AND cache_managed = 0",
                [],
                |row| row.get(0),
            )
            .expect("reviewed count");
        let cache_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM cards WHERE cache_managed = 1",
                [],
                |row| row.get(0),
            )
            .expect("cache count");
        assert_eq!(reviewed_count, 4);
        assert_eq!(cache_count, 30);
    }

    #[test]
    fn imported_duplicate_content_is_skipped() {
        let (_directory, database) = test_database();
        let mut card = serde_json::from_str::<Vec<KnowledgeCard>>(include_str!(
            "../resources/demo-cards.json"
        ))
        .expect("demo cards")
        .remove(0);
        card.id = "reviewed-duplicate-smoke".into();
        card.question = "为什么知识卡导入流程需要在入库前检测重复内容？".into();
        card.content_fingerprint = crate::content::fingerprint(&card.question, &card.short_answer);

        let first = database
            .insert_cards(std::slice::from_ref(&card), false, false)
            .expect("first import");
        card.id = "reviewed-duplicate-smoke-copy".into();
        let duplicate = database
            .insert_cards(&[card], false, false)
            .expect("duplicate import");

        assert_eq!(first.imported, 1);
        assert_eq!(duplicate.imported, 0);
        assert_eq!(duplicate.duplicates, 1);
    }
}
