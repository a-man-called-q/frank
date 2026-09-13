//! Store projection store persistence methods.

use super::*;

impl Store {
    pub async fn upsert_projection(
        &self,
        table: ProjectionTable,
        id: &str,
        value: &Value,
    ) -> Result<()> {
        let json = serde_json::to_string(value)?;
        match table {
            ProjectionTable::Project => {
                sqlx::query("INSERT INTO projects (project_id, value_json) VALUES (?, ?) ON CONFLICT(project_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Agent => {
                sqlx::query("INSERT INTO agent_profiles (agent_id, value_json) VALUES (?, ?) ON CONFLICT(agent_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Role => {
                let archived = value
                    .get("archived")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                sqlx::query("INSERT INTO roles (role_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(role_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                    .bind(id)
                    .bind(json)
                    .bind(if archived { 1_i64 } else { 0_i64 })
                    .execute(&self.pool)
                    .await?;
            }
            ProjectionTable::Mission => {
                sqlx::query("INSERT INTO missions (mission_id, value_json) VALUES (?, ?) ON CONFLICT(mission_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Task => {
                let mission_id =
                    value
                        .get("mission_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            StoreError::Validation("task projection is missing mission_id".into())
                        })?;
                sqlx::query("INSERT INTO tasks (task_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(task_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(id).bind(mission_id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Taskboard => {
                let archived = value
                    .get("archived")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                sqlx::query("INSERT INTO taskboards (taskboard_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(taskboard_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                    .bind(id)
                    .bind(json)
                    .bind(if archived { 1_i64 } else { 0_i64 })
                    .execute(&self.pool)
                    .await?;
            }
            ProjectionTable::WorkOffer => {
                let task_id = value
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("work offer projection is missing task_id".into())
                    })?;
                let agent_id = value
                    .get("agent_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("work offer projection is missing agent_id".into())
                    })?;
                let status = value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let created_at = value
                    .get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let expires_at = value
                    .get("expires_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                sqlx::query("INSERT INTO work_offers (offer_id, task_id, agent_id, status, value_json, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(offer_id) DO UPDATE SET task_id = excluded.task_id, agent_id = excluded.agent_id, status = excluded.status, value_json = excluded.value_json, expires_at = excluded.expires_at")
                    .bind(id)
                    .bind(task_id)
                    .bind(agent_id)
                    .bind(status)
                    .bind(json)
                    .bind(created_at)
                    .bind(expires_at)
                    .execute(&self.pool)
                    .await?;
            }
            ProjectionTable::HumanInput => {
                let task_id = value
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("human input projection is missing task_id".into())
                    })?;
                let status = value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let created_at = value
                    .get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let updated_at = value
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                sqlx::query("INSERT INTO human_input_requests (human_input_id, task_id, status, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(human_input_id) DO UPDATE SET task_id = excluded.task_id, status = excluded.status, value_json = excluded.value_json, updated_at = excluded.updated_at")
                    .bind(id)
                    .bind(task_id)
                    .bind(status)
                    .bind(json)
                    .bind(created_at)
                    .bind(updated_at)
                    .execute(&self.pool)
                    .await?;
            }
            ProjectionTable::OrganizationRelocation => {
                let from_board_id = value
                    .get("from_board_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation(
                            "relocation projection is missing from_board_id".into(),
                        )
                    })?;
                let to_board_id = value
                    .get("to_board_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation(
                            "relocation projection is missing to_board_id".into(),
                        )
                    })?;
                let from_revision = value
                    .get("from_revision")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let to_revision = value
                    .get("to_revision")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let created_at = value
                    .get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                sqlx::query("INSERT INTO organization_relocations (relocation_id, from_board_id, to_board_id, from_revision, to_revision, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(relocation_id) DO UPDATE SET from_board_id = excluded.from_board_id, to_board_id = excluded.to_board_id, from_revision = excluded.from_revision, to_revision = excluded.to_revision, value_json = excluded.value_json, created_at = excluded.created_at")
                    .bind(id)
                    .bind(from_board_id)
                    .bind(to_board_id)
                    .bind(from_revision as i64)
                    .bind(to_revision as i64)
                    .bind(json)
                    .bind(created_at)
                    .execute(&self.pool)
                    .await?;
            }
            ProjectionTable::Message => {
                let mission_id =
                    value
                        .get("mission_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            StoreError::Validation(
                                "message projection is missing mission_id".into(),
                            )
                        })?;
                sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(id).bind(mission_id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Approval => {
                let task_id = value
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("approval projection is missing task_id".into())
                    })?;
                sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                    .bind(id).bind(task_id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::TaskFeed => {
                let task_id = value
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("task feed projection is missing task_id".into())
                    })?;
                let kind = value
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let actor = value
                    .get("actor")
                    .map(serde_json::to_string)
                    .transpose()?
                    .unwrap_or_else(|| "{}".to_string());
                let created_at = value
                    .get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                sqlx::query("INSERT INTO task_feed (feed_id, task_id, kind, actor_json, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(feed_id) DO UPDATE SET task_id = excluded.task_id, kind = excluded.kind, actor_json = excluded.actor_json, value_json = excluded.value_json, created_at = excluded.created_at")
                    .bind(id).bind(task_id).bind(kind).bind(actor).bind(json).bind(created_at)
                    .execute(&self.pool).await?;
            }
        }
        Ok(())
    }
}
