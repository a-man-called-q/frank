//! Store pairing persistence methods.

use super::*;

impl Store {
    pub async fn certificate_fingerprint(&self) -> Result<String> {
        let row = sqlx::query("SELECT certificate_fingerprint FROM server_identity WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("certificate_fingerprint")?)
    }

    pub async fn set_certificate_fingerprint(&self, fingerprint: &str) -> Result<()> {
        sqlx::query("UPDATE server_identity SET certificate_fingerprint = ? WHERE id = 1")
            .bind(fingerprint)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn devices(&self) -> Result<Vec<StoredDevice>> {
        let rows = sqlx::query(
            "SELECT device_id, name, role, token_hash, certificate_fingerprint, revoked, last_seen_at FROM devices",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut devices = Vec::with_capacity(rows.len());
        for row in rows {
            let device_id: String = row.try_get("device_id")?;
            let role: String = row.try_get("role")?;
            let role = match role.as_str() {
                "owner" => DeviceRole::Owner,
                "operator" => DeviceRole::Operator,
                "observer" => DeviceRole::Observer,
                _ => {
                    return Err(StoreError::Validation(
                        "stored device role is invalid".into(),
                    ));
                }
            };
            let revoked = row.try_get::<i64, _>("revoked")? != 0;
            let last_seen_at = row
                .try_get::<String, _>("last_seen_at")?
                .parse::<u64>()
                .unwrap_or_default();
            devices.push(StoredDevice {
                device_id: DeviceId::parse(&device_id)
                    .map_err(|error| StoreError::Validation(error.to_string()))?,
                name: row.try_get("name")?,
                role,
                token_hash: row.try_get("token_hash")?,
                certificate_fingerprint: row.try_get("certificate_fingerprint")?,
                revoked,
                last_seen_at,
            });
        }
        Ok(devices)
    }

    pub async fn upsert_device(&self, device: &StoredDevice) -> Result<()> {
        sqlx::query(
            "INSERT INTO devices (device_id, name, role, token_hash, certificate_fingerprint, created_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, role = excluded.role, token_hash = excluded.token_hash, certificate_fingerprint = excluded.certificate_fingerprint, last_seen_at = excluded.last_seen_at, revoked = excluded.revoked",
        )
        .bind(device.device_id.to_string())
        .bind(&device.name)
        .bind(match device.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&device.token_hash)
        .bind(&device.certificate_fingerprint)
        .bind(timestamp_now())
        .bind(device.last_seen_at.to_string())
        .bind(if device.revoked { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke_device(&self, device_id: DeviceId) -> Result<bool> {
        let result = sqlx::query("UPDATE devices SET revoked = 1 WHERE device_id = ?")
            .bind(device_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_pairing_ticket(&self, ticket: &StoredPairingTicket) -> Result<()> {
        sqlx::query(
            "INSERT INTO pairing_tickets (ticket_id, secret_hash, role, certificate_fingerprint, expires_at, used) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(ticket_id) DO UPDATE SET secret_hash = excluded.secret_hash, role = excluded.role, certificate_fingerprint = excluded.certificate_fingerprint, expires_at = excluded.expires_at, used = excluded.used",
        )
        .bind(&ticket.ticket_id)
        .bind(&ticket.secret_hash)
        .bind(match ticket.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&ticket.certificate_fingerprint)
        .bind(ticket.expires_at as i64)
        .bind(if ticket.used { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pairing_tickets(&self) -> Result<Vec<StoredPairingTicket>> {
        let rows = sqlx::query("SELECT ticket_id, secret_hash, role, certificate_fingerprint, expires_at, used FROM pairing_tickets")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let role: String = row.try_get("role")?;
                let role = match role.as_str() {
                    "owner" => DeviceRole::Owner,
                    "operator" => DeviceRole::Operator,
                    "observer" => DeviceRole::Observer,
                    _ => {
                        return Err(StoreError::Validation(
                            "stored pairing role is invalid".into(),
                        ));
                    }
                };
                Ok(StoredPairingTicket {
                    ticket_id: row.try_get("ticket_id")?,
                    secret_hash: row.try_get("secret_hash")?,
                    role,
                    certificate_fingerprint: row.try_get("certificate_fingerprint")?,
                    expires_at: row.try_get::<i64, _>("expires_at")?.max(0) as u64,
                    used: row.try_get::<i64, _>("used")? != 0,
                })
            })
            .collect()
    }

    pub async fn mark_pairing_ticket_used(&self, ticket_id: &str) -> Result<bool> {
        let result =
            sqlx::query("UPDATE pairing_tickets SET used = 1 WHERE ticket_id = ? AND used = 0")
                .bind(ticket_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn complete_pairing(&self, ticket_id: &str, device: &StoredDevice) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO devices (device_id, name, role, token_hash, certificate_fingerprint, created_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, role = excluded.role, token_hash = excluded.token_hash, certificate_fingerprint = excluded.certificate_fingerprint, last_seen_at = excluded.last_seen_at, revoked = excluded.revoked",
        )
        .bind(device.device_id.to_string())
        .bind(&device.name)
        .bind(match device.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&device.token_hash)
        .bind(&device.certificate_fingerprint)
        .bind(timestamp_now())
        .bind(device.last_seen_at.to_string())
        .bind(if device.revoked { 1_i64 } else { 0_i64 })
        .execute(&mut *tx)
        .await?;
        let consumed =
            sqlx::query("UPDATE pairing_tickets SET used = 1 WHERE ticket_id = ? AND used = 0")
                .bind(ticket_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
        if !consumed {
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }
}
