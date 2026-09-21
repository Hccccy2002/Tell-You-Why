use super::{types::*, ENDPOINT};
use crate::{
    providers::{ProviderError, ProviderTransport},
    secret_store::SecretValue,
};
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};
use url::Url;

#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(
        &self,
        request: &SearchRequest,
        secret: &SecretValue,
        http: &dyn ProviderTransport,
    ) -> Result<Vec<WebEvidence>, SearchError>;
}
pub struct ZhipuSearch;

pub fn request_body(request: &SearchRequest) -> Result<Value, SearchError> {
    if request.query.trim().is_empty()
        || request.query.chars().count() > 70
        || request.query.chars().any(char::is_control)
        || !["search_pro", "search_std"].contains(&request.engine.as_str())
        || !["oneDay", "oneWeek", "oneMonth", "oneYear", "noLimit"]
            .contains(&request.recency.as_str())
    {
        return Err(SearchError::InvalidRequest);
    }
    let mut body = json!({"search_query":request.query.trim(),"search_engine":request.engine,
        "search_intent":false,"count":5,"search_recency_filter":request.recency,"content_size":"medium",
        "request_id":uuid::Uuid::new_v4().to_string()});
    if let Some(domain) = &request.domain {
        let parsed =
            Url::parse(&format!("https://{domain}")).map_err(|_| SearchError::InvalidRequest)?;
        if parsed.host_str() != Some(domain.as_str())
            || parsed.path() != "/"
            || parsed.port().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(SearchError::InvalidRequest);
        }
        body["search_domain_filter"] = json!(domain);
    }
    Ok(body)
}

pub fn parse_response(
    status: u16,
    body: &str,
    request: &SearchRequest,
) -> Result<Vec<WebEvidence>, SearchError> {
    if body.len() > 1_000_000 {
        return Err(SearchError::Malformed);
    }
    let value: Value = serde_json::from_str(body).map_err(|_| SearchError::Malformed)?;
    let code = value["error"]["code"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value["error"]["code"].to_string());
    match code.as_str() {
        "1113" => return Err(SearchError::Balance),
        "1000" | "1001" | "1002" | "1003" | "1004" => return Err(SearchError::InvalidKey),
        "1311" | "1315" => return Err(SearchError::Permission),
        "1701" | "1302" => return Err(SearchError::RateLimited),
        "1702" => return Err(SearchError::Unavailable),
        "1703" => return Ok(vec![]),
        _ => {}
    }
    if !(200..300).contains(&status) || value.get("error").is_some() {
        return Err(match status {
            401 => SearchError::InvalidKey,
            403 => SearchError::Permission,
            429 => SearchError::RateLimited,
            _ => SearchError::Unavailable,
        });
    }
    let rows = value["search_result"]
        .as_array()
        .ok_or(SearchError::Malformed)?;
    if value["search_intent"]
        .as_array()
        .is_some_and(|a| a.iter().any(|i| i["intent"] == "SEARCH_NONE"))
    {
        return Err(SearchError::Malformed);
    }
    let now = chrono::Utc::now();
    let today = chrono::Local::now().date_naive();
    let mut seen = HashSet::new();
    let mut evidence = vec![];
    let mut without_text = 0;
    let mut outside_domain = 0;
    for (index, row) in rows.iter().take(50).enumerate() {
        let snippet = clean(row["content"].as_str().unwrap_or_default(), 1800);
        if snippet.is_empty() {
            without_text += 1;
            continue;
        }
        let url = row["link"].as_str().and_then(usable_source_url);
        if let Some(domain) = &request.domain {
            let matches = url
                .as_ref()
                .and_then(Url::host_str)
                .is_some_and(|host| host == domain || host.ends_with(&format!(".{domain}")));
            if !matches {
                outside_domain += 1;
                continue;
            }
        }
        let mut title = clean(row["title"].as_str().unwrap_or_default(), 200);
        if title.is_empty() {
            title = format!("智谱搜索摘要 {}", index + 1);
        }
        let published = row["publish_date"]
            .as_str()
            .and_then(|date| date.get(..10))
            .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            .filter(|day| *day <= today);
        // A page's publication date is not the date of every fact on that page.
        // Keep usable search results, including rolling pages and undated snippets,
        // so the answer model can assess the content's own observation/forecast dates.
        let dedup_key = url.as_ref().map_or_else(
            || format!("text:{}", json!([title, snippet])),
            |url| format!("url:{url}"),
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        evidence.push(WebEvidence {
            id: format!("W{}", evidence.len() + 1),
            title,
            url: url.map(|url| url.to_string()),
            snippet,
            publisher: row["media"]
                .as_str()
                .map(|v| clean(v, 100))
                .filter(|v| !v.is_empty()),
            published_at: published.map(|v| v.to_string()),
            retrieved_at: now.to_rfc3339(),
        });
        if evidence.len() == 8 {
            break;
        }
    }
    if evidence.is_empty() && !rows.is_empty() {
        return Err(SearchError::UnusableResults {
            received: rows.len(),
            without_text,
            outside_domain,
        });
    }
    Ok(evidence)
}

fn usable_source_url(value: &str) -> Option<Url> {
    let mut url = Url::parse(value.trim()).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}
fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(limit)
        .collect::<String>()
        .trim()
        .into()
}

#[async_trait::async_trait]
impl SearchProvider for ZhipuSearch {
    async fn search(
        &self,
        request: &SearchRequest,
        secret: &SecretValue,
        http: &dyn ProviderTransport,
    ) -> Result<Vec<WebEvidence>, SearchError> {
        let response = http
            .post_json(
                &Url::parse(ENDPOINT).unwrap(),
                secret,
                &request_body(request)?,
                Duration::from_secs(10),
            )
            .await
            .map_err(|e| match e {
                ProviderError::Timeout => SearchError::Timeout,
                _ => SearchError::Unavailable,
            })?;
        parse_response(response.status, &response.body, request)
    }
}
