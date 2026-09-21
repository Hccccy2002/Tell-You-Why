use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    Off,
    Auto,
    Always,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchOptions {
    pub mode: SearchMode,
    pub engine: String,
    pub daily_attempt_limit: u32,
}
impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            mode: SearchMode::Off,
            engine: "search_pro".into(),
            daily_attempt_limit: 50,
        }
    }
}
impl SearchOptions {
    pub fn validate(&self) -> Result<(), SearchError> {
        if !["search_pro", "search_std"].contains(&self.engine.as_str())
            || !(1..=500).contains(&self.daily_attempt_limit)
        {
            return Err(SearchError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub query: String,
    pub engine: String,
    pub recency: String,
    pub domain: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidence {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub snippet: String,
    pub publisher: Option<String>,
    pub published_at: Option<String>,
    pub retrieved_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnswerBlock {
    pub text: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchAnswer {
    pub run_id: String,
    pub status: String,
    pub as_of: String,
    pub retrieved_at: String,
    pub sources: Vec<WebEvidence>,
    pub blocks: Vec<AnswerBlock>,
    pub limitation: Option<String>,
    pub cache_hit: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchError {
    #[error("搜索参数无效")]
    InvalidRequest,
    #[error("智谱 API Key 无效，请重新配置")]
    InvalidKey,
    #[error("智谱账户余额不足，请充值后重试")]
    Balance,
    #[error("此 Key 没有搜索接口权限，请使用智谱开放平台 API Key")]
    Permission,
    #[error("智谱搜索请求受限，请稍后重试")]
    RateLimited,
    #[error("智谱搜索服务暂不可用，请稍后重试")]
    Unavailable,
    #[error("搜索请求超时，请稍后重试")]
    Timeout,
    #[error("搜索响应格式异常")]
    Malformed,
    #[error("智谱返回了 {received} 条结果，但检查的结果中有 {without_text} 条缺少摘要、{outside_domain} 条不符合指定来源范围，无法整理回答")]
    UnusableResults {
        received: usize,
        without_text: usize,
        outside_domain: usize,
    },
    #[error("本次搜索已取消")]
    Cancelled,
    #[error("已达到本次或今日搜索次数上限")]
    Budget,
    #[error("搜索记录暂时无法保存，请稍后重试")]
    Storage,
}
impl SearchError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidKey => "invalid_key",
            Self::Balance => "insufficient_balance",
            Self::Permission => "permission_denied",
            Self::RateLimited => "rate_limited",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Malformed => "malformed_response",
            Self::UnusableResults { .. } => "unusable_results",
            Self::Cancelled => "cancelled",
            Self::Budget => "budget_exceeded",
            Self::Storage => "storage_error",
        }
    }
}
