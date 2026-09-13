//! Store auth persistence methods.

use super::*;

impl Store {
    /// Return the single bootstrapped local owner, if setup has completed.
    /// Password hashes are only exposed to the server-side auth service and
    /// are never serialized into a protocol response.
    pub async fn owner(&self) -> Result<Option<StoredOwner>> {
        let row = sqlx::query(
            "SELECT owner_accounts.owner_id, username, password_hash, created_at, password_changed_at FROM owner_accounts INNER JOIN owner_account_slot ON owner_account_slot.owner_id = owner_accounts.owner_id WHERE owner_account_slot.slot = 1 LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let owner_id: String = row.try_get("owner_id")?;
            let parse_id = |value: &str| {
                frank_protocol::UserId::parse(value)
                    .map_err(|error| StoreError::Validation(error.to_string()))
            };
            let parse_timestamp = |value: String| value.parse::<u64>().unwrap_or_default();
            Ok(StoredOwner {
                owner_id: parse_id(&owner_id)?,
                username: row.try_get("username")?,
                password_hash: row.try_get("password_hash")?,
                created_at: parse_timestamp(row.try_get("created_at")?),
                password_changed_at: parse_timestamp(row.try_get("password_changed_at")?),
            })
        })
        .transpose()
    }

    pub async fn create_owner(
        &self,
        owner_id: frank_protocol::UserId,
        username: &str,
        password_hash: &str,
        created_at: u64,
    ) -> Result<bool> {
        if username.is_empty() || password_hash.is_empty() {
            return Err(StoreError::Validation(
                "owner username and password hash are required".into(),
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let slot =
            sqlx::query("INSERT OR IGNORE INTO owner_account_slot (slot, owner_id) VALUES (1, ?)")
                .bind(owner_id.to_string())
                .execute(&mut *transaction)
                .await?;
        if slot.rows_affected() != 1 {
            return Ok(false);
        }
        let result = sqlx::query(
            "INSERT INTO owner_accounts (owner_id, username, password_hash, created_at, password_changed_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(owner_id.to_string())
        .bind(username)
        .bind(password_hash)
        .bind(created_at.to_string())
        .bind(created_at.to_string())
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn update_owner_password(
        &self,
        owner_id: frank_protocol::UserId,
        password_hash: &str,
        changed_at: u64,
    ) -> Result<bool> {
        if password_hash.is_empty() {
            return Err(StoreError::Validation("password hash is empty".into()));
        }
        let result = sqlx::query(
            "UPDATE owner_accounts SET password_hash = ?, password_changed_at = ? WHERE owner_id = ?",
        )
        .bind(password_hash)
        .bind(changed_at.to_string())
        .bind(owner_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn insert_auth_session(&self, session: &StoredAuthSession) -> Result<()> {
        if session.token_hash.is_empty() {
            return Err(StoreError::Validation("session token hash is empty".into()));
        }
        sqlx::query(
            "INSERT INTO auth_sessions (session_id, owner_id, device_id, token_hash, created_at, expires_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.session_id.to_string())
        .bind(session.owner_id.to_string())
        .bind(session.device_id.to_string())
        .bind(&session.token_hash)
        .bind(session.created_at as i64)
        .bind(session.expires_at as i64)
        .bind(session.last_seen_at as i64)
        .bind(if session.revoked { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_auth_session_if_owner_hash(
        &self,
        owner_id: frank_protocol::UserId,
        password_hash: &str,
        session: &StoredAuthSession,
    ) -> Result<bool> {
        if password_hash.is_empty() || session.token_hash.is_empty() || session.owner_id != owner_id
        {
            return Err(StoreError::Validation(
                "owner password hash, session token hash, and owner id are required".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT INTO auth_sessions (session_id, owner_id, device_id, token_hash, created_at, expires_at, last_seen_at, revoked) SELECT ?, ?, ?, ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM owner_accounts WHERE owner_id = ? AND password_hash = ?)",
        )
        .bind(session.session_id.to_string())
        .bind(session.owner_id.to_string())
        .bind(session.device_id.to_string())
        .bind(&session.token_hash)
        .bind(session.created_at as i64)
        .bind(session.expires_at as i64)
        .bind(session.last_seen_at as i64)
        .bind(if session.revoked { 1_i64 } else { 0_i64 })
        .bind(owner_id.to_string())
        .bind(password_hash)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn auth_session(
        &self,
        token_hash: &str,
        now: u64,
    ) -> Result<Option<StoredAuthSession>> {
        let row = sqlx::query(
            "SELECT session_id, owner_id, device_id, token_hash, created_at, expires_at, last_seen_at, revoked FROM auth_sessions WHERE token_hash = ? AND revoked = 0 AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now as i64)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let parse_id = |column: &str| {
            let value: String = row.try_get(column)?;
            uuid::Uuid::parse_str(&value).map_err(|error| StoreError::Validation(error.to_string()))
        };
        let session_id = frank_protocol::SessionId::from(parse_id("session_id")?);
        let owner_id = frank_protocol::UserId::from(parse_id("owner_id")?);
        let device_id = frank_protocol::DeviceId::from(parse_id("device_id")?);
        let session = StoredAuthSession {
            session_id,
            owner_id,
            device_id,
            token_hash: row.try_get("token_hash")?,
            created_at: row.try_get::<i64, _>("created_at")?.max(0) as u64,
            expires_at: row.try_get::<i64, _>("expires_at")?.max(0) as u64,
            last_seen_at: row.try_get::<i64, _>("last_seen_at")?.max(0) as u64,
            revoked: row.try_get::<i64, _>("revoked")? != 0,
        };
        sqlx::query("UPDATE auth_sessions SET last_seen_at = ? WHERE session_id = ?")
            .bind(now as i64)
            .bind(session.session_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(Some(StoredAuthSession {
            last_seen_at: now,
            ..session
        }))
    }

    pub async fn revoke_auth_session(&self, session_id: frank_protocol::SessionId) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE auth_sessions SET revoked = 1 WHERE session_id = ? AND revoked = 0",
        )
        .bind(session_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn revoke_auth_sessions(&self, owner_id: frank_protocol::UserId) -> Result<u64> {
        let result =
            sqlx::query("UPDATE auth_sessions SET revoked = 1 WHERE owner_id = ? AND revoked = 0")
                .bind(owner_id.to_string())
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    pub async fn prune_auth_sessions(&self, now: u64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= ? OR revoked = 1")
            .bind(now as i64)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn upsert_agent_capability(&self, capability: &StoredAgentCapability) -> Result<()> {
        if capability.capability_hash.trim().is_empty() {
            return Err(StoreError::Validation(
                "agent capability hash is empty".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO agent_capabilities (capability_hash, agent_id, task_id, issued_at, expires_at, revoked) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(capability_hash) DO UPDATE SET agent_id = excluded.agent_id, task_id = excluded.task_id, issued_at = excluded.issued_at, expires_at = excluded.expires_at, revoked = excluded.revoked",
        )
        .bind(&capability.capability_hash)
        .bind(capability.agent_id.to_string())
        .bind(capability.task_id.to_string())
        .bind(capability.issued_at as i64)
        .bind(capability.expires_at as i64)
        .bind(if capability.revoked { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke_agent_capability(&self, capability_hash: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE agent_capabilities SET revoked = 1 WHERE capability_hash = ? AND revoked = 0",
        )
        .bind(capability_hash)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn prune_agent_capabilities(&self, now: u64) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM agent_capabilities WHERE expires_at < ? OR revoked = 1")
                .bind(now as i64)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }
}
