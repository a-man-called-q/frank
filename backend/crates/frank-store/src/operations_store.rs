//! Store operations store persistence methods.

use super::*;

impl Store {
    pub async fn upsert_operation(&self, operation: &frank_protocol::OperationView) -> Result<()> {
        let value_json = serde_json::to_string(operation)?;
        let kind = serde_json::to_value(operation.kind)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let status = serde_json::to_value(operation.status)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        sqlx::query(
            "INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(operation.id.to_string())
        .bind(kind)
        .bind(status)
        .bind(&operation.resource)
        .bind(&operation.phase)
        .bind(i64::from(operation.attempt))
        .bind(&operation.error)
        .bind(value_json)
        .bind(&operation.created_at)
        .bind(&operation.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn operation(
        &self,
        operation_id: frank_protocol::OperationId,
    ) -> Result<Option<frank_protocol::OperationView>> {
        let row = sqlx::query("SELECT value_json FROM operations WHERE operation_id = ?")
            .bind(operation_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let value: String = row.try_get("value_json")?;
            Ok(serde_json::from_str(&value)?)
        })
        .transpose()
    }

    pub async fn operations(&self) -> Result<Vec<frank_protocol::OperationView>> {
        let rows = sqlx::query(
            "SELECT value_json FROM operations ORDER BY created_at ASC, operation_id ASC",
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

    pub async fn audit_backlog_count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE exported = 0")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.max(0) as u64)
    }

    pub async fn acquire_operation_lock(
        &self,
        resource: &str,
        operation_id: frank_protocol::OperationId,
    ) -> Result<bool> {
        if resource.trim().is_empty() || resource.len() > 4_096 {
            return Err(StoreError::Validation(
                "operation resource is invalid".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT OR IGNORE INTO operation_locks (resource, operation_id, acquired_at) VALUES (?, ?, ?)",
        )
        .bind(resource)
        .bind(operation_id.to_string())
        .bind(timestamp_now())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn release_operation_lock(
        &self,
        resource: &str,
        operation_id: frank_protocol::OperationId,
    ) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM operation_locks WHERE resource = ? AND operation_id = ?")
                .bind(resource)
                .bind(operation_id.to_string())
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn upsert_budget_clock(
        &self,
        scope_id: &str,
        started_at: u64,
        deadline: Option<u64>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO budget_clocks (scope_id, started_at, deadline) VALUES (?, ?, ?) ON CONFLICT(scope_id) DO UPDATE SET started_at = excluded.started_at, deadline = excluded.deadline",
        )
        .bind(scope_id)
        .bind(started_at as i64)
        .bind(deadline.map(|value| value as i64))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn budget_clock(&self, scope_id: &str) -> Result<Option<(u64, Option<u64>)>> {
        let row = sqlx::query("SELECT started_at, deadline FROM budget_clocks WHERE scope_id = ?")
            .bind(scope_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let started_at = row.try_get::<i64, _>("started_at")?.max(0) as u64;
            let deadline = row
                .try_get::<Option<i64>, _>("deadline")?
                .map(|value| value.max(0) as u64);
            Ok((started_at, deadline))
        })
        .transpose()
    }

    pub async fn prune_operations_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let operations = self.operations().await?;
        let candidates = operations
            .iter()
            .filter(|operation| {
                operation.updated_at < *cutoff
                    && matches!(
                        operation.status,
                        frank_protocol::OperationStatus::Succeeded
                            | frank_protocol::OperationStatus::Failed
                            | frank_protocol::OperationStatus::Cancelled
                    )
            })
            .map(|operation| operation.id)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(0);
        }
        let mut snapshot = self.snapshot().await?;
        let mut tx = self.pool.begin().await?;
        for operation_id in &candidates {
            sqlx::query("DELETE FROM operations WHERE operation_id = ?")
                .bind(operation_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        snapshot
            .operations
            .retain(|operation| !candidates.contains(&operation.id));
        sqlx::query("UPDATE server_state SET snapshot_json = ? WHERE id = 1")
            .bind(serde_json::to_string(&snapshot)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(candidates.len() as u64)
    }
}
