//! Snapshot-to-table projection.
//!
//! The event log is the source of truth; these tables are a derived read model
//! rebuilt from it. Everything here runs inside the caller's transaction so a
//! projection can never be half-applied against a committed event.

use frank_protocol::*;
use sqlx::{Sqlite, Transaction};

use serde_json::Value;

use crate::{Result, StoreError, unix_seconds};

pub(crate) async fn apply_projection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &frank_protocol::Event,
    snapshot: &Snapshot,
) -> Result<()> {
    use frank_protocol::Event;

    async fn upsert(
        tx: &mut Transaction<'_, Sqlite>,
        table: &str,
        id_column: &str,
        id: &str,
        value: Value,
    ) -> Result<()> {
        // Table and column names come only from the fixed match arms below;
        // values are always bound parameters.
        let sql = format!(
            "INSERT INTO {table} ({id_column}, value_json) VALUES (?, ?) ON CONFLICT({id_column}) DO UPDATE SET value_json = excluded.value_json"
        );
        sqlx::query(&sql)
            .bind(id)
            .bind(serde_json::to_string(&value)?)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    match event {
        Event::SettingsChanged { settings } => {
            sqlx::query("INSERT INTO settings (key, value_json, revision) VALUES ('server', ?, ?) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, revision = excluded.revision")
                .bind(serde_json::to_string(settings)?)
                .bind(snapshot.revision as i64)
                .execute(&mut **tx)
                .await?;
        }
        Event::ProjectUpserted { project } => {
            upsert(
                tx,
                "projects",
                "project_id",
                &project.id.to_string(),
                serde_json::to_value(project)?,
            )
            .await?;
        }
        Event::ProjectArchived { project_id } => {
            if let Some(project) = snapshot
                .projects
                .iter()
                .find(|project| project.id == *project_id)
            {
                upsert(
                    tx,
                    "projects",
                    "project_id",
                    &project.id.to_string(),
                    serde_json::to_value(project)?,
                )
                .await?;
            }
        }
        Event::AgentUpserted { agent } => {
            upsert(
                tx,
                "agent_profiles",
                "agent_id",
                &agent.id.to_string(),
                serde_json::to_value(agent)?,
            )
            .await?;
            upsert_provider_session_projection(tx, agent, snapshot).await?;
        }
        Event::AgentArchived { agent_id } | Event::AgentStatusChanged { agent_id, .. } => {
            if let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == *agent_id) {
                upsert(
                    tx,
                    "agent_profiles",
                    "agent_id",
                    &agent.id.to_string(),
                    serde_json::to_value(agent)?,
                )
                .await?;
                upsert_provider_session_projection(tx, agent, snapshot).await?;
            }
        }
        Event::RoleUpserted { role } => {
            sqlx::query("INSERT INTO roles (role_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(role_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                .bind(role.id.to_string())
                .bind(serde_json::to_string(role)?)
                .bind(if role.archived { 1_i64 } else { 0_i64 })
                .execute(&mut **tx)
                .await?;
        }
        Event::RoleArchived { role_id } => {
            if let Some(role) = snapshot.roles.iter().find(|role| role.id == *role_id) {
                sqlx::query("INSERT INTO roles (role_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(role_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                    .bind(role.id.to_string())
                    .bind(serde_json::to_string(role)?)
                    .bind(if role.archived { 1_i64 } else { 0_i64 })
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::MissionCreated { mission } => {
            upsert(
                tx,
                "missions",
                "mission_id",
                &mission.id.to_string(),
                serde_json::to_value(mission)?,
            )
            .await?;
        }
        Event::MissionStatusChanged { mission_id, .. }
        | Event::MissionSupervisorSessionChanged { mission_id, .. }
        | Event::MissionBlocked { mission_id, .. } => {
            if let Some(mission) = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == *mission_id)
            {
                upsert(
                    tx,
                    "missions",
                    "mission_id",
                    &mission.id.to_string(),
                    serde_json::to_value(mission)?,
                )
                .await?;
            }
        }
        Event::TaskboardUpserted { taskboard } => {
            sqlx::query("INSERT INTO taskboards (taskboard_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(taskboard_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                .bind(taskboard.id.to_string())
                .bind(serde_json::to_string(taskboard)?)
                .bind(if taskboard.archived { 1_i64 } else { 0_i64 })
                .execute(&mut **tx)
                .await?;
        }
        Event::TaskboardArchived { taskboard_id } => {
            if let Some(taskboard) = snapshot
                .taskboards
                .iter()
                .find(|board| board.id == *taskboard_id)
            {
                sqlx::query("INSERT INTO taskboards (taskboard_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(taskboard_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                    .bind(taskboard.id.to_string())
                    .bind(serde_json::to_string(taskboard)?)
                    .bind(if taskboard.archived { 1_i64 } else { 0_i64 })
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::OrganizationDraftSaved { .. } | Event::OrganizationPublished { .. } => {
            sqlx::query(
                "INSERT INTO organization_state (id, value_json) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json",
            )
            .bind(serde_json::to_string(&snapshot.organization)?)
            .execute(&mut **tx)
            .await?;
        }
        Event::ConnectorProfileUpserted { profile } => {
            sqlx::query("INSERT INTO connector_profiles (profile_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(profile_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                .bind(profile.id.to_string())
                .bind(serde_json::to_string(profile)?)
                .bind(if profile.archived { 1_i64 } else { 0_i64 })
                .execute(&mut **tx)
                .await?;
        }
        Event::ConnectorProfileArchived { profile_id } => {
            if let Some(profile) = snapshot
                .organization
                .connector_profiles
                .iter()
                .find(|profile| profile.id == *profile_id)
            {
                sqlx::query("INSERT INTO connector_profiles (profile_id, value_json, archived) VALUES (?, ?, ?) ON CONFLICT(profile_id) DO UPDATE SET value_json = excluded.value_json, archived = excluded.archived")
                    .bind(profile.id.to_string())
                    .bind(serde_json::to_string(profile)?)
                    .bind(if profile.archived { 1_i64 } else { 0_i64 })
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TaskCreated { task } | Event::TaskUpdated { task } => {
            upsert_task_projection(tx, task).await?;
            upsert_dependent_task_projections(tx, snapshot, task.id).await?;
        }
        Event::WorkItemDropped { task, .. } => {
            upsert_task_projection(tx, task).await?;
            upsert_dependent_task_projections(tx, snapshot, task.id).await?;
        }
        Event::ChildWorkItemsSpawned { parent, children } => {
            upsert_task_projection(tx, parent).await?;
            for child in children {
                upsert_task_projection(tx, child).await?;
            }
        }
        Event::TaskStatusChanged { task_id, .. } => {
            if let Some(task) = snapshot.tasks.iter().find(|task| task.id == *task_id) {
                upsert_task_projection(tx, task).await?;
            }
            upsert_dependent_task_projections(tx, snapshot, *task_id).await?;
            // Completing the final child also unlocks its parent in the
            // reducer. The parent is a structural link, not a normal DAG
            // dependency, so it needs an explicit projection refresh here.
            for parent in snapshot
                .tasks
                .iter()
                .filter(|task| task.child_task_ids.contains(task_id))
            {
                upsert_task_projection(tx, parent).await?;
            }
            for grant in snapshot
                .task_grants
                .iter()
                .filter(|grant| grant.task_id == *task_id)
            {
                sqlx::query(
                    "UPDATE task_grants SET revoked = ?, value_json = ? WHERE grant_id = ?",
                )
                .bind(if grant.revoked { 1_i64 } else { 0_i64 })
                .bind(serde_json::to_string(grant)?)
                .bind(&grant.id)
                .execute(&mut **tx)
                .await?;
            }
        }
        Event::TaskAssigned { task_id, .. }
        | Event::TaskClaimed { task_id, .. }
        | Event::TaskReleased { task_id } => {
            if let Some(task) = snapshot.tasks.iter().find(|task| task.id == *task_id) {
                upsert_task_projection(tx, task).await?;
            }
            for grant in snapshot
                .task_grants
                .iter()
                .filter(|grant| grant.task_id == *task_id)
            {
                sqlx::query(
                    "UPDATE task_grants SET revoked = ?, value_json = ? WHERE grant_id = ?",
                )
                .bind(if grant.revoked { 1_i64 } else { 0_i64 })
                .bind(serde_json::to_string(grant)?)
                .bind(&grant.id)
                .execute(&mut **tx)
                .await?;
            }
        }
        Event::TaskWorktreeProvisioning { task, operation } => {
            upsert_task_projection(tx, task).await?;
            upsert_dependent_task_projections(tx, snapshot, task.id).await?;
            let value_json = serde_json::to_string(operation)?;
            let kind = serde_json::to_value(operation.kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let status = serde_json::to_value(operation.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
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
                .execute(&mut **tx)
                .await?;
        }
        Event::ReviewWorkItemOpened { task, .. } | Event::ReviewWorkItemDecided { task, .. } => {
            // The review item itself is part of the authoritative snapshot;
            // keep the task projection in sync with the associated review
            // transition.  A separate projection table is intentionally not
            // required for replay because review_items are snapshot DTOs.
            upsert_task_projection(tx, task).await?;
            upsert_dependent_task_projections(tx, snapshot, task.id).await?;
        }
        Event::TaskActivityAdded { entry } => {
            let kind = serde_json::to_value(entry.kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO task_feed (feed_id, task_id, kind, actor_json, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(feed_id) DO UPDATE SET task_id = excluded.task_id, kind = excluded.kind, actor_json = excluded.actor_json, value_json = excluded.value_json, created_at = excluded.created_at")
                .bind(entry.id.to_string())
                .bind(entry.task_id.to_string())
                .bind(kind)
                .bind(serde_json::to_string(&entry.actor)?)
                .bind(serde_json::to_string(entry)?)
                .bind(&entry.created_at)
                .execute(&mut **tx)
                .await?;
            for artifact_id in &entry.artifact_ids {
                sqlx::query("INSERT OR IGNORE INTO task_artifact_links (task_id, artifact_id, linked_at) VALUES (?, ?, ?)")
                    .bind(entry.task_id.to_string())
                    .bind(artifact_id.to_string())
                    .bind(&entry.created_at)
                    .execute(&mut **tx)
                .await?;
            }
        }
        Event::AgentDeparted {
            agent_id,
            blocked_task_ids,
            ..
        } => {
            if let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == *agent_id) {
                upsert(
                    tx,
                    "agent_profiles",
                    "agent_id",
                    &agent.id.to_string(),
                    serde_json::to_value(agent)?,
                )
                .await?;
                upsert_provider_session_projection(tx, agent, snapshot).await?;
            }
            for task_id in blocked_task_ids {
                if let Some(task) = snapshot.tasks.iter().find(|task| task.id == *task_id) {
                    upsert_task_projection(tx, task).await?;
                }
            }
            for grant in snapshot
                .task_grants
                .iter()
                .filter(|grant| grant.agent_id == *agent_id)
            {
                sqlx::query(
                    "UPDATE task_grants SET revoked = ?, value_json = ? WHERE grant_id = ?",
                )
                .bind(if grant.revoked { 1_i64 } else { 0_i64 })
                .bind(serde_json::to_string(grant)?)
                .bind(&grant.id)
                .execute(&mut **tx)
                .await?;
            }
        }
        Event::WorkOfferCreated { offer } | Event::WorkOfferResponded { offer } => {
            let status = serde_json::to_value(offer.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO work_offers (offer_id, task_id, agent_id, status, value_json, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(offer_id) DO UPDATE SET task_id = excluded.task_id, agent_id = excluded.agent_id, status = excluded.status, value_json = excluded.value_json, expires_at = excluded.expires_at")
                .bind(offer.id.to_string())
                .bind(offer.task_id.to_string())
                .bind(offer.agent_id.to_string())
                .bind(status)
                .bind(serde_json::to_string(offer)?)
                .bind(&offer.created_at)
                .bind(&offer.expires_at)
                .execute(&mut **tx)
                .await?;
            if let Some(task) = snapshot.tasks.iter().find(|task| task.id == offer.task_id) {
                upsert_task_projection(tx, task).await?;
            }
        }
        Event::HumanInputRequested { task, input } | Event::HumanInputResolved { task, input } => {
            let status = serde_json::to_value(input.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO human_input_requests (human_input_id, task_id, status, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(human_input_id) DO UPDATE SET task_id = excluded.task_id, status = excluded.status, value_json = excluded.value_json, updated_at = excluded.updated_at")
                .bind(input.id.to_string())
                .bind(input.task_id.to_string())
                .bind(status)
                .bind(serde_json::to_string(input)?)
                .bind(&input.created_at)
                .bind(&input.updated_at)
                .execute(&mut **tx)
                .await?;
            upsert_task_projection(tx, task).await?;
        }
        Event::TaskReworkRequested { task, .. } => {
            upsert_task_projection(tx, task).await?;
        }
        Event::OrganizationDrainRequested { .. } | Event::OrganizationDrainCompleted { .. } => {
            sqlx::query(
                "INSERT INTO organization_state (id, value_json) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json",
            )
            .bind(serde_json::to_string(&snapshot.organization)? )
            .execute(&mut **tx)
            .await?;
        }
        Event::WorkItemsRelocated { relocation } => {
            sqlx::query("INSERT INTO organization_relocations (relocation_id, from_board_id, to_board_id, from_revision, to_revision, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(relocation_id) DO UPDATE SET from_board_id = excluded.from_board_id, to_board_id = excluded.to_board_id, from_revision = excluded.from_revision, to_revision = excluded.to_revision, value_json = excluded.value_json")
                .bind(relocation.id.to_string())
                .bind(relocation.from_board_id.to_string())
                .bind(relocation.to_board_id.to_string())
                .bind(relocation.from_revision as i64)
                .bind(relocation.to_revision as i64)
                .bind(serde_json::to_string(relocation)?)
                .bind(&relocation.created_at)
                .execute(&mut **tx)
                .await?;
            for task_id in &relocation.task_ids {
                if let Some(task) = snapshot.tasks.iter().find(|task| task.id == *task_id) {
                    upsert_task_projection(tx, task).await?;
                }
            }
        }
        Event::MessageQueued { message } => {
            sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                .bind(message.id.to_string())
                .bind(message.mission_id.to_string())
                .bind(serde_json::to_string(message)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::MessageDelivered { message_id }
        | Event::MessageAcknowledged { message_id }
        | Event::MessageCompleted { message_id }
        | Event::MessageFailed { message_id } => {
            if let Some(message) = snapshot
                .messages
                .iter()
                .find(|message| message.id == *message_id)
            {
                sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(message.id.to_string())
                    .bind(message.mission_id.to_string())
                    .bind(serde_json::to_string(message)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::ApprovalRequested { approval } => {
            sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                .bind(approval.id.to_string())
                .bind(approval.task_id.to_string())
                .bind(serde_json::to_string(approval)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::ApprovalDecided { approval_id, .. } | Event::ApprovalExpired { approval_id } => {
            if let Some(approval) = snapshot
                .approvals
                .iter()
                .find(|approval| approval.id == *approval_id)
            {
                sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                    .bind(approval.id.to_string())
                    .bind(approval.task_id.to_string())
                    .bind(serde_json::to_string(approval)?)
                    .execute(&mut **tx)
                .await?;
            }
        }
        Event::TaskGrantCreated { grant } => {
            sqlx::query("INSERT INTO task_grants (grant_id, task_id, agent_id, worktree, effect, expires_at, revoked, value_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(grant_id) DO UPDATE SET task_id = excluded.task_id, agent_id = excluded.agent_id, worktree = excluded.worktree, effect = excluded.effect, expires_at = excluded.expires_at, revoked = excluded.revoked, value_json = excluded.value_json")
                .bind(&grant.id)
                .bind(grant.task_id.to_string())
                .bind(grant.agent_id.to_string())
                .bind(&grant.worktree)
                .bind(serde_json::to_value(grant.effect)?.as_str().unwrap_or("unknown"))
                .bind(&grant.expires_at)
                .bind(if grant.revoked { 1_i64 } else { 0_i64 })
                .bind(serde_json::to_string(grant)?)
                .execute(&mut **tx)
                .await?;
            if let Some(approval_id) = grant.source_approval_id
                && let Some(approval) = snapshot
                    .approvals
                    .iter()
                    .find(|approval| approval.id == approval_id)
            {
                sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                    .bind(approval.id.to_string())
                    .bind(approval.task_id.to_string())
                    .bind(serde_json::to_string(approval)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TaskGrantRevoked { grant_id } => {
            if let Some(grant) = snapshot
                .task_grants
                .iter()
                .find(|grant| grant.id == *grant_id)
            {
                sqlx::query(
                    "UPDATE task_grants SET revoked = 1, value_json = ? WHERE grant_id = ?",
                )
                .bind(serde_json::to_string(grant)?)
                .bind(grant_id)
                .execute(&mut **tx)
                .await?;
            }
        }
        Event::ArtifactPublished { artifact } => {
            sqlx::query("INSERT INTO artifacts (artifact_id, mission_id, value_json, bytes, pinned) VALUES (?, ?, ?, X'', ?) ON CONFLICT(artifact_id) DO UPDATE SET value_json = excluded.value_json, pinned = excluded.pinned")
                .bind(artifact.id.to_string())
                .bind(artifact.mission_id.to_string())
                .bind(serde_json::to_string(artifact)?)
                .bind(if artifact.pinned { 1_i64 } else { 0_i64 })
                .execute(&mut **tx)
                .await?;
            if let Some(task_id) = artifact.task_id {
                sqlx::query("INSERT OR IGNORE INTO task_artifact_links (task_id, artifact_id, linked_at) VALUES (?, ?, ?)")
                    .bind(task_id.to_string())
                    .bind(artifact.id.to_string())
                    .bind(&artifact.created_at)
                    .execute(&mut **tx)
                    .await?;
            }
            if let Some(upload_id) = artifact.upload_id {
                // A streamed upload keeps its bytes in the upload row until
                // the final command commits. Copy those bytes into the
                // authoritative artifact row in this same transaction as the
                // publication event. Without this step a successful
                // FinalizeArtifactUpload would expose correct metadata but a
                // zero-byte download after reconnect/restart.
                let copied = sqlx::query(
                    "UPDATE artifacts SET bytes = (SELECT bytes FROM artifact_uploads WHERE upload_id = ?) WHERE artifact_id = ? AND EXISTS (SELECT 1 FROM artifact_uploads WHERE upload_id = ? AND completed = 0 AND received_size = expected_size AND CAST(expires_at AS INTEGER) > ?)",
                )
                .bind(upload_id.to_string())
                .bind(artifact.id.to_string())
                .bind(upload_id.to_string())
                .bind(unix_seconds() as i64)
                .execute(&mut **tx)
                .await?;
                if copied.rows_affected() != 1 {
                    return Err(StoreError::Validation(
                        "artifact upload bytes are unavailable or expired".into(),
                    ));
                }
                // The event projection is also guarded by the upload lease.
                // Normally the orchestrator validates expiry before emitting
                // `ArtifactPublished`, but replay/recovery must not turn a
                // stale upload into a completed one.
                sqlx::query("UPDATE artifact_uploads SET completed = 1 WHERE upload_id = ? AND received_size = expected_size AND CAST(expires_at AS INTEGER) > ?")
                    .bind(upload_id.to_string())
                    .bind(unix_seconds() as i64)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TerminalOpened { session } => {
            upsert_terminal_projection(tx, session).await?;
        }
        Event::TerminalLeaseChanged { lease } => {
            if let Some(session) = snapshot
                .terminals
                .iter()
                .find(|session| session.id == lease.session_id)
            {
                upsert_terminal_projection(tx, session).await?;
            }
            if lease.expires_at == "released" {
                sqlx::query("DELETE FROM control_leases WHERE session_id = ?")
                    .bind(lease.session_id.to_string())
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query("INSERT INTO control_leases (lease_id, session_id, value_json) VALUES (?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET lease_id = excluded.lease_id, value_json = excluded.value_json")
                    .bind(&lease.lease_id)
                    .bind(lease.session_id.to_string())
                    .bind(serde_json::to_string(lease)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TerminalClosed { session_id } => {
            if let Some(session) = snapshot
                .terminals
                .iter()
                .find(|session| session.id == *session_id)
            {
                upsert_terminal_projection(tx, session).await?;
            }
            sqlx::query("DELETE FROM control_leases WHERE session_id = ?")
                .bind(session_id.to_string())
                .execute(&mut **tx)
                .await?;
        }
        Event::UsageRecorded { usage } => {
            sqlx::query("INSERT INTO usage_records (attempt_id, scope_id, value_json) VALUES (?, ?, ?) ON CONFLICT(attempt_id) DO UPDATE SET scope_id = excluded.scope_id, value_json = excluded.value_json")
                .bind(usage.id.to_string())
                .bind(&usage.scope_id)
                .bind(serde_json::to_string(usage)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::OperationChanged { operation } => {
            let value_json = serde_json::to_string(operation)?;
            let kind = serde_json::to_value(operation.kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let status = serde_json::to_value(operation.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
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
                .execute(&mut **tx)
                .await?;
        }
        Event::ArtifactUploadStarted { upload } => {
            sqlx::query("INSERT INTO artifact_uploads (upload_id, mission_id, task_id, name, mime_type, expected_size, expected_sha256, received_size, bytes, completed, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, X'', 0, ?, ?) ON CONFLICT(upload_id) DO UPDATE SET received_size = excluded.received_size, completed = excluded.completed, expires_at = excluded.expires_at")
                .bind(upload.id.to_string())
                .bind(upload.spec.mission_id.to_string())
                .bind(upload.spec.task_id.map(|id| id.to_string()))
                .bind(&upload.spec.name)
                .bind(&upload.spec.mime_type)
                .bind(upload.spec.size as i64)
                .bind(&upload.spec.sha256)
                .bind(upload.received as i64)
                .bind(&upload.created_at)
                .bind(&upload.expires_at)
                .execute(&mut **tx)
                .await?;
        }
        Event::UpdateStateChanged { update } => {
            let state = serde_json::to_value(update.state.clone())?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO update_history (update_id, version, target, state, error, changed_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(update_id) DO UPDATE SET version = excluded.version, target = excluded.target, state = excluded.state, error = excluded.error, changed_at = excluded.changed_at")
                .bind(update.id.to_string())
                .bind(&update.version)
                .bind(&update.target)
                .bind(state)
                .bind(&update.error)
                .bind(&update.checked_at)
                .execute(&mut **tx)
                .await?;
            // Apply/Rollback update commands carry a host operation in the
            // same snapshot. Keep the narrow operation projection in sync
            // even though the event envelope intentionally remains a single
            // UpdateStateChanged event for wire compatibility.
            for operation in snapshot
                .operations
                .iter()
                .filter(|operation| operation.kind == frank_protocol::OperationKind::HostUpdate)
            {
                let value_json = serde_json::to_string(operation)?;
                let kind = serde_json::to_value(operation.kind)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let status = serde_json::to_value(operation.status)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
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
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::SnapshotReplaced { .. }
        | Event::BudgetPaused { .. }
        | Event::MemoryProposed { .. }
        | Event::MemoryRead { .. }
        | Event::DeliveryStarted { .. }
        | Event::DeliveryCompleted { .. }
        | Event::DeliveryBlocked { .. }
        | Event::SupervisorPlanProposed { .. } => {}
        Event::ToolchainInstallationRecorded {
            manifest_id,
            version,
            runner_id,
            status,
            project_id,
            task_id,
            install_path,
        } => {
            let value = serde_json::json!({
                "manifest_id": manifest_id,
                "version": version,
                "runner_id": runner_id,
                "status": status,
                "project_id": project_id,
                "task_id": task_id,
                "install_path": install_path,
            });
            sqlx::query("INSERT INTO toolchain_installations (manifest_id, version, runner_id, value_json, status, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(manifest_id, version, runner_id) DO UPDATE SET value_json = excluded.value_json, status = excluded.status, updated_at = excluded.updated_at")
                .bind(manifest_id)
                .bind(version)
                .bind(runner_id.to_string())
                .bind(serde_json::to_string(&value)?)
                .bind(serde_json::to_value(status)?.as_str().unwrap_or("unknown"))
                .bind(timestamp_now())
                .execute(&mut **tx)
                .await?;
        }
        Event::CheckRunRecorded { check } => {
            let status = serde_json::to_value(check.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO check_runs (check_run_id, project_id, task_id, runner_id, value_json, status, started_at, finished_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(check_run_id) DO UPDATE SET value_json = excluded.value_json, status = excluded.status, finished_at = excluded.finished_at")
                .bind(&check.id)
                .bind(check.project_id.to_string())
                .bind(check.task_id.map(|id| id.to_string()))
                .bind(check.runner_id.to_string())
                .bind(serde_json::to_string(check)?)
                .bind(status)
                .bind(&check.started_at)
                .bind(&check.finished_at)
                .execute(&mut **tx)
                .await?;
        }
    }
    // Task transitions append feed entries directly to the authoritative
    // snapshot. Re-project the small feed rows here as well so status/claim
    // events and comments share one durable activity table.
    for entry in snapshot.task_feed.iter().take(2_048) {
        let kind = serde_json::to_value(entry.kind)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        sqlx::query("INSERT INTO task_feed (feed_id, task_id, kind, actor_json, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(feed_id) DO UPDATE SET task_id = excluded.task_id, kind = excluded.kind, actor_json = excluded.actor_json, value_json = excluded.value_json, created_at = excluded.created_at")
            .bind(entry.id.to_string())
            .bind(entry.task_id.to_string())
            .bind(kind)
            .bind(serde_json::to_string(&entry.actor)?)
            .bind(serde_json::to_string(entry)?)
            .bind(&entry.created_at)
            .execute(&mut **tx)
            .await?;
        for artifact_id in &entry.artifact_ids {
            sqlx::query("INSERT OR IGNORE INTO task_artifact_links (task_id, artifact_id, linked_at) VALUES (?, ?, ?)")
                .bind(entry.task_id.to_string())
                .bind(artifact_id.to_string())
                .bind(&entry.created_at)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

async fn upsert_task_projection(
    tx: &mut Transaction<'_, Sqlite>,
    task: &frank_protocol::TaskView,
) -> Result<()> {
    sqlx::query("INSERT INTO tasks (task_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(task_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
        .bind(task.id.to_string())
        .bind(task.mission_id.to_string())
        .bind(serde_json::to_string(task)?)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM task_dependencies WHERE task_id = ?")
        .bind(task.id.to_string())
        .execute(&mut **tx)
        .await?;
    for dependency in &task.dependencies {
        sqlx::query("INSERT INTO task_dependencies (task_id, depends_on) VALUES (?, ?)")
            .bind(task.id.to_string())
            .bind(dependency.to_string())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn upsert_dependent_task_projections(
    tx: &mut Transaction<'_, Sqlite>,
    snapshot: &Snapshot,
    dependency_id: frank_protocol::TaskId,
) -> Result<()> {
    for task in snapshot
        .tasks
        .iter()
        .filter(|task| task.dependencies.contains(&dependency_id))
    {
        upsert_task_projection(tx, task).await?;
    }
    Ok(())
}

async fn upsert_terminal_projection(
    tx: &mut Transaction<'_, Sqlite>,
    session: &frank_protocol::TerminalSessionView,
) -> Result<()> {
    sqlx::query("INSERT INTO terminal_sessions (session_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
        .bind(session.id.to_string())
        .bind(session.task_id.to_string())
        .bind(serde_json::to_string(session)?)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn upsert_provider_session_projection(
    tx: &mut Transaction<'_, Sqlite>,
    agent: &frank_protocol::AgentView,
    snapshot: &Snapshot,
) -> Result<()> {
    if let Some(session_id) = agent.provider_session_id.as_deref() {
        sqlx::query("DELETE FROM provider_sessions WHERE agent_id = ? AND session_id != ?")
            .bind(agent.id.to_string())
            .bind(session_id)
            .execute(&mut **tx)
            .await?;
        let task_id = snapshot
            .tasks
            .iter()
            .find(|task| {
                task.assigned_agent == Some(agent.id)
                    && task.status == frank_protocol::TaskStatus::Running
            })
            .map(|task| task.id.to_string());
        sqlx::query("INSERT INTO provider_sessions (session_id, provider, agent_id, task_id, value_json, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET provider = excluded.provider, agent_id = excluded.agent_id, task_id = excluded.task_id, value_json = excluded.value_json, updated_at = excluded.updated_at")
            .bind(session_id)
            .bind("openrouter")
            .bind(agent.id.to_string())
            .bind(task_id)
            .bind(serde_json::to_string(agent)?)
            .bind(timestamp_now())
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM provider_sessions WHERE agent_id = ?")
            .bind(agent.id.to_string())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionTable {
    Project,
    Agent,
    Role,
    Mission,
    Task,
    Taskboard,
    WorkOffer,
    HumanInput,
    OrganizationRelocation,
    Message,
    Approval,
    TaskFeed,
}
