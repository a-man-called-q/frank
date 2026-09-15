//! Durable runner, toolchain-installation, and check-run projections.

use frank_protocol::{
    CheckRunView, RunnerId, RunnerJobStatus, RunnerJobView, RunnerView, TaskId,
    ToolchainRequirementStatus, timestamp_now,
};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::{Result, Store};

impl Store {
    pub async fn runner_views(&self) -> Result<Vec<RunnerView>> {
        let rows =
            sqlx::query("SELECT value_json FROM runners ORDER BY updated_at DESC, runner_id")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|row| {
                let value: String = row.try_get("value_json")?;
                Ok(serde_json::from_str(&value)?)
            })
            .collect()
    }

    pub async fn upsert_runner_view(&self, runner: &RunnerView) -> Result<()> {
        sqlx::query("INSERT INTO runners (runner_id, value_json, status, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(runner_id) DO UPDATE SET value_json = excluded.value_json, status = excluded.status, updated_at = excluded.updated_at")
            .bind(runner.id.to_string())
            .bind(serde_json::to_string(runner)?)
            .bind(serde_json::to_value(&runner.status)?.as_str().unwrap_or("unknown"))
            .bind(runner.last_seen_at.clone().unwrap_or_else(timestamp_now))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn runner_view(&self, runner_id: RunnerId) -> Result<Option<RunnerView>> {
        let row = sqlx::query("SELECT value_json FROM runners WHERE runner_id = ?")
            .bind(runner_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let value: String = row.try_get("value_json")?;
            Ok(serde_json::from_str(&value)?)
        })
        .transpose()
    }

    /// Store only the digest of the durable runner credential. The raw
    /// credential is returned once in the WebSocket Welcome frame and is
    /// never recoverable from SQLite.
    pub async fn upsert_runner_credential_hash(
        &self,
        runner_id: RunnerId,
        token_hash: &str,
    ) -> Result<()> {
        if token_hash.trim().is_empty() {
            return Err(crate::StoreError::Validation(
                "runner credential hash is empty".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO runner_credentials (runner_id, token_hash, created_at, revoked) VALUES (?, ?, ?, 0) ON CONFLICT(runner_id) DO UPDATE SET token_hash = excluded.token_hash, created_at = excluded.created_at, revoked = 0",
        )
        .bind(runner_id.to_string())
        .bind(token_hash)
        .bind(timestamp_now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn runner_credential_matches(
        &self,
        runner_id: RunnerId,
        token_hash: &str,
    ) -> Result<bool> {
        let matched = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM runner_credentials WHERE runner_id = ? AND token_hash = ? AND revoked = 0)",
        )
        .bind(runner_id.to_string())
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(matched != 0)
    }

    pub async fn revoke_runner_credential(&self, runner_id: RunnerId) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE runner_credentials SET revoked = 1 WHERE runner_id = ? AND revoked = 0",
        )
        .bind(runner_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn upsert_runner_job(&self, job: &RunnerJobView) -> Result<()> {
        sqlx::query(
            "INSERT INTO runner_jobs (job_id, runner_id, project_id, task_id, check_id, value_json, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(job_id) DO UPDATE SET value_json = excluded.value_json, status = excluded.status, updated_at = excluded.updated_at",
        )
        .bind(&job.id)
        .bind(job.runner_id.to_string())
        .bind(job.project_id.to_string())
        .bind(job.task_id.map(|id: TaskId| id.to_string()))
        .bind(&job.check_id)
        .bind(serde_json::to_string(job)?)
        .bind(serde_json::to_value(job.status)?.as_str().unwrap_or("unknown"))
        .bind(&job.created_at)
        .bind(&job.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn runner_job_views(
        &self,
        runner_id: Option<RunnerId>,
    ) -> Result<Vec<RunnerJobView>> {
        let rows = if let Some(runner_id) = runner_id {
            sqlx::query(
                "SELECT value_json FROM runner_jobs WHERE runner_id = ? ORDER BY updated_at DESC",
            )
            .bind(runner_id.to_string())
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query("SELECT value_json FROM runner_jobs ORDER BY updated_at DESC")
                .fetch_all(&self.pool)
                .await?
        };
        rows.into_iter()
            .map(|row| {
                let value: String = row.try_get("value_json")?;
                Ok(serde_json::from_str(&value)?)
            })
            .collect()
    }

    pub async fn mark_runner_jobs_interrupted(&self) -> Result<u64> {
        mark_runner_jobs_interrupted_pool(&self.pool).await
    }

    pub async fn check_run_views(&self, project_id: Option<&str>) -> Result<Vec<CheckRunView>> {
        let rows = if let Some(project_id) = project_id {
            sqlx::query(
                "SELECT value_json FROM check_runs WHERE project_id = ? ORDER BY started_at DESC",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query("SELECT value_json FROM check_runs ORDER BY started_at DESC")
                .fetch_all(&self.pool)
                .await?
        };
        rows.into_iter()
            .map(|row| {
                let value: String = row.try_get("value_json")?;
                Ok(serde_json::from_str(&value)?)
            })
            .collect()
    }

    pub async fn toolchain_installation_views(&self) -> Result<Vec<Value>> {
        let rows = sqlx::query(
            "SELECT value_json FROM toolchain_installations ORDER BY updated_at DESC, manifest_id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let value: String = row.try_get("value_json")?;
                Ok(serde_json::from_str(&value)?)
            })
            .collect()
    }

    pub async fn toolchain_installation_status(
        &self,
        manifest_id: &str,
        version: &str,
    ) -> Result<Option<(ToolchainRequirementStatus, Option<String>)>> {
        let row = sqlx::query(
            "SELECT status, value_json FROM toolchain_installations WHERE manifest_id = ? AND version = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(manifest_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let status: String = row.try_get("status")?;
        let status = serde_json::from_value(Value::String(status))
            .unwrap_or(ToolchainRequirementStatus::Failed);
        let value: Value = serde_json::from_str(&row.try_get::<String, _>("value_json")?)?;
        let path = value
            .get("install_path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        Ok(Some((status, path)))
    }
}

/// Mark in-flight jobs as interrupted during startup while keeping the
/// indexed columns and the serialized projection in lockstep. The latter is
/// what remote clients deserialize, so updating only the SQL status column
/// would leave a stale queued/running view after a restart.
pub(crate) async fn mark_runner_jobs_interrupted_pool(pool: &SqlitePool) -> Result<u64> {
    let rows = sqlx::query(
        "SELECT job_id, value_json FROM runner_jobs WHERE status IN ('queued', 'running')",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let failed = serde_json::to_value(RunnerJobStatus::Failed)?
        .as_str()
        .unwrap_or("failed")
        .to_string();
    let mut tx = pool.begin().await?;
    let mut updated = 0_u64;
    for row in rows {
        let job_id: String = row.try_get("job_id")?;
        let value_json: String = row.try_get("value_json")?;
        let mut view: RunnerJobView = serde_json::from_str(&value_json)?;
        view.status = RunnerJobStatus::Failed;
        view.updated_at = timestamp_now();
        sqlx::query(
            "UPDATE runner_jobs SET value_json = ?, status = ?, updated_at = ? WHERE job_id = ?",
        )
        .bind(serde_json::to_string(&view)?)
        .bind(&failed)
        .bind(&view.updated_at)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        updated = updated.saturating_add(1);
    }
    tx.commit().await?;
    Ok(updated)
}
