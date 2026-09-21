use super::types::*;
use crate::db::{Database, DbError};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Database {
    pub(crate) fn search_options(&self) -> Result<SearchOptions, DbError> {
        let value: Option<String> = self
            .connect()?
            .query_row("SELECT record FROM search_options WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(DbError::from))
            .unwrap_or_else(|| Ok(SearchOptions::default()))
    }
    pub(crate) fn save_search_options(&self, options: &SearchOptions) -> Result<(), DbError> {
        options
            .validate()
            .map_err(|e| DbError::Validation(e.to_string()))?;
        self.connect()?.execute("INSERT INTO search_options VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET record=excluded.record", [serde_json::to_string(options)?])?;
        Ok(())
    }
    pub(crate) fn search_attempts_today(&self) -> Result<u32, DbError> {
        Ok(self.connect()?.query_row(
            "SELECT COUNT(*) FROM search_attempts WHERE local_day=?1",
            [chrono::Local::now().date_naive().to_string()],
            |row| row.get(0),
        )?)
    }
    pub(crate) fn reserve_search_attempt(
        &self,
        run_id: &str,
        limit: u32,
    ) -> Result<(), SearchError> {
        let mut conn = self.connect().map_err(|_| SearchError::Storage)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SearchError::Storage)?;
        let day = chrono::Local::now().date_naive().to_string();
        let total: u32 = tx
            .query_row(
                "SELECT COUNT(*) FROM search_attempts WHERE local_day=?1",
                [&day],
                |r| r.get(0),
            )
            .map_err(|_| SearchError::Storage)?;
        let attempts: u32 = tx
            .query_row(
                "SELECT COUNT(*) FROM search_attempts WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(|_| SearchError::Storage)?;
        if total >= limit || attempts >= 2 {
            return Err(SearchError::Budget);
        }
        tx.execute(
            "INSERT INTO search_attempts(run_id,local_day,created_at) VALUES(?1,?2,?3)",
            params![run_id, day, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|_| SearchError::Storage)?;
        tx.commit().map_err(|_| SearchError::Storage)
    }
    pub(crate) fn search_start(
        &self,
        id: &str,
        card_id: &str,
        options: &SearchOptions,
    ) -> Result<(), DbError> {
        self.connect()?.execute("INSERT INTO search_runs(id,card_id,state,created_at,options_json) VALUES(?1,?2,'planning',?3,?4)",
            params![id,card_id,chrono::Utc::now().to_rfc3339(),serde_json::to_string(options)?])?;
        Ok(())
    }
    pub(crate) fn search_run_state(
        &self,
        id: &str,
        card_id: &str,
    ) -> Result<Option<String>, DbError> {
        Ok(self
            .connect()?
            .query_row(
                "SELECT state FROM search_runs WHERE id=?1 AND card_id=?2",
                params![id, card_id],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub(crate) fn search_set_state(
        &self,
        id: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<bool, DbError> {
        Ok(self.connect()?.execute("UPDATE search_runs SET state=?1,error_code=?2 WHERE id=?3 AND state IN ('planning','searching','answering','validating')", params![state,error,id])? == 1)
    }
    pub(crate) fn search_snapshot(
        &self,
        id: &str,
        evidence: &[WebEvidence],
    ) -> Result<(), DbError> {
        self.connect()?.execute("UPDATE search_runs SET evidence_json=?1 WHERE id=?2 AND state IN ('searching','answering')", params![serde_json::to_string(evidence)?,id])?;
        Ok(())
    }
    pub(crate) fn search_cache_get(&self, id: &str) -> Result<Option<Vec<WebEvidence>>, DbError> {
        let record: Option<String> = self
            .connect()?
            .query_row(
                "SELECT evidence_json FROM search_cache WHERE id=?1 AND expires_at>?2",
                params![id, chrono::Utc::now().timestamp()],
                |r| r.get(0),
            )
            .optional()?;
        record
            .map(|v| serde_json::from_str(&v).map_err(DbError::from))
            .transpose()
    }
    pub(crate) fn search_cache_put(
        &self,
        id: &str,
        evidence: &[WebEvidence],
    ) -> Result<(), DbError> {
        let conn = self.connect()?;
        conn.execute("INSERT INTO search_cache VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET expires_at=excluded.expires_at,evidence_json=excluded.evidence_json",
            params![id,chrono::Utc::now().timestamp()+300,serde_json::to_string(evidence)?])?;
        conn.execute("DELETE FROM search_cache WHERE id NOT IN (SELECT id FROM search_cache ORDER BY expires_at DESC LIMIT 50)", [])?;
        Ok(())
    }
    pub(crate) fn search_recover(&self) -> Result<(), DbError> {
        self.connect()?.execute("UPDATE search_runs SET state='interrupted' WHERE state IN ('planning','searching','answering','validating')", [])?;
        Ok(())
    }
}
