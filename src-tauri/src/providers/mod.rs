use crate::content::{finalize_generated_cards, GeneratedCardInput};
use crate::db::{Database, DbError};
use crate::models::{KnowledgeCard, ProviderModel, ProviderRegion, ProviderSpec};
use crate::secret_store::SecretValue;
use async_trait::async_trait;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;
use url::Url;

const DEEPSEEK_ORIGIN: &str = "https://api.deepseek.com";
const KIMI_CN_ORIGIN: &str = "https://api.moonshot.cn";
const KIMI_GLOBAL_ORIGIN: &str = "https://api.moonshot.ai";
const CONNECTION_TEST_TIMEOUT: Duration = Duration::from_secs(15);
const GENERATION_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone)]
pub struct ProviderEndpoint {
    pub origin: &'static str,
    pub chat_path: &'static str,
    pub models: &'static [&'static str],
}

pub struct ProviderRegistry;

impl ProviderRegistry {
    pub fn endpoint(provider_id: &str, region: &str) -> Result<ProviderEndpoint, ProviderError> {
        match (provider_id, region) {
            ("deepseek", "default") => Ok(ProviderEndpoint {
                origin: DEEPSEEK_ORIGIN,
                chat_path: "/chat/completions",
                models: &["deepseek-v4-flash", "deepseek-v4-pro"],
            }),
            ("kimi", "cn") => Ok(ProviderEndpoint {
                origin: KIMI_CN_ORIGIN,
                chat_path: "/v1/chat/completions",
                models: &["kimi-k3", "kimi-k2.6"],
            }),
            ("kimi", "global") => Ok(ProviderEndpoint {
                origin: KIMI_GLOBAL_ORIGIN,
                chat_path: "/v1/chat/completions",
                models: &["kimi-k3", "kimi-k2.6"],
            }),
            _ => Err(ProviderError::UnsafeTarget),
        }
    }

    pub fn validate_model(endpoint: &ProviderEndpoint, model: &str) -> Result<(), ProviderError> {
        if endpoint.models.contains(&model) {
            Ok(())
        } else {
            Err(ProviderError::UnsupportedModel)
        }
    }

    pub fn specs(database: &Database) -> Result<Vec<ProviderSpec>, DbError> {
        let profiles = database.provider_profiles()?;
        let deepseek = profiles
            .iter()
            .find(|profile| profile.provider_id == "deepseek");
        let kimi = profiles
            .iter()
            .find(|profile| profile.provider_id == "kimi");
        Ok(vec![
            ProviderSpec {
                id: "deepseek".into(),
                label: "DeepSeek".into(),
                regions: vec![ProviderRegion {
                    id: "default".into(),
                    label: "官方服务".into(),
                }],
                models: vec![
                    ProviderModel {
                        id: "deepseek-v4-flash".into(),
                        label: "DeepSeek V4 Flash".into(),
                        recommended: true,
                    },
                    ProviderModel {
                        id: "deepseek-v4-pro".into(),
                        label: "DeepSeek V4 Pro".into(),
                        recommended: false,
                    },
                ],
                selected_region: deepseek
                    .map_or("default", |profile| profile.region.as_str())
                    .into(),
                selected_model: deepseek
                    .map_or("deepseek-v4-flash", |profile| profile.model.as_str())
                    .into(),
                key_configured: deepseek
                    .and_then(|profile| profile.key_last4.as_ref())
                    .is_some(),
                key_last4: deepseek.and_then(|profile| profile.key_last4.clone()),
                connection_verified: deepseek.is_some_and(|profile| profile.connection_verified),
            },
            ProviderSpec {
                id: "kimi".into(),
                label: "Kimi".into(),
                regions: vec![
                    ProviderRegion {
                        id: "cn".into(),
                        label: "中国大陆".into(),
                    },
                    ProviderRegion {
                        id: "global".into(),
                        label: "国际服务".into(),
                    },
                ],
                models: vec![
                    ProviderModel {
                        id: "kimi-k3".into(),
                        label: "Kimi K3".into(),
                        recommended: true,
                    },
                    ProviderModel {
                        id: "kimi-k2.6".into(),
                        label: "Kimi K2.6".into(),
                        recommended: false,
                    },
                ],
                selected_region: kimi.map_or("cn", |profile| profile.region.as_str()).into(),
                selected_model: kimi
                    .map_or("kimi-k3", |profile| profile.model.as_str())
                    .into(),
                key_configured: kimi
                    .and_then(|profile| profile.key_last4.as_ref())
                    .is_some(),
                key_last4: kimi.and_then(|profile| profile.key_last4.clone()),
                connection_verified: kimi.is_some_and(|profile| profile.connection_verified),
            },
        ])
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("连接失败，请重新检查并复制完整 API Key")]
    InvalidKey,
    #[error("当前额度不足，请前往供应商控制台检查账户余额")]
    InsufficientBalance,
    #[error("生成已暂停，请稍后再试")]
    RateLimited,
    #[error("网络请求超时，当前继续使用本地内容")]
    Timeout,
    #[error("模型响应结构异常，已丢弃本批内容")]
    MalformedResponse,
    #[error("模型暂未返回内容，请重新生成本批内容")]
    EmptyContent,
    #[error("模型输出被截断，请重新生成本批内容")]
    TruncatedResponse,
    #[error("模型返回的知识卡 JSON 无法解析，已丢弃本批内容")]
    InvalidJson,
    #[error("模型服务暂时不可用，当前继续使用本地内容")]
    Unavailable,
    #[error("当前网络不可用，正在使用本地内容")]
    Network,
    #[error("请求目标不在官方供应商白名单中")]
    UnsafeTarget,
    #[error("所选模型不在应用内置注册表中")]
    UnsupportedModel,
    #[error("模型内容没有通过本地安全与字段校验")]
    ContentRejected,
}

impl ProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidKey => "invalid_key",
            Self::InsufficientBalance => "insufficient_balance",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::MalformedResponse => "malformed_response",
            Self::EmptyContent => "empty_content",
            Self::TruncatedResponse => "truncated_response",
            Self::InvalidJson => "invalid_json",
            Self::Unavailable => "unavailable",
            Self::Network => "network",
            Self::UnsafeTarget => "unsafe_target",
            Self::UnsupportedModel => "unsupported_model",
            Self::ContentRejected => "content_rejected",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransportResponse {
    pub status: u16,
    pub body: String,
}

#[async_trait]
pub trait ProviderTransport: Send + Sync {
    async fn post_json(
        &self,
        endpoint: &Url,
        api_key: &SecretValue,
        body: &Value,
        timeout: Duration,
    ) -> Result<TransportResponse, ProviderError>;
}

#[derive(Clone)]
pub struct RestrictedHttpClient {
    client: reqwest::Client,
    allowed_hosts: HashSet<&'static str>,
}

impl RestrictedHttpClient {
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .user_agent("TellYouWhy/0.1")
            .build()
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(Self {
            client,
            allowed_hosts: ["api.deepseek.com", "api.moonshot.cn", "api.moonshot.ai"]
                .into_iter()
                .collect(),
        })
    }

    pub fn validate_url(&self, endpoint: &Url) -> Result<(), ProviderError> {
        let host = endpoint.host_str().ok_or(ProviderError::UnsafeTarget)?;
        let standard_https_port = endpoint.port().is_none() || endpoint.port() == Some(443);
        if endpoint.scheme() != "https"
            || !standard_https_port
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || !self.allowed_hosts.contains(host)
        {
            return Err(ProviderError::UnsafeTarget);
        }
        Ok(())
    }
}

#[async_trait]
impl ProviderTransport for RestrictedHttpClient {
    async fn post_json(
        &self,
        endpoint: &Url,
        api_key: &SecretValue,
        body: &Value,
        timeout: Duration,
    ) -> Result<TransportResponse, ProviderError> {
        self.validate_url(endpoint)?;
        let response = self
            .client
            .post(endpoint.clone())
            .bearer_auth(api_key.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(body)
            .timeout(timeout)
            .send()
            .await
            .map_err(classify_transport_error)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(classify_transport_error)?;
        Ok(TransportResponse { status, body })
    }
}

fn classify_transport_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else if error.is_connect() {
        ProviderError::Network
    } else {
        ProviderError::Unavailable
    }
}

#[derive(Debug, Clone)]
pub struct ProviderContext {
    pub endpoint: Url,
    pub model: String,
}

impl ProviderContext {
    pub fn from_registry(
        provider_id: &str,
        region: &str,
        model: &str,
    ) -> Result<Self, ProviderError> {
        let registry = ProviderRegistry::endpoint(provider_id, region)?;
        ProviderRegistry::validate_model(&registry, model)?;
        let endpoint = Url::parse(&format!("{}{}", registry.origin, registry.chat_path))
            .map_err(|_| ProviderError::UnsafeTarget)?;
        Ok(Self {
            endpoint,
            model: model.into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct GenerationRequest {
    pub topics: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub structured_json: bool,
    pub max_cards_per_batch: usize,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn test_connection(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        transport: &dyn ProviderTransport,
    ) -> Result<(), ProviderError>;
    async fn generate_knowledge_cards(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        request: &GenerationRequest,
        transport: &dyn ProviderTransport,
    ) -> Result<Vec<KnowledgeCard>, ProviderError>;
}

pub fn adapter(provider_id: &str) -> Result<Box<dyn ProviderAdapter>, ProviderError> {
    match provider_id {
        "deepseek" => Ok(Box::new(DeepSeekAdapter)),
        "kimi" => Ok(Box::new(KimiAdapter)),
        _ => Err(ProviderError::UnsafeTarget),
    }
}

pub struct DeepSeekAdapter;
pub struct KimiAdapter;

#[async_trait]
impl ProviderAdapter for DeepSeekAdapter {
    fn id(&self) -> &'static str {
        "deepseek"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        common_capabilities(2)
    }

    async fn test_connection(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        transport: &dyn ProviderTransport,
    ) -> Result<(), ProviderError> {
        test_openai_compatible(context, api_key, transport).await
    }

    async fn generate_knowledge_cards(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        request: &GenerationRequest,
        transport: &dyn ProviderTransport,
    ) -> Result<Vec<KnowledgeCard>, ProviderError> {
        generate_openai_compatible(self.id(), context, api_key, request, transport).await
    }
}

#[async_trait]
impl ProviderAdapter for KimiAdapter {
    fn id(&self) -> &'static str {
        "kimi"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        common_capabilities(5)
    }

    async fn test_connection(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        transport: &dyn ProviderTransport,
    ) -> Result<(), ProviderError> {
        test_openai_compatible(context, api_key, transport).await
    }

    async fn generate_knowledge_cards(
        &self,
        context: &ProviderContext,
        api_key: &SecretValue,
        request: &GenerationRequest,
        transport: &dyn ProviderTransport,
    ) -> Result<Vec<KnowledgeCard>, ProviderError> {
        generate_openai_compatible(self.id(), context, api_key, request, transport).await
    }
}

fn common_capabilities(max_cards_per_batch: usize) -> ProviderCapabilities {
    ProviderCapabilities {
        structured_json: true,
        max_cards_per_batch,
    }
}

async fn test_openai_compatible(
    context: &ProviderContext,
    api_key: &SecretValue,
    transport: &dyn ProviderTransport,
) -> Result<(), ProviderError> {
    let body = json!({
        "model": context.model,
        "messages": [
            {
                "role": "system",
                "content": "这是一次最小连接测试。"
            },
            {
                "role": "user",
                "content": "请只回复 OK。"
            }
        ],
        "max_tokens": 3,
        "stream": false
    });
    let response = transport
        .post_json(&context.endpoint, api_key, &body, CONNECTION_TEST_TIMEOUT)
        .await?;
    ensure_success(response.status, &response.body)
}

async fn generate_openai_compatible(
    provider_id: &str,
    context: &ProviderContext,
    api_key: &SecretValue,
    request: &GenerationRequest,
    transport: &dyn ProviderTransport,
) -> Result<Vec<KnowledgeCard>, ProviderError> {
    if request.count == 0 || request.count > 5 || request.topics.is_empty() {
        return Err(ProviderError::ContentRejected);
    }
    let topics = request
        .topics
        .iter()
        .take(8)
        .map(|topic| topic.chars().take(30).collect::<String>())
        .collect::<Vec<_>>()
        .join("、");
    let output_example = json!({
        "cards": [{
            "topicId": "natural_science",
            "topicLabel": "自然科学",
            "tags": ["气体", "溶解度"],
            "question": "为什么刚打开的汽水会迅速冒出许多气泡？",
            "shortAnswer": "密封时的高压让更多二氧化碳溶在液体中；开盖后压力突然下降，气体溶解度随之降低，便从微小成核点析出并形成大量气泡。",
            "explanation": "汽水在灌装时会被加压注入二氧化碳。较高压力使更多气体能够稳定地溶解在液体中，密封瓶内因此暂时看不到大量气泡。开盖后，液面上方的压力迅速降到接近大气压，原先溶解的二氧化碳不再能全部留在水中。气体会优先在瓶壁划痕、灰尘颗粒或晃动形成的微小空隙处聚集，这些位置称为成核点。小气泡形成后继续吸收周围的二氧化碳，体积增大并上浮，所以会看到连续冒泡。温度越高，二氧化碳通常越不容易溶解；摇晃又会增加成核机会，因此温热或刚被摇过的汽水开盖时更容易喷涌。这个解释适用于普通碳酸饮料，实际程度还会受到配方、温度、瓶内压力和容器表面状态影响。",
            "whyItMatters": "它解释了为什么汽水应冷藏并在开盖前保持静置。",
            "difficulty": "general",
            "estimatedReadSeconds": 45
        }]
    })
    .to_string();
    let prompt = format!(
        "请围绕这些低风险知识领域生成 {} 张简体中文知识卡：{}。返回紧凑的 JSON 对象，顶层必须包含 cards 数组；每张卡必须包含 topicId、topicLabel、tags、question、shortAnswer、explanation、whyItMatters、difficulty 和 estimatedReadSeconds。字段名及大小写必须与这个 JSON 结构示例完全一致：{}。question 为 12 至 45 个中文字符，shortAnswer 控制在 40 至 90 个中文字符，explanation 控制在 180 至 280 个中文字符，estimatedReadSeconds 为 30 至 90。difficulty 只能是 beginner、general 或 advanced。必须恰好返回要求的卡片数量。禁止医疗诊断、法律意见、投资建议、实时政治和需要实时数据的问题。不要提供或编造来源链接，不要声称内容已核验。只输出这个 JSON 对象，不要输出 Markdown、解释或其他文字。",
        request.count, topics, output_example
    );
    let mut body = json!({
        "model": context.model,
        "messages": [
            {
                "role": "system",
                "content": "你是克制的知识卡编辑。严格输出合法 JSON，不使用 Markdown，不输出额外文字。"
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "temperature": 0.4,
        "max_tokens": 6000,
        "stream": false,
        "response_format": { "type": "json_object" }
    });
    if provider_id == "deepseek" {
        body["thinking"] = json!({ "type": "disabled" });
    }
    let response = transport
        .post_json(&context.endpoint, api_key, &body, GENERATION_TIMEOUT)
        .await?;
    ensure_success(response.status, &response.body)?;
    let content = extract_content(&response.body)?;
    let cards = parse_generated_content(&content)?;
    finalize_generated_cards(cards, provider_id, &context.model)
        .map_err(|_| ProviderError::ContentRejected)
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    finish_reason: Option<String>,
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeneratedEnvelope {
    cards: Vec<GeneratedCardInput>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GeneratedPayload {
    Envelope(GeneratedEnvelope),
    Cards(Vec<GeneratedCardInput>),
}

fn extract_content(body: &str) -> Result<String, ProviderError> {
    let response: ChatResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::MalformedResponse)?;
    let choice = response
        .choices
        .first()
        .ok_or(ProviderError::MalformedResponse)?;
    match choice.finish_reason.as_deref() {
        Some("content_filter") => return Err(ProviderError::ContentRejected),
        Some("insufficient_system_resource") => return Err(ProviderError::Unavailable),
        Some("length") => return Err(ProviderError::TruncatedResponse),
        _ => {}
    }
    choice
        .message
        .content
        .clone()
        .filter(|content| !content.trim().is_empty())
        .ok_or(ProviderError::EmptyContent)
}

fn parse_generated_content(content: &str) -> Result<Vec<GeneratedCardInput>, ProviderError> {
    let payload: GeneratedPayload =
        serde_json::from_str(strip_json_fence(content)).map_err(|_| ProviderError::InvalidJson)?;
    Ok(match payload {
        GeneratedPayload::Envelope(envelope) => envelope.cards,
        GeneratedPayload::Cards(cards) => cards,
    })
}

fn strip_json_fence(content: &str) -> &str {
    const FENCE: &str = "\x60\x60\x60";
    let trimmed = content.trim();
    if !trimmed.starts_with(FENCE) {
        return trimmed;
    }
    let without_open = trimmed
        .split_once('\n')
        .map_or(trimmed, |(_, remaining)| remaining);
    without_open
        .strip_suffix(FENCE)
        .map(str::trim)
        .unwrap_or(without_open)
}

fn ensure_success(status: u16, body: &str) -> Result<(), ProviderError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let normalized = body.to_ascii_lowercase();
    if status == 401 || status == 403 {
        Err(ProviderError::InvalidKey)
    } else if status == 402
        || normalized.contains("insufficient balance")
        || normalized.contains("insufficient quota")
        || normalized.contains("余额不足")
    {
        Err(ProviderError::InsufficientBalance)
    } else if status == 429 {
        Err(ProviderError::RateLimited)
    } else if status == 408 || status == 504 {
        Err(ProviderError::Timeout)
    } else {
        Err(ProviderError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_store::SecretValue;
    use std::sync::Mutex;

    struct MockTransport {
        response: TransportResponse,
        endpoints: Mutex<Vec<String>>,
        bodies: Mutex<Vec<Value>>,
        timeouts: Mutex<Vec<Duration>>,
    }

    impl MockTransport {
        fn successful_cards() -> Self {
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
            let outer = json!({
                "choices": [{
                    "message": {
                        "content": generated.to_string()
                    }
                }]
            });
            Self {
                response: TransportResponse {
                    status: 200,
                    body: outer.to_string(),
                },
                endpoints: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
                timeouts: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ProviderTransport for MockTransport {
        async fn post_json(
            &self,
            endpoint: &Url,
            _api_key: &SecretValue,
            body: &Value,
            timeout: Duration,
        ) -> Result<TransportResponse, ProviderError> {
            self.endpoints
                .lock()
                .expect("endpoint lock")
                .push(endpoint.to_string());
            self.bodies.lock().expect("body lock").push(body.clone());
            self.timeouts.lock().expect("timeout lock").push(timeout);
            Ok(self.response.clone())
        }
    }

    fn verify_adapter(provider_id: &str, region: &str, expected_host: &str) {
        let adapter = adapter(provider_id).expect("adapter must exist");
        let default_model = ProviderRegistry::endpoint(provider_id, region)
            .expect("endpoint must exist")
            .models[0];
        let context =
            ProviderContext::from_registry(provider_id, region, default_model).expect("context");
        let transport = MockTransport::successful_cards();
        let secret = SecretValue::for_test("sk-mock-only");
        let cards = tauri::async_runtime::block_on(adapter.generate_knowledge_cards(
            &context,
            &secret,
            &GenerationRequest {
                topics: vec!["自然科学".into()],
                count: 1,
            },
            &transport,
        ))
        .expect("mock generation must succeed");
        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].trust_status,
            crate::models::TrustStatus::AiUnverified
        );
        assert!(transport
            .endpoints
            .lock()
            .expect("endpoint lock")
            .first()
            .expect("captured endpoint")
            .contains(expected_host));
        assert_eq!(
            transport.timeouts.lock().expect("timeout lock")[0],
            GENERATION_TIMEOUT
        );
        let bodies = transport.bodies.lock().expect("body lock");
        let body = bodies[0].to_string();
        assert!(!body.contains("sk-mock-only"));
        let captured_body = &bodies[0];
        if provider_id == "deepseek" {
            assert_eq!(captured_body["thinking"]["type"], "disabled");
        } else {
            assert!(captured_body.get("thinking").is_none());
        }
        assert_eq!(captured_body["response_format"]["type"], "json_object");
        let prompt = captured_body["messages"][1]["content"]
            .as_str()
            .expect("generation prompt");
        let example_start = prompt.find('{').expect("JSON example start");
        let example_end = prompt.rfind('}').expect("JSON example end");
        let example_cards = parse_generated_content(&prompt[example_start..=example_end])
            .expect("prompt JSON example should parse");
        finalize_generated_cards(example_cards, provider_id, default_model)
            .expect("prompt JSON example should pass local validation");
    }

    #[test]
    fn deepseek_adapter_passes_mock_generation() {
        verify_adapter("deepseek", "default", "api.deepseek.com");
        assert_eq!(
            adapter("deepseek")
                .expect("adapter")
                .capabilities()
                .max_cards_per_batch,
            2
        );
    }

    #[test]
    fn kimi_adapter_passes_mock_generation() {
        verify_adapter("kimi", "cn", "api.moonshot.cn");
        assert_eq!(
            adapter("kimi")
                .expect("adapter")
                .capabilities()
                .max_cards_per_batch,
            5
        );
    }

    #[test]
    fn registry_rejects_arbitrary_origins_and_models() {
        assert!(matches!(
            ProviderRegistry::endpoint("custom", "https://example.com"),
            Err(ProviderError::UnsafeTarget)
        ));
        assert!(matches!(
            ProviderContext::from_registry("deepseek", "default", "made-up-model"),
            Err(ProviderError::UnsupportedModel)
        ));
    }

    #[test]
    fn provider_errors_are_normalized_without_raw_body() {
        assert!(matches!(
            ensure_success(401, "token sk-should-never-be-shown"),
            Err(ProviderError::InvalidKey)
        ));
        assert_eq!(
            ProviderError::InvalidKey.to_string(),
            "连接失败，请重新检查并复制完整 API Key"
        );
    }

    #[test]
    fn restricted_client_rejects_non_whitelisted_url() {
        let client = RestrictedHttpClient::new().expect("client");
        assert!(client
            .validate_url(&Url::parse("https://example.com/chat").expect("url"))
            .is_err());
        assert!(client
            .validate_url(&Url::parse("https://api.deepseek.com/chat/completions").expect("url"))
            .is_ok());
    }

    #[test]
    fn parser_accepts_direct_array_and_snake_case_fields() {
        let content = json!([{
            "topic_id": "natural_science",
            "topic_label": "自然科学",
            "tags": ["水"],
            "question": "为什么水结冰以后体积反而会变得更大？",
            "short_answer": "水分子结冰时会形成带有规则空隙的晶体结构，所以同样质量的冰会占据更大的体积，密度也因此低于液态水并通常浮在水面。",
            "explanation": "液态水中的分子仍可移动并相对紧密地排列。温度下降到冰点附近时，氢键把水分子固定到带有规则空隙的晶格中。晶格占据的空间更大，因此水结冰时体积增加，密度也低于液态水。这个现象同时解释了冰为什么通常会浮在水面，也影响了寒冷地区的岩石风化与水体生态。水的密度还会随温度改变，实际结冰过程也会受到溶质、压力和成核条件影响，因此这个规律需要在具体环境中理解。此外，水分子排列并非瞬间完成，冷却速度也会影响晶体形成方式。",
            "why_it_matters": "这会影响湖泊结冰方式和寒冷地区的自然环境。",
            "difficulty": "beginner",
            "estimated_read_seconds": 50
        }])
        .to_string();
        let cards = parse_generated_content(&content).expect("array should parse");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].topic_id, "natural_science");
    }

    #[test]
    fn provider_content_failures_are_classified_without_raw_body() {
        let truncated = json!({
            "choices": [{
                "finish_reason": "length",
                "message": {
                    "content": null,
                    "reasoning_content": "sensitive provider reasoning"
                }
            }]
        });
        assert!(matches!(
            extract_content(&truncated.to_string()),
            Err(ProviderError::TruncatedResponse)
        ));

        let empty = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "content": ""
                }
            }]
        });
        assert!(matches!(
            extract_content(&empty.to_string()),
            Err(ProviderError::EmptyContent)
        ));
        assert!(matches!(
            parse_generated_content("{not valid json"),
            Err(ProviderError::InvalidJson)
        ));
    }
}
