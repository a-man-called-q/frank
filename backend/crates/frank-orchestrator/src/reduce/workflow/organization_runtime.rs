//! Organization runtime drain, resume, and relocation handlers.

use frank_protocol::*;

use super::ensure_board;
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_request_organization_drain(
        &self,
        mut snapshot: Snapshot,
        target_revision: u64,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let active_revision = snapshot
            .organization
            .published
            .as_ref()
            .map(|graph| graph.published_revision)
            .unwrap_or_default();
        if target_revision == 0 || target_revision < active_revision {
            return Err(OrchestratorError::Validation(
                "drain target revision must be the active or newer Organization revision".into(),
            ));
        }
        snapshot.organization_runtime.active_revision = active_revision;
        snapshot.organization_runtime.status = OrganizationDrainStatus::Draining;
        snapshot.organization_runtime.drain_requested_revision = Some(target_revision);
        // Only cards that captured an older Organization revision
        // require an explicit relocation. Legacy cards without that
        // additive metadata remain valid during the migration.
        snapshot.organization_runtime.pending_relocation_count = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.organization_revision
                    .is_some_and(|revision| revision < target_revision)
                    && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
            })
            .count()
            .min(u32::MAX as usize)
            as u32;
        Ok((
            snapshot,
            Event::OrganizationDrainRequested { target_revision },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_complete_organization_drain(
        &self,
        mut snapshot: Snapshot,
        revision: u64,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if actor.kind != ActorKind::System {
            return Err(OrchestratorError::Forbidden);
        }
        if snapshot.organization_runtime.status != OrganizationDrainStatus::Draining {
            return Err(OrchestratorError::Validation(
                "Organization is not currently draining".into(),
            ));
        }
        if snapshot.organization_runtime.drain_requested_revision != Some(revision) {
            return Err(OrchestratorError::Validation(
                "drain completion revision does not match the requested revision".into(),
            ));
        }
        let has_active_work = snapshot.tasks.iter().any(|task| {
            snapshot.missions.iter().any(|mission| {
                mission.id == task.mission_id && mission.status == MissionStatus::Active
            }) && matches!(task.status, TaskStatus::Running | TaskStatus::Review)
        });
        if has_active_work {
            return Err(OrchestratorError::Validation(
                "Organization still has active work".into(),
            ));
        }
        if snapshot.organization_runtime.pending_relocation_count != 0 {
            return Err(OrchestratorError::Validation(
                "relocate every unfinished card before completing Organization drain".into(),
            ));
        }
        snapshot.organization_runtime.status = OrganizationDrainStatus::Paused;
        snapshot.organization_runtime.paused_revision = Some(revision);
        snapshot.organization_runtime.drain_requested_revision = None;
        Ok((
            snapshot,
            Event::OrganizationDrainCompleted { revision },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_resume_organization(
        &self,
        mut snapshot: Snapshot,
        revision: u64,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if snapshot.organization_runtime.status != OrganizationDrainStatus::Paused {
            return Err(OrchestratorError::Validation(
                "Organization must be paused before it can resume".into(),
            ));
        }
        let published_revision = snapshot
            .organization
            .published
            .as_ref()
            .map(|graph| graph.published_revision)
            .unwrap_or_default();
        if revision == 0 || revision != published_revision {
            return Err(OrchestratorError::Validation(
                "resume revision does not match the published Organization".into(),
            ));
        }
        if snapshot.organization_runtime.pending_relocation_count != 0 {
            return Err(OrchestratorError::Validation(
                "relocate every unfinished card before resuming Organization".into(),
            ));
        }
        snapshot.organization_runtime.active_revision = revision;
        snapshot.organization_runtime.status = OrganizationDrainStatus::Running;
        snapshot.organization_runtime.paused_revision = None;
        Ok((
            snapshot,
            Event::OrganizationDrainCompleted { revision },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_relocate_work_items(
        &self,
        mut snapshot: Snapshot,
        from_board_id: TaskboardId,
        to_board_id: TaskboardId,
        task_ids: Vec<TaskId>,
        reason: Option<String>,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        ensure_board(&snapshot, from_board_id)?;
        ensure_board(&snapshot, to_board_id)?;
        if from_board_id == to_board_id || task_ids.is_empty() {
            return Err(OrchestratorError::Validation(
                "relocation requires two boards and at least one card".into(),
            ));
        }
        if task_ids.len() > 256
            || reason
                .as_deref()
                .is_some_and(|value| !valid_content_text(value, MAX_MESSAGE_BODY_BYTES))
        {
            return Err(OrchestratorError::Validation(
                "relocation card list or reason is invalid".into(),
            ));
        }
        let relocation_revision = snapshot
            .organization_runtime
            .drain_requested_revision
            .unwrap_or(snapshot.organization_runtime.active_revision);
        let mut moved = Vec::with_capacity(task_ids.len());
        for task_id in &task_ids {
            if moved.contains(task_id) {
                return Err(OrchestratorError::Validation(
                    "relocation cannot contain duplicate cards".into(),
                ));
            }
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == *task_id)
                .ok_or(OrchestratorError::NotFound)?;
            if task.taskboard_id != Some(from_board_id) {
                return Err(OrchestratorError::Validation(
                    "relocation card is not on the source board".into(),
                ));
            }
            if matches!(task.status, TaskStatus::Running | TaskStatus::Review) {
                return Err(OrchestratorError::Validation(
                    "active work must finish before its card is relocated".into(),
                ));
            }
            task.taskboard_id = Some(to_board_id);
            task.organization_revision = Some(relocation_revision);
            task.active_role_node_id = None;
            task.assigned_agent = None;
            task.claimed_at = None;
            task.claim_source = None;
            moved.push(*task_id);
        }
        let relocation = OrganizationRelocationView {
            id: RelocationId::new(),
            from_revision: snapshot.organization_runtime.active_revision,
            to_revision: relocation_revision,
            from_board_id,
            to_board_id,
            task_ids: moved,
            reason,
            created_at: timestamp_now(),
        };
        snapshot.organization_relocations.push(relocation.clone());
        snapshot.organization_runtime.pending_relocation_count = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.organization_revision
                    .is_some_and(|revision| revision < relocation.to_revision)
                    && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
            })
            .count()
            .min(u32::MAX as usize)
            as u32;
        Ok((
            snapshot,
            Event::WorkItemsRelocated { relocation },
            CommandResult::Accepted,
        ))
    }
}
