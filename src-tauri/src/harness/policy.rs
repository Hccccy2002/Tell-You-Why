use crate::{db::Database, providers::ProviderError, review_agent::ReviewRun};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    time::{Duration, Instant},
};

/// Snapshotted per run. Waiting for a human never consumes active execution time.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RunPolicy {
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_token_charge: u64,
    pub max_active_ms: u64,
    pub model_timeout_ms: u64,
    pub tool_timeout_ms: u64,
    pub model_retries: usize,
    pub retry_delay_ms: u64,
    pub max_repeated_observations: usize,
    pub turns_per_slice: usize,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            max_model_calls: crate::review_agent::MAX_MODEL_CALLS,
            max_tool_calls: crate::review_agent::MAX_TOOL_CALLS,
            max_token_charge: 400_000,
            max_active_ms: 300_000,
            model_timeout_ms: 90_000,
            tool_timeout_ms: 60_000,
            model_retries: 1,
            retry_delay_ms: 500,
            max_repeated_observations: 3,
            turns_per_slice: 6,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StopReason {
    pub code: String,
    pub message: String,
    pub resumable: bool,
}

impl StopReason {
    pub fn new(code: &str, message: &str, resumable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            resumable,
        }
    }
    pub fn cancelled() -> Self {
        Self::new("user_pause", "复习已暂停，已完成的步骤已保存", true)
    }
    pub fn time_limit() -> Self {
        Self::new(
            "active_time_limit",
            "本次复习已达到执行时间上限，请开始新的复习",
            false,
        )
    }
    pub fn state(&self) -> &'static str {
        if !self.resumable {
            "stopped"
        } else if ["user_pause", "slice_complete", "interrupted"].contains(&self.code.as_str()) {
            "paused"
        } else {
            "failed"
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RunControl {
    pub policy: RunPolicy,
    /// Includes reservations for requests whose outcome was lost during a crash.
    pub charged_tokens: u64,
    pub charged_active_ms: u64,
    pub stop: Option<StopReason>,
    pub last_observation: Option<String>,
    pub repeated_observations: usize,
    pub tool_permissions: super::tools::Permissions,
    pub tool_attempts: std::collections::BTreeMap<String, usize>,
}

impl RunControl {
    pub fn check(&self, models: usize, tools: usize, model_step: bool) -> Result<(), StopReason> {
        let p = &self.policy;
        if p.turns_per_slice == 0
            || p.model_timeout_ms == 0
            || p.tool_timeout_ms == 0
            || p.model_retries > 3
            || p.retry_delay_ms > 10_000
            || p.max_repeated_observations < 2
        {
            return Err(StopReason::new(
                "invalid_policy",
                "本次复习的运行配置无效，请开始新的复习",
                false,
            ));
        }
        if self.charged_active_ms >= p.max_active_ms {
            return Err(StopReason::time_limit());
        }
        if model_step && models >= p.max_model_calls {
            return Err(StopReason::new(
                "model_call_limit",
                "已达到本次模型调用上限，请开始新的复习",
                false,
            ));
        }
        if !model_step && tools >= p.max_tool_calls {
            return Err(StopReason::new(
                "tool_call_limit",
                "已达到本次工具调用上限，请开始新的复习",
                false,
            ));
        }
        Ok(())
    }

    pub fn reserve_tokens(&mut self, amount: u64) -> Result<(), StopReason> {
        if self.charged_tokens.saturating_add(amount) > self.policy.max_token_charge {
            return Err(StopReason::new(
                "token_limit",
                "本次复习剩余用量预算不足，请开始新的复习",
                false,
            ));
        }
        self.charged_tokens = self.charged_tokens.saturating_add(amount);
        Ok(())
    }

    pub fn settle_tokens(&mut self, reserved: u64, usage: &Value) {
        // Unknown usage retains the reservation. Never interpret a missing value as zero.
        if let Some(actual) = usage["total_tokens"].as_u64() {
            self.charged_tokens = self
                .charged_tokens
                .saturating_sub(reserved)
                .saturating_add(actual);
        }
    }

    pub fn reserve_time(&mut self, maximum: u64) -> u64 {
        let allowance = maximum.min(
            self.policy
                .max_active_ms
                .saturating_sub(self.charged_active_ms),
        );
        self.charged_active_ms = self.charged_active_ms.saturating_add(allowance);
        allowance
    }

    pub fn settle_time(&mut self, reserved: u64, elapsed: Duration) {
        self.charged_active_ms = self
            .charged_active_ms
            .saturating_sub(reserved)
            .saturating_add(elapsed.as_millis().min(u64::MAX as u128) as u64);
    }

    pub fn observe(&mut self, name: &str, arguments: &str, output: &Value) -> Option<StopReason> {
        if !["search_textbook", "read_source", "get_learning_progress"].contains(&name) {
            self.last_observation = None;
            self.repeated_observations = 0;
            return None;
        }
        let args = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!(arguments));
        let digest = format!(
            "{:x}",
            Sha256::digest(json!([name, args, output]).to_string().as_bytes())
        );
        self.repeated_observations = if self.last_observation.as_ref() == Some(&digest) {
            self.repeated_observations.saturating_add(1)
        } else {
            1
        };
        self.last_observation = Some(digest);
        (self.repeated_observations >= self.policy.max_repeated_observations).then(|| {
            StopReason::new(
                "no_progress",
                "连续重复查询没有取得新进展，请调整复习目标后重新开始",
                false,
            )
        })
    }
}

pub fn can_resume(run: &ReviewRun) -> bool {
    ["ready", "paused", "failed"].contains(&run.state.as_str())
        && !matches!(run.control.stop.as_ref(), Some(s) if !s.resumable)
        && run.control.charged_active_ms < run.control.policy.max_active_ms
        && run.model_calls < run.control.policy.max_model_calls
        && run.tool_calls < run.control.policy.max_tool_calls
        && run.control.charged_tokens < run.control.policy.max_token_charge
}

pub fn provider_failure(error: &ProviderError) -> StopReason {
    let retryable = matches!(
        error,
        ProviderError::Timeout
            | ProviderError::Network
            | ProviderError::Unavailable
            | ProviderError::RateLimited
    );
    StopReason::new(error.code(), &error.to_string(), retryable)
}

/// Budget estimate, not provider usage: one unit per UTF-8 byte plus protocol overhead.
/// Model-specific tokenizers are deliberately not claimed here.
pub fn request_charge(body: &Value, output_tokens: u64) -> u64 {
    (body.to_string().len() as u64)
        .saturating_add(256)
        .saturating_add(output_tokens)
}

/// Poll cancellation without restarting the future. Dropping a model future ends our wait;
/// already accepted remote work or blocking native work cannot be rolled back by this layer.
pub async fn guarded<F: Future>(
    db: &Database,
    run_id: &str,
    timeout_ms: u64,
    active_limited: bool,
    future: F,
) -> Result<F::Output, StopReason> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut future = std::pin::pin!(future);
    loop {
        let current = db.review_load(run_id).map_err(|_| {
            StopReason::new(
                "checkpoint_unavailable",
                "无法读取复习检查点，请稍后继续",
                true,
            )
        })?;
        if current.cancel_requested {
            return Err(StopReason::cancelled());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(if active_limited {
                StopReason::time_limit()
            } else {
                StopReason::new("operation_timeout", "当前步骤超时，可稍后继续复习", true)
            });
        }
        if let Ok(value) =
            tokio::time::timeout(remaining.min(Duration::from_millis(100)), future.as_mut()).await
        {
            return Ok(value);
        }
    }
}
