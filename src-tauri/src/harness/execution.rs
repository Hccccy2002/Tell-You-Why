//! A scoped cancellation signal with explicit worker cleanup acknowledgement.
use super::tools::{ErrorCode, ToolError};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct State {
    cancelled: AtomicBool,
    workers: AtomicUsize,
    cleanup_failed: AtomicBool,
    deadline: Instant,
}
#[derive(Clone)]
pub struct ExecutionControl(Arc<State>);
pub struct ExecutionScope {
    pub control: ExecutionControl,
}
pub struct WorkerGuard(ExecutionControl);

impl ExecutionScope {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            control: ExecutionControl(Arc::new(State {
                cancelled: AtomicBool::new(false),
                workers: AtomicUsize::new(0),
                cleanup_failed: AtomicBool::new(false),
                deadline: Instant::now() + Duration::from_millis(timeout_ms),
            })),
        }
    }
    pub async fn finish(&self) -> Result<(), ToolError> {
        self.control.cancel();
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.control.0.workers.load(Ordering::SeqCst) > 0 {
            if Instant::now() >= deadline {
                return Err(ToolError::new(
                    ErrorCode::CleanupFailed,
                    "无法确认资料进程已结束，已停止任务",
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if self.control.0.cleanup_failed.load(Ordering::SeqCst) {
            return Err(ToolError::new(
                ErrorCode::CleanupFailed,
                "资料进程清理未完成，已停止任务",
            ));
        }
        Ok(())
    }
}
impl Drop for ExecutionScope {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
impl ExecutionControl {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
    }
    pub fn check(&self) -> Result<(), ToolError> {
        if self.0.cancelled.load(Ordering::SeqCst) {
            return Err(ToolError::new(ErrorCode::Cancelled, "资料操作已取消"));
        }
        if Instant::now() >= self.0.deadline {
            return Err(ToolError::new(ErrorCode::Timeout, "资料操作超时"));
        }
        Ok(())
    }
    pub fn worker(&self) -> WorkerGuard {
        self.0.workers.fetch_add(1, Ordering::SeqCst);
        WorkerGuard(self.clone())
    }
    pub fn cleanup_failed(&self) {
        self.0.cleanup_failed.store(true, Ordering::SeqCst);
    }
}
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0 .0.workers.fetch_sub(1, Ordering::SeqCst);
    }
}
