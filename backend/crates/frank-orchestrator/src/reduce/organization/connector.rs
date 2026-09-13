//! Connector profile handlers.

use frank_protocol::*;

use super::validation::{
    graph_references_profile, profile_compatible, sanitize_profile_config, valid_name,
    validate_profile_config_for_kind, validate_profile_spec,
};
use crate::{Orchestrator, OrchestratorError, Result};

impl Orchestrator {
    pub(super) async fn reduce_create_connector_profile(
        &self,
        mut snapshot: Snapshot,
        spec: ConnectorProfileSpec,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        validate_profile_spec(&spec)?;
        if snapshot
            .organization
            .connector_profiles
            .iter()
            .any(|profile| !profile.archived && profile.name.eq_ignore_ascii_case(&spec.name))
        {
            return Err(OrchestratorError::Validation(
                "connector profile name must be unique".into(),
            ));
        }
        let id = ConnectorProfileId::new();
        let profile = ConnectorProfileView {
            id,
            name: spec.name,
            kind: spec.kind,
            config: sanitize_profile_config(spec.config),
            health: ConnectorHealth::Unknown,
            configured: false,
            diagnostic: None,
            checked_at: None,
            archived: false,
        };
        snapshot
            .organization
            .connector_profiles
            .push(profile.clone());
        Ok((
            snapshot,
            Event::ConnectorProfileUpserted { profile },
            CommandResult::Created { id: id.to_string() },
        ))
    }

    pub(super) async fn reduce_update_connector_profile(
        &self,
        mut snapshot: Snapshot,
        profile_id: ConnectorProfileId,
        patch: ConnectorProfilePatch,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if let Some(name) = patch.name.as_deref()
            && !valid_name(name)
        {
            return Err(OrchestratorError::Validation(
                "connector profile name is invalid".into(),
            ));
        }
        let profile_index = snapshot
            .organization
            .connector_profiles
            .iter()
            .position(|profile| profile.id == profile_id && !profile.archived)
            .ok_or(OrchestratorError::NotFound)?;
        let current_profile = snapshot.organization.connector_profiles[profile_index].clone();
        let effective_kind = patch.kind.unwrap_or(current_profile.kind);
        let effective_config = patch.config.as_ref().unwrap_or(&current_profile.config);
        validate_profile_config_for_kind(effective_config, effective_kind)?;
        if let Some(kind) = patch.kind
            && let Some(graph) = snapshot.organization.published.as_ref()
        {
            for node in graph.nodes.iter().filter(|node| {
                node.connector_profile_id == Some(profile_id)
                    && node.kind == OrganizationNodeKind::Capability
            }) {
                if let Some(capability) = node.capability
                    && !profile_compatible(capability, kind)
                {
                    return Err(OrchestratorError::Validation(
                        "connector kind change would invalidate the published Organization".into(),
                    ));
                }
            }
        }
        let profile = &mut snapshot.organization.connector_profiles[profile_index];
        if let Some(name) = patch.name {
            profile.name = name.trim().to_owned();
        }
        if let Some(kind) = patch.kind {
            profile.kind = kind;
        }
        if let Some(config) = patch.config {
            profile.config = sanitize_profile_config(config);
        }
        profile.health = ConnectorHealth::Unknown;
        profile.diagnostic = None;
        profile.checked_at = None;
        let updated = profile.clone();
        if snapshot
            .organization
            .connector_profiles
            .iter()
            .any(|candidate| {
                candidate.id != profile_id
                    && !candidate.archived
                    && candidate.name.eq_ignore_ascii_case(&updated.name)
            })
        {
            return Err(OrchestratorError::Validation(
                "connector profile name must be unique".into(),
            ));
        }
        Ok((
            snapshot,
            Event::ConnectorProfileUpserted { profile: updated },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_archive_connector_profile(
        &self,
        mut snapshot: Snapshot,
        profile_id: ConnectorProfileId,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let referenced = graph_references_profile(&snapshot.organization.draft, profile_id)
            || snapshot
                .organization
                .published
                .as_ref()
                .is_some_and(|graph| graph_references_profile(graph, profile_id));
        if referenced {
            return Err(OrchestratorError::Validation(
                "connector profile is still referenced by Organization".into(),
            ));
        }
        let profile = snapshot
            .organization
            .connector_profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id && !profile.archived)
            .ok_or(OrchestratorError::NotFound)?;
        profile.archived = true;
        Ok((
            snapshot,
            Event::ConnectorProfileArchived { profile_id },
            CommandResult::Accepted,
        ))
    }
}
