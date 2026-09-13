//! Store snapshot store persistence methods.

use super::*;

impl Store {
    pub async fn current_revision(&self) -> Result<u64> {
        let row = sqlx::query("SELECT revision FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let revision: i64 = row.try_get("revision")?;
        Ok(revision.max(0) as u64)
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let row = sqlx::query("SELECT snapshot_json FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let json: String = row.try_get("snapshot_json")?;
        serde_json::from_str(&json).map_err(|_| StoreError::CorruptSnapshot)
    }

    pub async fn idempotent_response(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommandResponse>> {
        let row =
            sqlx::query("SELECT response_json FROM idempotency_commands WHERE command_id = ?")
                .bind(command_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| {
            let json: String = row.try_get("response_json")?;
            Ok(serde_json::from_str(&json)?)
        })
        .transpose()
    }

    pub async fn replace_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        sqlx::query(
            "UPDATE server_state SET snapshot_json = ?, revision = ?, event_seq = ? WHERE id = 1",
        )
        .bind(serde_json::to_string(snapshot)?)
        .bind(snapshot.revision as i64)
        .bind(snapshot.event_seq as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
