use super::types::SearchError;
use crate::db::Database;
use std::{
    future::Future,
    time::{Duration, Instant},
};

pub async fn guarded<F: Future>(
    db: &Database,
    id: &str,
    card: &str,
    deadline: Instant,
    future: F,
) -> Result<F::Output, SearchError> {
    let mut future = std::pin::pin!(future);
    loop {
        let state = db
            .search_run_state(id, card)
            .map_err(|_| SearchError::Storage)?;
        if !state
            .as_deref()
            .is_some_and(|s| ["planning", "searching", "answering", "validating"].contains(&s))
        {
            return Err(SearchError::Cancelled);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(SearchError::Timeout);
        }
        if let Ok(value) =
            tokio::time::timeout(left.min(Duration::from_millis(100)), future.as_mut()).await
        {
            return Ok(value);
        }
    }
}
