use crate::models::{Difficulty, GeneratedBy, KnowledgeCard, SourceRef, TrustStatus};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

const HIGH_RISK_TERMS: &[&str] = &[
    "医疗诊断",
    "处方药",
    "法律意见",
    "投资建议",
    "买入股票",
    "实时政治",
    "博彩",
    "成人内容",
];

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("文件格式不受支持，只能导入 JSON 或 CSV")]
    UnsupportedFormat,
    #[error("文件内容无法解析：{0}")]
    Parse(String),
    #[error("知识卡字段校验失败：{0}")]
    Validation(String),
}

pub fn fingerprint(question: &str, short_answer: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize_text(question));
    hasher.update(b"|");
    hasher.update(normalize_text(short_answer));
    format!("{:x}", hasher.finalize())
}

fn normalize_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn validate_card(card: &KnowledgeCard) -> Result<(), ContentError> {
    let question_len = card.question.chars().count();
    let answer_len = card.short_answer.chars().count();
    let explanation_len = card.explanation.chars().count();

    if card.schema_version != 1 {
        return Err(ContentError::Validation("schema_version 必须为 1".into()));
    }
    if card.language != "zh-CN" {
        return Err(ContentError::Validation("首版只接受 zh-CN 内容".into()));
    }
    if !(12..=45).contains(&question_len) {
        return Err(ContentError::Validation("问题长度应为 12–45 个字符".into()));
    }
    if !(40..=120).contains(&answer_len) {
        return Err(ContentError::Validation(
            "简短答案长度应为 40–120 个字符".into(),
        ));
    }
    if !(180..=500).contains(&explanation_len) {
        return Err(ContentError::Validation(
            "详细解释长度应为 180–500 个字符".into(),
        ));
    }
    if !(30..=90).contains(&card.estimated_read_seconds) {
        return Err(ContentError::Validation("预计阅读时间应为 30–90 秒".into()));
    }
    if card.topic_id.trim().is_empty() || card.topic_label.trim().is_empty() {
        return Err(ContentError::Validation("内容领域不能为空".into()));
    }
    let combined = format!(
        "{} {} {}",
        card.question, card.short_answer, card.explanation
    );
    if HIGH_RISK_TERMS.iter().any(|term| combined.contains(term)) {
        return Err(ContentError::Validation(
            "内容触及 MVP 不支持的高风险主题".into(),
        ));
    }
    if matches!(
        card.trust_status,
        TrustStatus::Verified | TrustStatus::SourceGrounded
    ) && card.source_refs.is_empty()
    {
        return Err(ContentError::Validation(
            "已核验或基于来源生成的内容必须包含真实来源".into(),
        ));
    }
    if chrono::DateTime::parse_from_rfc3339(&card.created_at).is_err() {
        return Err(ContentError::Validation(
            "created_at 必须是 ISO 8601 时间".into(),
        ));
    }
    for source in &card.source_refs {
        let parsed = Url::parse(&source.url)
            .map_err(|_| ContentError::Validation("来源 URL 无效".into()))?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(ContentError::Validation("来源 URL 必须使用 HTTPS".into()));
        }
    }
    Ok(())
}

pub fn questions_are_similar(left: &str, right: &str) -> bool {
    let normalized_left = normalize_text(left);
    let normalized_right = normalize_text(right);
    if normalized_left.chars().count() < 2 || normalized_right.chars().count() < 2 {
        return normalized_left == normalized_right;
    }
    let left = bigrams(&normalized_left);
    let right = bigrams(&normalized_right);
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    union > 0 && intersection as f64 / union as f64 >= 0.88
}

fn bigrams(value: &str) -> HashSet<String> {
    let chars: Vec<char> = value.chars().collect();
    chars
        .windows(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect()
}

#[derive(Debug, Deserialize)]
struct JsonEnvelope {
    cards: Vec<KnowledgeCard>,
}

#[derive(Debug, Deserialize)]
struct CsvCard {
    #[serde(default)]
    id: String,
    #[serde(default = "schema_one")]
    schema_version: i64,
    #[serde(default = "default_language")]
    language: String,
    topic_id: String,
    topic_label: String,
    #[serde(default)]
    tags: String,
    question: String,
    short_answer: String,
    explanation: String,
    #[serde(default)]
    why_it_matters: String,
    #[serde(default = "default_difficulty")]
    difficulty: String,
    estimated_read_seconds: i64,
    #[serde(default)]
    source_titles: String,
    #[serde(default)]
    source_urls: String,
    #[serde(default = "default_import_trust")]
    trust_status: String,
    #[serde(default)]
    created_at: String,
}

fn schema_one() -> i64 {
    1
}

fn default_language() -> String {
    "zh-CN".into()
}

fn default_difficulty() -> String {
    "general".into()
}

fn default_import_trust() -> String {
    "demo_unreviewed".into()
}

pub fn parse_import(
    path_extension: &str,
    bytes: &[u8],
) -> Result<Vec<KnowledgeCard>, ContentError> {
    let mut cards = match path_extension.to_ascii_lowercase().as_str() {
        "json" => parse_json(bytes)?,
        "csv" => parse_csv(bytes)?,
        _ => return Err(ContentError::UnsupportedFormat),
    };
    for card in &mut cards {
        card.id = if card.id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            card.id.trim().to_string()
        };
        card.content_fingerprint = fingerprint(&card.question, &card.short_answer);
        card.is_favorite = false;
        validate_card(card)?;
    }
    Ok(cards)
}

fn parse_json(bytes: &[u8]) -> Result<Vec<KnowledgeCard>, ContentError> {
    if let Ok(cards) = serde_json::from_slice::<Vec<KnowledgeCard>>(bytes) {
        return Ok(cards);
    }
    serde_json::from_slice::<JsonEnvelope>(bytes)
        .map(|envelope| envelope.cards)
        .map_err(|error| ContentError::Parse(error.to_string()))
}

fn parse_csv(bytes: &[u8]) -> Result<Vec<KnowledgeCard>, ContentError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(bytes);
    reader
        .deserialize::<CsvCard>()
        .map(|row| {
            row.map_err(|error| ContentError::Parse(error.to_string()))
                .and_then(csv_to_card)
        })
        .collect()
}

fn csv_to_card(row: CsvCard) -> Result<KnowledgeCard, ContentError> {
    let difficulty = match row.difficulty.as_str() {
        "beginner" => Difficulty::Beginner,
        "general" => Difficulty::General,
        "advanced" => Difficulty::Advanced,
        _ => return Err(ContentError::Validation("difficulty 无效".into())),
    };
    let trust_status = match row.trust_status.as_str() {
        "verified" => TrustStatus::Verified,
        "source_grounded" => TrustStatus::SourceGrounded,
        "ai_unverified" => TrustStatus::AiUnverified,
        "demo_unreviewed" => TrustStatus::DemoUnreviewed,
        _ => return Err(ContentError::Validation("trust_status 无效".into())),
    };
    let titles: Vec<&str> = row
        .source_titles
        .split('|')
        .filter(|value| !value.trim().is_empty())
        .collect();
    let urls: Vec<&str> = row
        .source_urls
        .split('|')
        .filter(|value| !value.trim().is_empty())
        .collect();
    if !titles.is_empty() && titles.len() != urls.len() {
        return Err(ContentError::Validation(
            "source_titles 与 source_urls 数量必须一致".into(),
        ));
    }
    let source_refs = urls
        .iter()
        .enumerate()
        .map(|(index, url)| SourceRef {
            title: titles.get(index).copied().unwrap_or("来源").trim().into(),
            url: url.trim().into(),
            publisher: None,
            accessed_at: None,
        })
        .collect();
    Ok(KnowledgeCard {
        id: row.id,
        schema_version: row.schema_version,
        language: row.language,
        topic_id: row.topic_id,
        topic_label: row.topic_label,
        tags: row
            .tags
            .split('|')
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .collect(),
        question: row.question,
        short_answer: row.short_answer,
        explanation: row.explanation,
        why_it_matters: (!row.why_it_matters.trim().is_empty()).then_some(row.why_it_matters),
        difficulty,
        estimated_read_seconds: row.estimated_read_seconds,
        source_refs,
        trust_status,
        content_fingerprint: String::new(),
        generated_by: None,
        created_at: if row.created_at.trim().is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            row.created_at
        },
        is_favorite: false,
        hidden_from_feed: false,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCardInput {
    #[serde(alias = "topic_id")]
    pub topic_id: String,
    #[serde(alias = "topic_label")]
    pub topic_label: String,
    pub tags: Vec<String>,
    pub question: String,
    #[serde(alias = "short_answer")]
    pub short_answer: String,
    pub explanation: String,
    #[serde(default, alias = "why_it_matters")]
    pub why_it_matters: Option<String>,
    pub difficulty: Difficulty,
    #[serde(alias = "estimated_read_seconds")]
    pub estimated_read_seconds: i64,
}

pub fn finalize_generated_cards(
    values: Vec<GeneratedCardInput>,
    provider: &str,
    model: &str,
) -> Result<Vec<KnowledgeCard>, ContentError> {
    if values.is_empty() || values.len() > 5 {
        return Err(ContentError::Validation(
            "单批模型内容必须包含 1–5 张卡片".into(),
        ));
    }
    let mut fingerprints = HashSet::new();
    let mut cards = Vec::with_capacity(values.len());
    let mut first_error = None;

    for value in values {
        let content_fingerprint = fingerprint(&value.question, &value.short_answer);
        let card = KnowledgeCard {
            id: Uuid::new_v4().to_string(),
            schema_version: 1,
            language: "zh-CN".into(),
            topic_id: value.topic_id,
            topic_label: value.topic_label,
            tags: value.tags,
            question: value.question,
            short_answer: value.short_answer,
            explanation: value.explanation,
            why_it_matters: value.why_it_matters,
            difficulty: value.difficulty,
            estimated_read_seconds: value.estimated_read_seconds,
            source_refs: Vec::new(),
            trust_status: TrustStatus::AiUnverified,
            content_fingerprint: content_fingerprint.clone(),
            generated_by: Some(GeneratedBy {
                provider: provider.into(),
                model: model.into(),
                prompt_version: "knowledge-cards-v1".into(),
            }),
            created_at: chrono::Utc::now().to_rfc3339(),
            is_favorite: false,
            hidden_from_feed: false,
        };

        if let Err(error) = validate_card(&card) {
            first_error.get_or_insert(error);
            continue;
        }
        if !fingerprints.insert(content_fingerprint) {
            first_error
                .get_or_insert_with(|| ContentError::Validation("同一批内容存在重复".into()));
            continue;
        }
        cards.push(card);
    }

    if cards.is_empty() {
        Err(first_error
            .unwrap_or_else(|| ContentError::Validation("模型内容没有可用的知识卡".into())))
    } else {
        Ok(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_generated_card() -> GeneratedCardInput {
        GeneratedCardInput {
            topic_id: "natural_science".into(),
            topic_label: "自然科学".into(),
            tags: vec!["水".into()],
            question: "为什么水结冰以后体积反而会变得更大？".into(),
            short_answer:
                "水分子在结冰时形成带有空隙的规则晶体结构，所以同样质量会占据更大体积，密度也因此低于液态水，冰通常会浮在水面。"
                    .into(),
            explanation: "液态水中的分子仍可移动并相对紧密地排列。温度下降到冰点附近时，氢键把水分子固定到带有规则空隙的晶格中。晶格占据的空间更大，因此水结冰时体积增加，密度也低于液态水。这个现象同时解释了冰为什么通常会浮在水面，也影响了寒冷地区的岩石风化与水体生态。水的密度还会随温度改变，实际结冰过程也会受到溶质、压力和成核条件影响，因此这个规律需要在具体环境中理解。此外，水分子排列并非瞬间完成，冷却速度也会影响晶体形成方式。".into(),
            why_it_matters: None,
            difficulty: Difficulty::Beginner,
            estimated_read_seconds: 50,
        }
    }

    #[test]
    fn fingerprint_ignores_spacing_and_punctuation() {
        assert_eq!(
            fingerprint("为什么 天空是蓝色？", "因为散射。"),
            fingerprint("为什么天空是蓝色", "因为散射")
        );
    }

    #[test]
    fn similar_questions_are_detected() {
        assert!(questions_are_similar(
            "为什么晴朗白天的天空通常是蓝色？",
            "为什么晴朗白天的天空通常是蓝色"
        ));
        assert!(!questions_are_similar(
            "为什么晴朗白天的天空通常是蓝色？",
            "电脑为什么需要多级缓存？"
        ));
    }

    #[test]
    fn generated_content_is_never_marked_verified() {
        let cards = finalize_generated_cards(
            vec![GeneratedCardInput {
                topic_id: "natural_science".into(),
                topic_label: "自然科学".into(),
                tags: vec!["水".into()],
                question: "为什么水结冰以后体积反而会变得更大？".into(),
                short_answer:
                    "水分子在结冰时形成带有空隙的规则晶体结构，所以同样质量会占据更大体积，密度也因此低于液态水，冰通常会浮在水面。"
                        .into(),
                explanation: "液态水中的分子仍可移动并相对紧密地排列。温度下降到冰点附近时，氢键把水分子固定到带有规则空隙的晶格中。晶格占据的空间更大，因此水结冰时体积增加，密度也低于液态水。这个现象同时解释了冰为什么通常会浮在水面，也影响了寒冷地区的岩石风化与水体生态。水的密度还会随温度改变，实际结冰过程也会受到溶质、压力和成核条件影响，因此这个规律需要在具体环境中理解。此外，水分子排列并非瞬间完成，冷却速度也会影响晶体形成方式。".into(),
                why_it_matters: None,
                difficulty: Difficulty::Beginner,
                estimated_read_seconds: 50,
            }],
            "mock",
            "mock-model",
        )
        .expect("generated card should validate");
        assert_eq!(cards[0].trust_status, TrustStatus::AiUnverified);
        assert!(cards[0].source_refs.is_empty());
    }

    #[test]
    fn generated_batch_keeps_valid_cards_when_another_card_is_invalid() {
        let invalid = GeneratedCardInput {
            question: "太短".into(),
            ..valid_generated_card()
        };

        let cards =
            finalize_generated_cards(vec![valid_generated_card(), invalid], "mock", "mock-model")
                .expect("the valid generated card should be retained");

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].question, valid_generated_card().question);
    }

    #[test]
    fn generated_batch_returns_first_error_when_all_cards_are_invalid() {
        let first_invalid = GeneratedCardInput {
            question: "太短".into(),
            ..valid_generated_card()
        };
        let second_invalid = GeneratedCardInput {
            estimated_read_seconds: 1,
            ..valid_generated_card()
        };

        let error =
            finalize_generated_cards(vec![first_invalid, second_invalid], "mock", "mock-model")
                .expect_err("an entirely invalid batch should fail");

        assert!(matches!(
            error,
            ContentError::Validation(message) if message.contains("问题长度")
        ));
    }

    #[test]
    fn json_and_csv_import_templates_pass_strict_validation() {
        let json_cards = parse_import(
            "json",
            include_bytes!("../../examples/cards-import-template.json"),
        )
        .expect("JSON template should parse");
        let csv_cards = parse_import(
            "csv",
            include_bytes!("../../examples/cards-import-template.csv"),
        )
        .expect("CSV template should parse");

        assert_eq!(json_cards.len(), 1);
        assert_eq!(csv_cards.len(), 1);
        assert!(!json_cards[0].content_fingerprint.is_empty());
        assert!(!csv_cards[0].content_fingerprint.is_empty());
        assert_eq!(json_cards[0].trust_status, TrustStatus::DemoUnreviewed);
        assert_eq!(csv_cards[0].trust_status, TrustStatus::DemoUnreviewed);
    }

    #[test]
    fn invalid_import_rejects_the_entire_batch() {
        let mut cards = serde_json::from_slice::<Vec<KnowledgeCard>>(include_bytes!(
            "../../examples/cards-import-template.json"
        ))
        .expect("JSON template should deserialize");
        cards.push(KnowledgeCard {
            question: "太短".into(),
            id: "invalid-card".into(),
            ..cards[0].clone()
        });
        let bytes = serde_json::to_vec(&cards).expect("test batch should serialize");

        assert!(matches!(
            parse_import("json", &bytes),
            Err(ContentError::Validation(_))
        ));
    }
}
