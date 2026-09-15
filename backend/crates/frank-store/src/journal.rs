//! Owner-facing Journal projection and event classification.

use frank_protocol::{
    ActorRef, Event, EventEnvelope, JournalEntryKind, JournalEntryView, JournalFilter,
    JournalOutcome, JournalPage,
};
use serde_json::Value;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{Result, Store};

pub(crate) async fn insert_journal_entry_tx(
    tx: &mut Transaction<'_, Sqlite>,
    envelope: &EventEnvelope,
    snapshot: &frank_protocol::Snapshot,
) -> Result<()> {
    let entry = classify_event(envelope, snapshot);
    sqlx::query(
        "INSERT OR REPLACE INTO journal_entries (sequence, occurred_at, actor_json, kind, outcome, project_id, mission_id, task_id, agent_id, check_run_id, summary, detail_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.sequence as i64)
    .bind(&entry.occurred_at)
    .bind(serde_json::to_string(&entry.actor)?)
    .bind(enum_name(&entry.kind))
    .bind(enum_name(&entry.outcome))
    .bind(entry.project_id.map(|id| id.to_string()))
    .bind(entry.mission_id.map(|id| id.to_string()))
    .bind(entry.task_id.map(|id| id.to_string()))
    .bind(entry.agent_id.map(|id| id.to_string()))
    .bind(entry.check_run_id)
    .bind(&entry.summary)
    .bind(serde_json::to_string(&entry.detail)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Rebuild the projection from the retained event log during an upgrade. The
/// event log is intentionally the only recovery source; a partially written
/// Journal row is safe to replace because its sequence is the primary key.
pub(crate) async fn rebuild_journal_from_events(pool: &SqlitePool) -> Result<u64> {
    let snapshot_json: String =
        sqlx::query_scalar("SELECT snapshot_json FROM server_state WHERE id = 1")
            .fetch_one(pool)
            .await?;
    let snapshot: frank_protocol::Snapshot = serde_json::from_str(&snapshot_json)?;
    let rows =
        sqlx::query("SELECT seq, occurred_at, actor_json, event_json FROM events ORDER BY seq ASC")
            .fetch_all(pool)
            .await?;
    let mut tx = pool.begin().await?;
    let mut rebuilt = 0_u64;
    for row in rows {
        let envelope = EventEnvelope {
            seq: row.try_get::<i64, _>("seq")?.max(0) as u64,
            occurred_at: row.try_get("occurred_at")?,
            actor: serde_json::from_str(&row.try_get::<String, _>("actor_json")?)?,
            correlation_id: None,
            event: serde_json::from_str(&row.try_get::<String, _>("event_json")?)?,
        };
        insert_journal_entry_tx(&mut tx, &envelope, &snapshot).await?;
        rebuilt = rebuilt.saturating_add(1);
    }
    tx.commit().await?;
    Ok(rebuilt)
}

impl Store {
    pub async fn journal_page(&self, filter: &JournalFilter) -> Result<JournalPage> {
        let limit = filter.limit.unwrap_or(50).clamp(1, 200) as i64;
        let mut sql = String::from(
            "SELECT sequence, occurred_at, actor_json, kind, outcome, project_id, mission_id, task_id, agent_id, check_run_id, summary, detail_json FROM journal_entries WHERE 1 = 1",
        );
        if filter.before_sequence.is_some() {
            sql.push_str(" AND sequence < ?");
        }
        if filter.project_id.is_some() {
            sql.push_str(" AND project_id = ?");
        }
        if filter.mission_id.is_some() {
            sql.push_str(" AND mission_id = ?");
        }
        if filter.task_id.is_some() {
            sql.push_str(" AND task_id = ?");
        }
        if filter.agent_id.is_some() {
            sql.push_str(" AND agent_id = ?");
        }
        if !filter.kinds.is_empty() {
            sql.push_str(" AND kind IN (");
            sql.push_str(&vec!["?"; filter.kinds.len()].join(","));
            sql.push(')');
        }
        if !filter.outcomes.is_empty() {
            sql.push_str(" AND outcome IN (");
            sql.push_str(&vec!["?"; filter.outcomes.len()].join(","));
            sql.push(')');
        }
        sql.push_str(" ORDER BY sequence DESC LIMIT ?");

        let mut query = sqlx::query(&sql);
        if let Some(value) = filter.before_sequence {
            query = query.bind(value as i64);
        }
        if let Some(value) = filter.project_id {
            query = query.bind(value.to_string());
        }
        if let Some(value) = filter.mission_id {
            query = query.bind(value.to_string());
        }
        if let Some(value) = filter.task_id {
            query = query.bind(value.to_string());
        }
        if let Some(value) = filter.agent_id {
            query = query.bind(value.to_string());
        }
        for value in &filter.kinds {
            query = query.bind(enum_name(value));
        }
        for value in &filter.outcomes {
            query = query.bind(enum_name(value));
        }
        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(row_to_entry(&row)?);
        }
        let next_before_sequence = entries.last().map(|entry| entry.sequence);
        Ok(JournalPage {
            entries,
            next_before_sequence,
        })
    }
}

fn row_to_entry(row: &sqlx::sqlite::SqliteRow) -> Result<JournalEntryView> {
    let kind: String = row.try_get("kind")?;
    let outcome: String = row.try_get("outcome")?;
    let detail_json: String = row.try_get("detail_json")?;
    Ok(JournalEntryView {
        sequence: row.try_get::<i64, _>("sequence")?.max(0) as u64,
        occurred_at: row.try_get("occurred_at")?,
        actor: serde_json::from_str::<ActorRef>(&row.try_get::<String, _>("actor_json")?)?,
        kind: parse_kind(&kind),
        outcome: parse_outcome(&outcome),
        summary: row.try_get("summary")?,
        detail: serde_json::from_str::<Value>(&detail_json).ok(),
        project_id: parse_id(row.try_get::<Option<String>, _>("project_id")?),
        mission_id: parse_id(row.try_get::<Option<String>, _>("mission_id")?),
        task_id: parse_id(row.try_get::<Option<String>, _>("task_id")?),
        agent_id: parse_id(row.try_get::<Option<String>, _>("agent_id")?),
        check_run_id: row.try_get("check_run_id")?,
    })
}

fn parse_id<T>(value: Option<String>) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    value.and_then(|value| serde_json::from_value(Value::String(value)).ok())
}

fn enum_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

fn parse_kind(value: &str) -> JournalEntryKind {
    serde_json::from_value(Value::String(value.to_string())).unwrap_or(JournalEntryKind::Event)
}

fn parse_outcome(value: &str) -> JournalOutcome {
    serde_json::from_value(Value::String(value.to_string())).unwrap_or(JournalOutcome::Info)
}

pub(crate) fn classify_event(
    envelope: &EventEnvelope,
    snapshot: &frank_protocol::Snapshot,
) -> JournalEntryView {
    let event = &envelope.event;
    let mut entry = JournalEntryView {
        sequence: envelope.seq,
        occurred_at: envelope.occurred_at.clone(),
        actor: envelope.actor.clone(),
        kind: JournalEntryKind::Event,
        outcome: JournalOutcome::Info,
        summary: event_summary(event),
        detail: serde_json::to_value(event).ok().map(sanitize_detail),
        project_id: None,
        mission_id: None,
        task_id: None,
        agent_id: None,
        check_run_id: None,
    };
    match event {
        Event::ProjectUpserted { project } => entry.project_id = Some(project.id),
        Event::ProjectArchived { project_id } => entry.project_id = Some(*project_id),
        Event::AgentUpserted { agent } => {
            entry.agent_id = Some(agent.id);
            entry.kind = JournalEntryKind::AgentLifecycle;
        }
        Event::AgentArchived { agent_id } | Event::AgentStatusChanged { agent_id, .. } => {
            entry.agent_id = Some(*agent_id);
            entry.kind = JournalEntryKind::AgentLifecycle;
        }
        Event::TaskCreated { task } | Event::TaskUpdated { task } => {
            entry.task_id = Some(task.id);
            entry.mission_id = Some(task.mission_id);
            entry.kind = JournalEntryKind::Task;
            entry.outcome = match task.status {
                frank_protocol::TaskStatus::Blocked => JournalOutcome::Blocked,
                frank_protocol::TaskStatus::Done => JournalOutcome::Success,
                _ => JournalOutcome::Info,
            };
        }
        Event::TaskStatusChanged { task_id, status } => {
            entry.task_id = Some(*task_id);
            entry.kind = JournalEntryKind::Task;
            entry.outcome = match status {
                frank_protocol::TaskStatus::Done => JournalOutcome::Success,
                frank_protocol::TaskStatus::Blocked => JournalOutcome::Blocked,
                _ => JournalOutcome::Info,
            };
        }
        Event::TaskActivityAdded { entry: feed } => {
            entry.task_id = Some(feed.task_id);
            entry.kind = JournalEntryKind::Comment;
        }
        Event::ApprovalRequested { approval } => {
            entry.task_id = Some(approval.task_id);
            entry.agent_id = Some(approval.agent_id);
            entry.kind = JournalEntryKind::Approval;
            entry.outcome = JournalOutcome::Pending;
        }
        Event::ApprovalDecided {
            approval_id,
            decision,
        } => {
            entry.kind = JournalEntryKind::Approval;
            if let Some(approval) = snapshot
                .approvals
                .iter()
                .find(|approval| approval.id == *approval_id)
            {
                entry.task_id = Some(approval.task_id);
                entry.agent_id = Some(approval.agent_id);
            }
            entry.outcome = match decision {
                frank_protocol::ApprovalDecision::AllowOnce
                | frank_protocol::ApprovalDecision::AllowForTask => JournalOutcome::Success,
                frank_protocol::ApprovalDecision::DenyOnce => JournalOutcome::Failure,
            };
        }
        Event::ApprovalExpired { .. } => {
            entry.kind = JournalEntryKind::Approval;
            entry.outcome = JournalOutcome::Failure;
        }
        Event::TaskGrantCreated { grant } => {
            entry.task_id = Some(grant.task_id);
            entry.agent_id = Some(grant.agent_id);
            entry.kind = JournalEntryKind::Approval;
            entry.outcome = JournalOutcome::Success;
        }
        Event::TaskGrantRevoked { .. } => entry.kind = JournalEntryKind::Approval,
        Event::UsageRecorded { usage } => {
            entry.agent_id = if usage.scope == frank_protocol::BudgetScope::Agent {
                frank_protocol::AgentId::parse(&usage.scope_id).ok()
            } else {
                None
            };
            entry.task_id = if usage.scope == frank_protocol::BudgetScope::Task {
                frank_protocol::TaskId::parse(&usage.scope_id).ok()
            } else {
                None
            };
            entry.kind = JournalEntryKind::Usage;
        }
        Event::OperationChanged { operation } => {
            entry.kind = if operation.kind == frank_protocol::OperationKind::RunChecks {
                JournalEntryKind::Check
            } else {
                JournalEntryKind::Event
            };
            entry.outcome = match operation.status {
                frank_protocol::OperationStatus::Succeeded => JournalOutcome::Success,
                frank_protocol::OperationStatus::Failed => JournalOutcome::Failure,
                frank_protocol::OperationStatus::Waiting => JournalOutcome::Pending,
                _ => JournalOutcome::Info,
            };
        }
        Event::ToolchainInstallationRecorded {
            manifest_id,
            project_id,
            task_id,
            status,
            ..
        } => {
            entry.project_id = *project_id;
            entry.task_id = *task_id;
            entry.kind = JournalEntryKind::Toolchain;
            entry.summary = format!("Toolchain installation: {manifest_id}");
            entry.outcome = match status {
                frank_protocol::ToolchainRequirementStatus::Ready => JournalOutcome::Success,
                frank_protocol::ToolchainRequirementStatus::Failed
                | frank_protocol::ToolchainRequirementStatus::ManualRequirement => {
                    JournalOutcome::Failure
                }
                frank_protocol::ToolchainRequirementStatus::NeedsApproval
                | frank_protocol::ToolchainRequirementStatus::Installing => JournalOutcome::Pending,
                _ => JournalOutcome::Info,
            };
        }
        Event::CheckRunRecorded { check } => {
            entry.project_id = Some(check.project_id);
            entry.task_id = check.task_id;
            entry.check_run_id = Some(check.id.clone());
            entry.kind = if check.check_id.to_ascii_lowercase().contains("build") {
                JournalEntryKind::Build
            } else {
                JournalEntryKind::Check
            };
            entry.outcome = match check.status {
                frank_protocol::CheckRunStatus::Passed => JournalOutcome::Success,
                frank_protocol::CheckRunStatus::Failed
                | frank_protocol::CheckRunStatus::TimedOut => JournalOutcome::Failure,
                _ => JournalOutcome::Pending,
            };
        }
        _ => {}
    }
    // A number of compact events intentionally carry only a TaskId (or a
    // grant/approval id). Enrich those rows from the authoritative snapshot
    // so Journal filters work consistently for every task lifecycle event.
    if let Some(task_id) = entry.task_id
        && let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id)
    {
        entry.mission_id.get_or_insert(task.mission_id);
        if entry.agent_id.is_none() {
            entry.agent_id = task.assigned_agent;
        }
        if entry.project_id.is_none() {
            entry.project_id = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .map(|mission| mission.project_id);
        }
    }
    if entry.task_id.is_none() {
        match event {
            Event::ApprovalExpired { approval_id } | Event::ApprovalDecided { approval_id, .. } => {
                if let Some(approval) = snapshot
                    .approvals
                    .iter()
                    .find(|approval| approval.id == *approval_id)
                {
                    entry.task_id = Some(approval.task_id);
                    entry.agent_id = Some(approval.agent_id);
                }
            }
            Event::TaskGrantRevoked { grant_id } => {
                if let Some(grant) = snapshot
                    .task_grants
                    .iter()
                    .find(|grant| grant.id == *grant_id)
                {
                    entry.task_id = Some(grant.task_id);
                    entry.agent_id = Some(grant.agent_id);
                }
            }
            _ => {}
        }
    }
    if let Some(task_id) = entry.task_id
        && entry.project_id.is_none()
        && let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id)
    {
        entry.project_id = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)
            .map(|mission| mission.project_id);
    }
    entry
}

fn sanitize_detail(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let value = if is_secret_key(&key) {
                        Value::String("[redacted]".into())
                    } else {
                        sanitize_detail(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_detail).collect()),
        Value::String(value) => Value::String(sanitize_string(&value)),
        other => other,
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "api_key",
        "apikey",
        "authorization",
        "credential",
        "private_key",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn sanitize_string(value: &str) -> String {
    let mut output = value.to_string();
    let mut search_from = 0;
    while let Some(relative) = output[search_from..].find("Bearer ") {
        let prefix_start = search_from + relative;
        let token_start = prefix_start + "Bearer ".len();
        let token_end = output[token_start..]
            .find(char::is_whitespace)
            .map(|offset| token_start + offset)
            .unwrap_or(output.len());
        output.replace_range(token_start..token_end, "[redacted]");
        search_from = token_start + "[redacted]".len();
    }
    for prefix in ["sk-", "rk-"] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(prefix) {
            let start = search_from + relative;
            let end = output[start..]
                .find(char::is_whitespace)
                .map(|offset| start + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "[redacted]");
            search_from = start + "[redacted]".len();
        }
    }
    for key in ["api_key=", "apikey=", "token=", "secret=", "password="] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(key) {
            let start = search_from + relative + key.len();
            let end = output[start..]
                .find(char::is_whitespace)
                .map(|offset| start + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "[redacted]");
            search_from = start + "[redacted]".len();
        }
    }
    output.chars().take(8_192).collect()
}

fn event_summary(event: &Event) -> String {
    match event {
        Event::TaskStatusChanged { status, .. } => format!("Task moved to {status:?}"),
        Event::AgentStatusChanged { status, .. } => format!("Agent status: {status:?}"),
        Event::ApprovalRequested { .. } => "Approval requested".into(),
        Event::ApprovalDecided { decision, .. } => format!("Approval decided: {decision:?}"),
        Event::UsageRecorded { .. } => "Model usage recorded".into(),
        Event::TaskActivityAdded { .. } => "Task comment added".into(),
        Event::TaskGrantCreated { .. } => "Task grant created".into(),
        Event::TaskGrantRevoked { .. } => "Task grant revoked".into(),
        Event::CheckRunRecorded { check } => format!("Check completed: {}", check.check_id),
        Event::ToolchainInstallationRecorded { manifest_id, .. } => {
            format!("Toolchain installation: {manifest_id}")
        }
        _ => "Server activity recorded".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_protocol::{ActorKind, Event, TaskId, TaskStatus};

    #[test]
    fn task_events_are_classified_with_safe_detail() {
        let envelope = EventEnvelope {
            seq: 4,
            occurred_at: "4".into(),
            actor: ActorRef {
                kind: ActorKind::System,
                id: None,
                display_name: None,
            },
            correlation_id: None,
            event: Event::TaskStatusChanged {
                task_id: TaskId::nil(),
                status: TaskStatus::Done,
            },
        };
        let entry = classify_event(
            &envelope,
            &frank_protocol::Snapshot::empty(frank_protocol::ServerId::nil()),
        );
        assert_eq!(entry.kind, JournalEntryKind::Task);
        assert_eq!(entry.outcome, JournalOutcome::Success);
        assert!(entry.detail.is_some());
    }

    #[test]
    fn journal_detail_redacts_bearer_and_key_shaped_values() {
        let sanitized =
            sanitize_string("Bearer super-secret sk-live-value token=abc password=hunter2 visible");
        assert!(!sanitized.contains("super-secret"));
        assert!(!sanitized.contains("sk-live-value"));
        assert!(!sanitized.contains("abc"));
        assert!(!sanitized.contains("hunter2"));
        assert!(sanitized.contains("visible"));
    }
}
