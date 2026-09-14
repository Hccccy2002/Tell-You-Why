//! Tool contracts and execution policy. Domain effects are committed by review_store.
use super::policy::{self, StopReason};
use crate::{
    db::Database,
    review_agent::{ReviewLibrary, ReviewQuestion, ReviewRun, ToolCall},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

pub const VERSION: &str = "review-tools-v2";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    ReadOnly,
    WriteReview,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    pub read: bool,
    pub write_review: bool,
}
impl Default for Permissions {
    fn default() -> Self {
        Self {
            read: true,
            write_review: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownTool,
    InvalidArguments,
    PermissionDenied,
    InvalidReference,
    ScopeMismatch,
    PreconditionFailed,
    AnswerNotSubmitted,
    SourceRequired,
    DependencyUnavailable,
    DependencyFailed,
    InvalidResult,
    StorageFailed,
    OutputLimit,
    Timeout,
    Cancelled,
    CleanupFailed,
}

#[derive(Clone, Debug, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
}
impl ToolError {
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn retryable(&self) -> bool {
        matches!(
            self.code,
            ErrorCode::DependencyUnavailable | ErrorCode::Timeout
        )
    }
    pub fn envelope(&self) -> Value {
        json!({"ok":false,"error":self.message,"error_code":self.code,"retryable":self.retryable()})
    }
    pub fn storage(_: crate::db::DbError) -> Self {
        Self::new(ErrorCode::StorageFailed, "无法读取复习数据，请稍后重试")
    }
    pub fn dependency(_: String) -> Self {
        Self::new(
            ErrorCode::DependencyFailed,
            "本地资料服务执行失败，请检查资料状态",
        )
    }
}

pub struct ToolSpec {
    pub name: &'static str,
    pub access: Access,
    pub timeout_ms: u64,
    pub max_attempts: usize,
    pub max_output_bytes: usize,
    pub definition: Value,
}
impl ToolSpec {
    pub fn retry(&self, error: &ToolError, attempts: usize) -> bool {
        self.access == Access::ReadOnly && error.retryable() && attempts < self.max_attempts
    }
    pub fn authorize(&self, permissions: &Permissions) -> Result<(), ToolError> {
        let allowed = match self.access {
            Access::ReadOnly => permissions.read,
            Access::WriteReview => permissions.write_review,
        };
        if allowed {
            Ok(())
        } else {
            Err(ToolError::new(
                ErrorCode::PermissionDenied,
                "当前任务未授权此类工具操作",
            ))
        }
    }
}

pub fn registry() -> Vec<ToolSpec> {
    let string = json!({"type":"string","minLength":1});
    let define = |name,
                  description,
                  access,
                  timeout_ms,
                  max_attempts,
                  properties: Value,
                  required: Value| ToolSpec {
        name,
        access,
        timeout_ms,
        max_attempts,
        max_output_bytes: 65_536,
        definition: json!({"type":"function","function":{"name":name,"description":description,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}}),
    };
    vec![
        define("get_learning_progress", "读取当前教材范围内的真实学习记录和近期答题结果。", Access::ReadOnly, 5_000, 1, json!({}), json!([])),
        define("search_textbook", "在当前教材与章节中检索。返回原文片段编号及摘要；可更换关键词重试。", Access::ReadOnly, 60_000, 2, json!({"query":{"type":"string","minLength":1,"maxLength":1000},"mode":{"type":"string","enum":["keyword","hybrid"]}}), json!(["query","mode"])),
        define("read_source", "按 source_id 读取原文，每次最多 2000 字符。长原文通过 start_char 从 0 开始分段读取，has_more 为 true 时用 next_start_char 继续。编号仅限本次任务已检索到的材料。", Access::ReadOnly, 5_000, 1, json!({"source_id":string,"start_char":{"type":"integer","minimum":0},"max_chars":{"type":"integer","minimum":1,"maximum":2000}}), json!(["source_id"])),
        define("save_review_question", "保存一道选择题并等待用户作答。source_ids 仅使用已检索编号，无原文则为空数组；card_id 可关联学习记录中的卡片。复习已有知识点时，memory_id 必须使用 get_learning_progress 返回的知识点 id。", Access::WriteReview, 5_000, 1, json!({"topic":{"type":"string","minLength":1,"maxLength":100},"question":{"type":"string","minLength":1,"maxLength":1000},"options":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":2,"maxItems":4,"uniqueItems":true},"correct_index":{"type":"integer","minimum":0,"maximum":3},"explanation":{"type":"string","minLength":1,"maxLength":4000},"source_ids":{"type":"array","items":string,"maxItems":36},"card_id":{"type":["string","null"]},"memory_id":{"type":["string","null"]}}), json!(["topic","question","options","correct_index","explanation"])),
        define("record_quiz_result", "根据用户实际提交的选项记录答题结果。仅传题目编号，不能替用户填写答案或分数。", Access::WriteReview, 5_000, 1, json!({"question_id":string}), json!(["question_id"])),
    ]
}
pub fn lookup(name: &str) -> Result<ToolSpec, ToolError> {
    registry()
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| ToolError::new(ErrorCode::UnknownTool, "未知工具，请使用提供的工具列表"))
}
pub fn definitions() -> Value {
    json!(registry()
        .into_iter()
        .map(|s| s.definition)
        .collect::<Vec<_>>())
}

// This validator implements only the schema vocabulary emitted by registry().
// Rust domain handlers separately enforce references and state preconditions.
fn conforms(value: &Value, schema: &Value) -> bool {
    let matches_type = |kind: &str| match kind {
        "string" => value.is_string(),
        "null" => value.is_null(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "integer" => value.as_u64().is_some(),
        _ => false,
    };
    let typed = schema["type"]
        .as_str()
        .map(matches_type)
        .unwrap_or_else(|| {
            schema["type"]
                .as_array()
                .is_some_and(|types| types.iter().any(|t| t.as_str().is_some_and(matches_type)))
        });
    if !typed
        || schema["enum"]
            .as_array()
            .is_some_and(|items| !items.contains(value))
    {
        return false;
    }
    if let Some(s) = value.as_str() {
        let size = s.chars().count() as u64;
        if s.trim().is_empty() && schema["minLength"].as_u64().unwrap_or(0) > 0 {
            return false;
        }
        if size < schema["minLength"].as_u64().unwrap_or(0)
            || size > schema["maxLength"].as_u64().unwrap_or(u64::MAX)
        {
            return false;
        }
    }
    if let Some(n) = value.as_u64() {
        if n < schema["minimum"].as_u64().unwrap_or(0)
            || n > schema["maximum"].as_u64().unwrap_or(u64::MAX)
        {
            return false;
        }
    }
    if let Some(items) = value.as_array() {
        if items.len() < schema["minItems"].as_u64().unwrap_or(0) as usize
            || items.len() > schema["maxItems"].as_u64().unwrap_or(u64::MAX) as usize
        {
            return false;
        }
        if schema["uniqueItems"] == true
            && items
                .iter()
                .enumerate()
                .any(|(i, v)| items[..i].contains(v))
        {
            return false;
        }
        if !items.iter().all(|item| conforms(item, &schema["items"])) {
            return false;
        }
    }
    if let Some(object) = value.as_object() {
        if schema["required"].as_array().is_some_and(|keys| {
            keys.iter()
                .any(|key| !object.contains_key(key.as_str().unwrap_or("")))
        }) {
            return false;
        }
        if !object.iter().all(|(key, v)| {
            schema["properties"]
                .get(key)
                .is_some_and(|s| conforms(v, s))
        }) {
            return false;
        }
    }
    true
}

#[cfg(test)]
pub async fn execute(
    db: &Database,
    library: &dyn ReviewLibrary,
    run: &mut ReviewRun,
    call: &ToolCall,
) -> Result<(Value, Option<ReviewQuestion>), ToolError> {
    let scope = super::execution::ExecutionScope::new(lookup(&call.name)?.timeout_ms);
    let result = execute_in(db, library, run, call, &scope.control).await;
    scope.finish().await?;
    result
}

async fn execute_in(
    db: &Database,
    library: &dyn ReviewLibrary,
    run: &mut ReviewRun,
    call: &ToolCall,
    control: &super::execution::ExecutionControl,
) -> Result<(Value, Option<ReviewQuestion>), ToolError> {
    control.check()?;
    let spec = lookup(&call.name)?;
    spec.authorize(&run.control.tool_permissions)?;
    let invalid = || {
        ToolError::new(
            ErrorCode::InvalidArguments,
            "工具参数格式错误，请按工具定义修正",
        )
    };
    if call.arguments.len() > 32_768 {
        return Err(invalid());
    }
    let input: Value = serde_json::from_str(&call.arguments).map_err(|_| invalid())?;
    if !conforms(&input, &spec.definition["function"]["parameters"]) {
        return Err(invalid());
    }
    let mut candidate = run.clone();
    let result = crate::review_tools::execute(db, library, &mut candidate, call, control).await?;
    control.check()?;
    if result.0.to_string().len() > spec.max_output_bytes {
        return Err(ToolError::new(
            ErrorCode::OutputLimit,
            "工具结果过长，请缩小查询范围",
        ));
    }
    *run = candidate;
    Ok(result)
}

/// Claim-time verification participates in cancellation before the first model call.
pub async fn verify_version(
    db: &Database,
    library: &dyn ReviewLibrary,
    run: &mut ReviewRun,
) -> Result<(), StopReason> {
    let checkpoint =
        |_| StopReason::new("checkpoint_unavailable", "无法读取或保存复习检查点", true);
    if db
        .review_load(&run.id)
        .map_err(checkpoint)?
        .cancel_requested
    {
        return Err(StopReason::cancelled());
    }
    run.control.check(run.model_calls, run.tool_calls, true)?;
    if !run.control.tool_permissions.read {
        return Err(StopReason::new(
            "permission_denied",
            "当前任务未授权读取教材",
            false,
        ));
    }
    let maximum = run.control.policy.tool_timeout_ms.min(60_000);
    let allowance = run.control.reserve_time(maximum);
    let seq = run.trace_begin("state", "verify_textbook", json!({}));
    db.review_save(run, None).map_err(checkpoint)?;
    let scope = super::execution::ExecutionScope::new(allowance);
    let started = Instant::now();
    let result = policy::guarded(
        db,
        &run.id,
        allowance,
        allowance < maximum,
        library.call_controlled(
            json!({"op":"learning_version","kb":run.scope.kb,"version":run.scope.version}),
            scope.control.clone(),
        ),
    )
    .await;
    let cleanup = scope.finish().await;
    run.control.settle_time(allowance, started.elapsed());
    run.trace[seq - 1].details["process_cleanup"] = json!(if cleanup.is_ok() {
        "confirmed"
    } else {
        "unconfirmed"
    });
    if cleanup.is_err() {
        return Err(StopReason::new(
            "process_cleanup_failed",
            "资料进程清理失败，已停止任务",
            false,
        ));
    }
    if db
        .review_load(&run.id)
        .map_err(checkpoint)?
        .cancel_requested
    {
        return Err(StopReason::cancelled());
    }
    let value = result?.map_err(|error| {
        if error.code == ErrorCode::Timeout && allowance < maximum {
            StopReason::time_limit()
        } else {
            StopReason::new(
                if error.code == ErrorCode::CleanupFailed {
                    "process_cleanup_failed"
                } else {
                    "textbook_check_failed"
                },
                &error.message,
                error.retryable(),
            )
        }
    })?;
    if value["version"] != run.scope.version {
        return Err(StopReason::new(
            "scope_mismatch",
            "资料版本已变化，请重新开始复习",
            false,
        ));
    }
    run.trace_finish(seq, "succeeded");
    db.review_save(run, None).map_err(checkpoint)?;
    Ok(())
}

/// Execute one pending call, including bounded read retries and atomic checkpointing.
pub async fn next(
    db: &Database,
    library: &dyn ReviewLibrary,
    run: &mut ReviewRun,
) -> Result<(), StopReason> {
    let checkpoint = |_| {
        StopReason::new(
            "checkpoint_unavailable",
            "无法保存复习检查点，请稍后继续",
            true,
        )
    };
    let call = run.pending[0].clone();
    let spec = lookup(&call.name).ok();
    loop {
        if db
            .review_load(&run.id)
            .map_err(checkpoint)?
            .cancel_requested
        {
            return Err(StopReason::cancelled());
        }
        run.control.check(run.model_calls, run.tool_calls, false)?;
        run.tool_calls += 1;
        let attempts = run
            .control
            .tool_attempts
            .entry(call.id.clone())
            .or_default();
        *attempts += 1;
        let attempt = *attempts;
        let mut details = crate::review_trace::tool_details(&call);
        details["runtime_version"] = json!(VERSION);
        details["attempt"] = json!(attempt);
        details["access"] = json!(spec.as_ref().map(|s| s.access));
        let seq = run.trace_begin("tool", &call.name, details);
        let maximum = run
            .control
            .policy
            .tool_timeout_ms
            .min(spec.as_ref().map_or(5_000, |s| s.timeout_ms));
        let allowance = run.control.reserve_time(maximum);
        db.review_save(run, None).map_err(checkpoint)?;
        let mut candidate = run.clone();
        let started = Instant::now();
        let scope = super::execution::ExecutionScope::new(allowance);
        let outcome = policy::guarded(
            db,
            &run.id,
            allowance,
            allowance < maximum,
            execute_in(db, library, &mut candidate, &call, &scope.control),
        )
        .await;
        let cleanup = scope.finish().await;
        run.control.settle_time(allowance, started.elapsed());
        run.trace[seq - 1].details["process_cleanup"] = json!(if cleanup.is_ok() {
            "confirmed"
        } else {
            "unconfirmed"
        });
        candidate.trace[seq - 1].details["process_cleanup"] =
            run.trace[seq - 1].details["process_cleanup"].clone();
        if cleanup.is_err() {
            return Err(StopReason::new(
                "process_cleanup_failed",
                "无法确认资料进程已结束，已停止任务",
                false,
            ));
        }
        if db
            .review_load(&run.id)
            .map_err(checkpoint)?
            .cancel_requested
        {
            return Err(StopReason::cancelled());
        }
        candidate.control = run.control.clone();
        let outcome = match outcome {
            Ok(value) => value,
            Err(reason) if reason.code == "operation_timeout" => Err(ToolError::new(
                ErrorCode::Timeout,
                "工具执行超时，请稍后重试",
            )),
            Err(reason) => return Err(reason),
        };
        if matches!(&outcome, Err(error) if error.code == ErrorCode::CleanupFailed) {
            return Err(StopReason::new(
                "process_cleanup_failed",
                "资料进程清理失败，已停止任务",
                false,
            ));
        }
        if matches!(&outcome, Err(error) if error.code == ErrorCode::Timeout) && allowance < maximum
        {
            return Err(StopReason::time_limit());
        }
        let retry = outcome
            .as_ref()
            .err()
            .is_some_and(|e| spec.as_ref().is_some_and(|s| s.retry(e, attempt)));
        let (output, graded) = match outcome {
            Ok((data, graded)) => (json!({"ok":true,"data":data}), graded),
            Err(error) => {
                candidate = run.clone();
                (error.envelope(), None)
            }
        };
        candidate.trace_finish(
            seq,
            if output["ok"] == true {
                "succeeded"
            } else {
                "failed"
            },
        );
        candidate.trace[seq - 1].details["result"] =
            crate::review_trace::tool_result(&call.name, &output);
        if retry {
            let retry_seq = candidate.trace_begin("policy", "tool_retry_scheduled", json!({"call_id":call.id,"tool":call.name,"error_code":output["error_code"],"next_attempt":attempt+1}));
            candidate.trace_finish(retry_seq, "succeeded");
            db.review_save(&candidate, None).map_err(checkpoint)?;
            *run = candidate;
            continue;
        }
        candidate.pending.remove(0);
        candidate
            .messages
            .push(json!({"role":"tool","tool_call_id":call.id,"content":output.to_string()}));
        let stalled = candidate
            .control
            .observe(&call.name, &call.arguments, &output);
        db.review_save(&candidate, graded.as_ref())
            .map_err(checkpoint)?;
        *run = candidate;
        return stalled.map_or(Ok(()), Err);
    }
}
