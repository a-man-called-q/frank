//! Organization graph draft and publish handlers.

use std::collections::HashSet;

use frank_protocol::*;

use super::validation::validate_graph;
use crate::{Orchestrator, OrchestratorError, Result};

impl Orchestrator {
    pub(super) async fn reduce_save_organization_draft(
        &self,
        mut snapshot: Snapshot,
        graph: OrganizationGraph,
        expected_draft_revision: u64,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if graph.id != snapshot.organization.draft.id {
            return Err(OrchestratorError::Validation(
                "organization id does not match the server document".into(),
            ));
        }
        if graph.draft_revision != expected_draft_revision
            || snapshot.organization.draft.draft_revision != expected_draft_revision
        {
            return Err(OrchestratorError::OrganizationRevisionConflict {
                expected: expected_draft_revision,
                actual: snapshot.organization.draft.draft_revision,
            });
        }
        validate_graph(&snapshot, &graph, false)?;
        let mut next = graph;
        next.published_revision = snapshot
            .organization
            .published
            .as_ref()
            .map(|value| value.published_revision)
            .unwrap_or_default();
        next.draft_revision = expected_draft_revision.saturating_add(1);
        snapshot.organization.draft = next.clone();
        Ok((
            snapshot,
            Event::OrganizationDraftSaved { graph: next },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_publish_organization(
        &self,
        mut snapshot: Snapshot,
        expected_published_revision: u64,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let mut graph = snapshot.organization.draft.clone();
        let actual = snapshot
            .organization
            .published
            .as_ref()
            .map(|value| value.published_revision)
            .unwrap_or_default();
        if actual != expected_published_revision {
            return Err(OrchestratorError::OrganizationRevisionConflict {
                expected: expected_published_revision,
                actual,
            });
        }
        // Profile metadata is event-sourced, while external
        // credentials live behind the daemon-only resolver.  Check
        // both boundaries before the atomic publish so a graph can
        // never become active with a Google/Postgres profile that
        // has no credential at runtime.
        self.validate_published_connector_profiles(&snapshot, &graph)
            .await?;
        validate_graph(&snapshot, &graph, true)?;
        let next_revision = actual.saturating_add(1);
        graph.published_revision = next_revision;
        snapshot.organization.published = Some(graph.clone());
        snapshot.organization.draft.published_revision = graph.published_revision;
        if actual > 0 {
            // Publishing a replacement graph never interrupts an
            // active provider. Keep the old revision as the runtime
            // boundary and let reconciliation drain it before the
            // owner explicitly resumes on the new graph.
            snapshot.organization_runtime.status = OrganizationDrainStatus::Draining;
            snapshot.organization_runtime.drain_requested_revision = Some(next_revision);
            let new_board_ids = graph
                .nodes
                .iter()
                .filter_map(|node| node.taskboard_id)
                .collect::<std::collections::HashSet<_>>();
            snapshot.organization_runtime.pending_relocation_count = snapshot
                .tasks
                .iter()
                .filter(|task| {
                    !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
                        && task
                            .taskboard_id
                            .is_some_and(|board| !new_board_ids.contains(&board))
                })
                .count()
                as u32;
        } else {
            snapshot.organization_runtime.active_revision = next_revision;
        }
        Ok((
            snapshot,
            Event::OrganizationPublished { graph },
            CommandResult::Accepted,
        ))
    }

    async fn validate_published_connector_profiles(
        &self,
        snapshot: &Snapshot,
        graph: &OrganizationGraph,
    ) -> Result<()> {
        let profile_ids = graph
            .nodes
            .iter()
            .filter(|node| node.kind == OrganizationNodeKind::Capability)
            .filter_map(|node| node.connector_profile_id)
            .collect::<HashSet<_>>();
        for profile_id in profile_ids {
            let profile = snapshot
                .organization
                .connector_profiles
                .iter()
                .find(|profile| profile.id == profile_id && !profile.archived)
                .ok_or(OrchestratorError::NotFound)?;
            if matches!(
                profile.health,
                ConnectorHealth::Unhealthy | ConnectorHealth::Degraded
            ) {
                return Err(OrchestratorError::Validation(
                    "published Organization cannot reference an unhealthy connector profile".into(),
                ));
            }
            if matches!(
                profile.kind,
                ConnectorKind::GoogleWorkspace | ConnectorKind::Postgres
            ) {
                let configured = self
                    .connector_secret(profile.id)
                    .await
                    .map_err(OrchestratorError::ProviderUnavailable)?
                    .is_some_and(|secret| !secret.trim().is_empty());
                if !configured {
                    return Err(OrchestratorError::Validation(
                        "connector profile credential is not configured".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
