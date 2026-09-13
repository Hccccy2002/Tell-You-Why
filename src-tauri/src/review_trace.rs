use crate::review_agent::{ReviewRun, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub sequence: usize,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub details: Value,
    pub usage: Option<Value>,
}

impl ReviewRun {
    pub fn trace_begin(&mut self, kind: &str, name: &str, details: Value) -> usize {
        let sequence = self.trace.len() + 1;
        self.trace.push(TraceEvent {
            sequence,
            kind: kind.into(),
            name: name.into(),
            status: "started".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            duration_ms: None,
            details,
            usage: None,
        });
        sequence
    }

    pub fn trace_finish(&mut self, sequence: usize, status: &str) {
        if let Some(event) = self.trace.iter_mut().find(|e| e.sequence == sequence) {
            let now = chrono::Utc::now();
            event.status = status.into();
            event.finished_at = Some(now.to_rfc3339());
            event.duration_ms = chrono::DateTime::parse_from_rfc3339(&event.started_at)
                .ok()
                .map(|start| {
                    (now - start.with_timezone(&chrono::Utc))
                        .num_milliseconds()
                        .max(0)
                });
        }
    }

    pub fn trace_state(&mut self, name: &str) {
        let seq = self.trace_begin("state", name, json!({}));
        self.trace_finish(seq, "succeeded");
    }

    pub fn trace_close_open(&mut self, status: &str) {
        let open: Vec<_> = self
            .trace
            .iter()
            .filter(|e| e.status == "started")
            .map(|e| e.sequence)
            .collect();
        for seq in open {
            self.trace_finish(seq, status);
        }
    }

    pub fn trace_usage(&mut self, sequence: usize, usage: &Value) {
        let clean = fields(
            usage,
            &["prompt_tokens", "completion_tokens", "total_tokens"],
        );
        let numeric: serde_json::Map<String, Value> = clean
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, value)| value.as_u64().is_some())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if !numeric.is_empty() {
            if let Some(event) = self.trace.iter_mut().find(|e| e.sequence == sequence) {
                event.usage = Some(Value::Object(numeric));
            }
        }
    }

    pub fn trace_document(&self) -> Value {
        let model_events: Vec<_> = self.trace.iter().filter(|e| e.kind == "model").collect();
        let reported: Vec<_> = model_events
            .iter()
            .filter_map(|e| e.usage.as_ref().and_then(|v| v["total_tokens"].as_u64()))
            .collect();
        json!({"schema_version":"review-trace-v1","run":self.summary(),"prompt_version":self.prompt_version,
            "model_requests":self.model_calls,"tool_calls":self.tool_calls,
            "reported_total_tokens":if reported.is_empty(){None}else{Some(reported.iter().sum::<u64>())},
            "usage_reported_requests":reported.len(),"events":self.trace})
    }

    pub fn summary(&self) -> Value {
        json!({"id":self.id,"kb":self.scope.kb,"version":self.scope.version,"goal":self.goal,"state":self.state,
            "created_at":self.created_at,"provider":self.provider,"model":self.model,"question_count":self.questions.len()})
    }
}

fn fields(value: &Value, allowed: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    for key in allowed {
        if let Some(item) = value.get(key) {
            result.insert((*key).into(), item.clone());
        }
    }
    Value::Object(result)
}

// Export only useful operational fields. Never export prompts, model reasoning,
// raw provider payloads, or the answer key/explanation hidden before submission.
pub fn tool_details(call: &ToolCall) -> Value {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let allowed: &[&str] = match call.name.as_str() {
        "search_textbook" => &["query", "mode"],
        "read_source" => &["source_id"],
        "save_review_question" => &["topic", "source_ids", "card_id", "memory_id"],
        "record_quiz_result" => &["question_id"],
        _ => &[],
    };
    // Invalid or oversized arguments still produce a compact trace entry.
    let clean = fields(&args, allowed);
    json!({"call_id":call.id,"arguments":if clean.to_string().chars().count()<=2000{clean}else{json!({"omitted":"参数过长"})}})
}

pub fn tool_result(name: &str, output: &Value) -> Value {
    if output["ok"] != true {
        let error = output["error"].as_str().unwrap_or("");
        let category = if error.contains("参数") {
            "invalid_arguments"
        } else if error.contains("尚未提交") {
            "answer_not_submitted"
        } else if error.contains("编号") {
            "invalid_reference"
        } else if error.contains("章节") || error.contains("范围") || error.contains("版本") {
            "scope_mismatch"
        } else if error.contains("未知工具") {
            "unknown_tool"
        } else {
            "tool_error"
        };
        return json!({"error_code":category,"message":"工具执行失败，错误已返回模型以便修正"});
    }
    let data = &output["data"];
    match name {
        "get_learning_progress" => {
            json!({"cards":data["cards"].as_array().map_or(0, Vec::len),"quiz_results":data["recent_quiz_results"].as_array().map_or(0, Vec::len)})
        }
        "search_textbook" => {
            json!({"matches":data["matches"].as_array().map(|matches|matches.iter().map(|m|fields(m,&["source_id","page"])).collect::<Vec<_>>()).unwrap_or_default()})
        }
        "read_source" => fields(data, &["id", "page", "block_id"]),
        "save_review_question" => fields(data, &["question_id", "saved"]),
        "record_quiz_result" => fields(data, &["question_id", "selected_index", "correct"]),
        _ => json!({}),
    }
}
