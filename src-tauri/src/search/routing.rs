use serde::{Deserialize, Serialize};

pub fn is_current(question: &str) -> bool {
    let q = question.to_lowercase();
    [
        "最新",
        "现在",
        "目前",
        "当前",
        "今天",
        "今日",
        "近期",
        "今年",
        "昨天",
        "新闻",
        "价格",
        "汇率",
        "股价",
        "天气",
        "排名",
        "在任",
        "还在维护",
        "停止支持",
        "过时",
        "现行",
        "latest",
        "current",
        "today",
        "price",
        "maintained",
        "deprecated",
        "news",
    ]
    .iter()
    .any(|v| q.contains(v))
}
pub fn explicit_search(question: &str) -> bool {
    [
        "联网",
        "搜索",
        "查证",
        "核查",
        "查一下",
        "search",
        "look up",
    ]
    .iter()
    .any(|v| question.to_lowercase().contains(v))
}
pub fn stable(question: &str) -> bool {
    !is_current(question)
        && !explicit_search(question)
        && [
            "为什么",
            "原理",
            "解释",
            "是什么",
            "什么是",
            "定义",
            "区别",
            "如何理解",
            "举例",
            "why",
            "explain",
            "what is",
        ]
        .iter()
        .any(|v| question.to_lowercase().contains(v))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryPlan {
    pub needs_search: bool,
    pub query: String,
    pub recency: String,
}
impl QueryPlan {
    pub fn validate(&self) -> bool {
        (!self.needs_search
            || (!self.query.trim().is_empty()
                && self.query.chars().count() <= 70
                && !self.query.chars().any(char::is_control)
                && !self.query.contains('@')
                && !self.query.contains("://")
                && !self.query.contains("\\")
                && !self
                    .query
                    .split(|c: char| !c.is_ascii_digit())
                    .any(|s| s.len() >= 7)))
            && ["noLimit", "oneDay", "oneWeek", "oneMonth", "oneYear"]
                .contains(&self.recency.as_str())
    }
}
