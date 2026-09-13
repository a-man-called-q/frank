//! Task-scoped snapshot projection.
//!
//! A provider session is not a device session.  The former receives a pure
//! allowlist projection assembled from [`Snapshot::empty`], so adding a field
//! to the device snapshot cannot accidentally widen the agent boundary.

use super::*;

/// Projects the minimum durable context a worker or reviewer needs for one
/// task.  The projector intentionally returns `None` when the task's parent
/// mission or project is missing instead of returning a partial graph.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskSnapshotProjector;

impl TaskSnapshotProjector {
    pub fn project(
        snapshot: &Snapshot,
        task_id: TaskId,
        caller_agent_id: AgentId,
    ) -> Option<Snapshot> {
        let task = snapshot.tasks.iter().find(|task| task.id == task_id)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)?;
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == mission.project_id)?;

        let taskboard_id = task.taskboard_id;
        let supervisor_ids = snapshot
            .agents
            .iter()
            .filter(|agent| agent.display_name == "Frank supervisor")
            .map(|agent| agent.id)
            .collect::<Vec<_>>();

        let mut scoped = Snapshot::empty(snapshot.server_id);
        scoped.revision = snapshot.revision;
        scoped.event_seq = snapshot.event_seq;

        // Keep the related graph records, but remove host filesystem paths
        // from the projection.  The daemon resolves the task worktree from
        // durable state when a scoped command is executed.
        let mut project = project.clone();
        project.path.clear();
        project.remote = None;
        project.worktree_root.clear();
        scoped.projects.push(project);
        let mut mission = mission.clone();
        mission.supervisor_session_id = None;
        mission.branch.clear();
        mission.budget = Budget::unlimited();
        scoped.missions.push(mission);
        let mut task = task.clone();
        task.dependencies.clear();
        task.required_role_id = None;
        task.assigned_agent = task.assigned_agent.filter(|id| *id == caller_agent_id);
        task.reviewer_agent = task.reviewer_agent.filter(|id| *id == caller_agent_id);
        task.worktree = None;
        task.workflow_id = None;
        task.parent_task_id = None;
        task.child_task_ids.clear();
        task.active_role_node_id = None;
        task.organization_revision = None;
        scoped.tasks.push(task);

        scoped.agents = snapshot
            .agents
            .iter()
            .filter(|agent| agent.id == caller_agent_id || supervisor_ids.contains(&agent.id))
            .cloned()
            .map(|mut agent| {
                // Identity/presentation fields are useful to render the
                // scoped mailbox; prompt, policy, budget, and provider
                // session data are daemon-owned secrets and are not part of
                // the provider context.
                agent.role_id = None;
                agent.role_revision = 0;
                agent.model_override = None;
                agent.pending_model_override = None;
                agent.pending_model_change = false;
                agent.pack_id = None;
                agent.pack_level = None;
                agent.instructions.clear();
                agent.policy = AgentPolicy::default();
                agent.budget = Budget::unlimited();
                agent.provider_session_id = None;
                agent.last_claimed_at = None;
                agent
            })
            .collect();

        if let Some(taskboard_id) = taskboard_id {
            scoped.taskboards = snapshot
                .taskboards
                .iter()
                .filter(|board| board.id == taskboard_id)
                .cloned()
                .map(|mut board| {
                    // Project/workflow and role templates are operator/
                    // organization concerns; the board identity and routing
                    // surface are sufficient.
                    board.project_id = None;
                    board.workflow_id = None;
                    board.default_role_id = None;
                    board
                })
                .collect();
        }

        scoped.messages = snapshot
            .messages
            .iter()
            .filter(|message| message.task_id == Some(task_id))
            .cloned()
            .collect();
        scoped.approvals = snapshot
            .approvals
            .iter()
            .filter(|approval| approval.task_id == task_id)
            .cloned()
            .collect();
        scoped.artifacts = snapshot
            .artifacts
            .iter()
            .filter(|artifact| artifact.task_id == Some(task_id))
            .cloned()
            .collect();
        scoped.usage = snapshot
            .usage
            .iter()
            .filter(|usage| {
                usage.scope == BudgetScope::Task && usage.scope_id == task_id.to_string()
            })
            .cloned()
            .collect();
        scoped.terminals = snapshot
            .terminals
            .iter()
            .filter(|terminal| terminal.task_id == task_id)
            .cloned()
            .collect();
        scoped.task_feed = snapshot
            .task_feed
            .iter()
            .filter(|entry| entry.task_id == task_id)
            .cloned()
            .collect();
        scoped.work_offers = snapshot
            .work_offers
            .iter()
            .filter(|offer| offer.task_id == task_id && offer.agent_id == caller_agent_id)
            .cloned()
            .collect();
        scoped.human_inputs = snapshot
            .human_inputs
            .iter()
            .filter(|input| input.task_id == task_id)
            .cloned()
            .collect();
        scoped.review_items = snapshot
            .review_items
            .iter()
            .filter(|review| {
                review.source_task_id == task_id
                    && (review.source_agent == caller_agent_id
                        || review.reviewer_agent == caller_agent_id)
            })
            .cloned()
            .collect();

        // Every other collection remains at the empty defaults above:
        // roles, organization graph/runtime/relocations, connectors,
        // operations, uploads, updates, and global/mission/agent budgets are
        // intentionally not part of a task provider's context.
        Some(scoped)
    }
}

impl Snapshot {
    /// Return a task-scoped allowlist projection for a provider session.
    pub fn scoped_to_task(&self, task_id: TaskId, caller_agent_id: AgentId) -> Option<Self> {
        TaskSnapshotProjector::project(self, task_id, caller_agent_id)
    }

    /// Named alias for callers that prefer the projection terminology.
    pub fn project_task_scope(&self, task_id: TaskId, caller_agent_id: AgentId) -> Option<Self> {
        self.scoped_to_task(task_id, caller_agent_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        Snapshot,
        ProjectId,
        MissionId,
        TaskId,
        TaskId,
        AgentId,
        AgentId,
    ) {
        let server_id = ServerId::new();
        let project_id = ProjectId::new();
        let mission_id = MissionId::new();
        let task_id = TaskId::new();
        let sibling_task_id = TaskId::new();
        let caller = AgentId::new();
        let sibling_agent = AgentId::new();
        let mut snapshot = Snapshot::empty(server_id);
        snapshot.revision = 41;
        snapshot.event_seq = 99;
        snapshot.server.allowed_project_roots = vec!["/private/secret".into()];
        snapshot.server.worktree_root = "/private/worktrees".into();
        snapshot.server.tls_fingerprint = "sha256:secret".into();
        snapshot.projects = vec![ProjectView {
            id: project_id,
            name: "Project".into(),
            path: "/private/project".into(),
            base_branch: "main".into(),
            remote: Some("git@example.invalid:private/repo".into()),
            check_commands: vec!["cargo test".into()],
            worktree_root: "/private/project/.worktrees".into(),
            push_policy: PushPolicy::Disabled,
            pr_policy: PrPolicy::Disabled,
            archived: false,
        }];
        snapshot.missions = vec![MissionView {
            id: mission_id,
            project_id,
            objective: "do the work".into(),
            status: MissionStatus::Active,
            supervisor_session_id: Some("provider-secret".into()),
            branch: "mission/one".into(),
            budget: Budget {
                time_seconds: Some(10),
                turns: Some(2),
                measured_tokens: Some(3),
                cost_micros: Some(4),
            },
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        }];
        snapshot.tasks = vec![
            TaskView {
                id: task_id,
                mission_id,
                title: "Task".into(),
                objective: "task objective".into(),
                dependencies: Vec::new(),
                required_role_id: None,
                priority: 1,
                budget: Budget::unlimited(),
                status: TaskStatus::Ready,
                assigned_agent: Some(caller),
                reviewer_agent: None,
                claimed_at: None,
                claim_source: None,
                attempt: 1,
                max_attempts: 2,
                worktree: Some("/private/worktree/task".into()),
                branch: Some("task/one".into()),
                result_artifact: None,
                taskboard_id: None,
                workflow_id: None,
                parent_task_id: None,
                child_task_ids: Vec::new(),
                kind: WorkItemKind::Task,
                active_role_node_id: None,
                organization_revision: None,
                rework_limit: DEFAULT_REWORK_LIMIT,
                rework_count: 0,
            },
            TaskView {
                id: sibling_task_id,
                mission_id,
                title: "Sibling".into(),
                objective: "sibling secret".into(),
                dependencies: Vec::new(),
                required_role_id: None,
                priority: 1,
                budget: Budget::unlimited(),
                status: TaskStatus::Ready,
                assigned_agent: Some(sibling_agent),
                reviewer_agent: None,
                claimed_at: None,
                claim_source: None,
                attempt: 1,
                max_attempts: 2,
                worktree: Some("/private/worktree/sibling".into()),
                branch: Some("task/sibling".into()),
                result_artifact: None,
                taskboard_id: None,
                workflow_id: None,
                parent_task_id: None,
                child_task_ids: Vec::new(),
                kind: WorkItemKind::Task,
                active_role_node_id: None,
                organization_revision: None,
                rework_limit: DEFAULT_REWORK_LIMIT,
                rework_count: 0,
            },
        ];
        snapshot.agents = vec![
            AgentView {
                id: caller,
                role_id: None,
                role_revision: 1,
                display_name: "Caller".into(),
                template: AgentTemplate::Builder,
                model: Some("model".into()),
                effective_model: Some("model".into()),
                model_source: ModelSource::Agent,
                model_override: None,
                pending_model_override: None,
                pending_model_change: false,
                pack_id: None,
                pack_level: None,
                instructions: "instructions".into(),
                policy: AgentPolicy::default(),
                budget: Budget::unlimited(),
                avatar: AvatarSpec {
                    palette: "blue".into(),
                    seed: 1,
                },
                status: AgentStatus::Working,
                provider_session_id: Some("caller-provider-secret".into()),
                last_claimed_at: None,
                archived: false,
            },
            AgentView {
                id: sibling_agent,
                role_id: None,
                role_revision: 1,
                display_name: "Sibling".into(),
                template: AgentTemplate::Builder,
                model: None,
                effective_model: None,
                model_source: ModelSource::Role,
                model_override: None,
                pending_model_override: None,
                pending_model_change: false,
                pack_id: None,
                pack_level: None,
                instructions: "sibling secret".into(),
                policy: AgentPolicy::default(),
                budget: Budget::unlimited(),
                avatar: AvatarSpec {
                    palette: "red".into(),
                    seed: 2,
                },
                status: AgentStatus::Idle,
                provider_session_id: None,
                last_claimed_at: None,
                archived: false,
            },
            AgentView {
                id: AgentId::new(),
                role_id: None,
                role_revision: 1,
                display_name: "Frank supervisor".into(),
                template: AgentTemplate::Generalist,
                model: None,
                effective_model: None,
                model_source: ModelSource::Role,
                model_override: None,
                pending_model_override: None,
                pending_model_change: false,
                pack_id: None,
                pack_level: None,
                instructions: "supervisor".into(),
                policy: AgentPolicy::default(),
                budget: Budget::unlimited(),
                avatar: AvatarSpec {
                    palette: "frank".into(),
                    seed: 3,
                },
                status: AgentStatus::Idle,
                provider_session_id: None,
                last_claimed_at: None,
                archived: false,
            },
        ];
        (
            snapshot,
            project_id,
            mission_id,
            task_id,
            sibling_task_id,
            caller,
            sibling_agent,
        )
    }

    #[test]
    fn projector_is_an_allowlist_and_preserves_wire_identity() {
        let (snapshot, _, _, task_id, sibling_task_id, caller, sibling_agent) = fixture();
        let scoped = snapshot.scoped_to_task(task_id, caller).unwrap();
        assert_eq!(scoped.server_id, snapshot.server_id);
        assert_eq!(scoped.revision, snapshot.revision);
        assert_eq!(scoped.event_seq, snapshot.event_seq);
        assert_eq!(scoped.tasks.len(), 1);
        assert_eq!(scoped.tasks[0].id, task_id);
        assert_ne!(scoped.tasks[0].id, sibling_task_id);
        assert!(scoped.agents.iter().any(|agent| agent.id == caller));
        assert!(!scoped.agents.iter().any(|agent| agent.id == sibling_agent));
        assert!(scoped.roles.is_empty());
        assert!(scoped.organization.connector_profiles.is_empty());
        assert!(scoped.operations.is_empty());
        assert!(scoped.uploads.is_empty());
        assert!(scoped.update.is_none());
        assert!(scoped.organization_relocations.is_empty());
        assert!(scoped.server.allowed_project_roots.is_empty());
        assert!(scoped.server.worktree_root.is_empty());
        assert!(scoped.server.tls_fingerprint.is_empty());
        assert_eq!(scoped.projects[0].path, "");
        assert!(scoped.projects[0].remote.is_none());
        assert!(scoped.missions[0].supervisor_session_id.is_none());
        assert!(scoped.missions[0].branch.is_empty());
        assert_eq!(scoped.missions[0].budget, Budget::unlimited());
        assert!(scoped.tasks[0].dependencies.is_empty());
        assert!(scoped.tasks[0].parent_task_id.is_none());
        assert!(scoped.tasks[0].child_task_ids.is_empty());
        assert_eq!(scoped.tasks[0].worktree, None);
        assert!(
            scoped
                .agents
                .iter()
                .all(|agent| agent.instructions.is_empty()
                    && agent.provider_session_id.is_none()
                    && agent.budget == Budget::unlimited())
        );
    }

    #[test]
    fn mixed_task_feed_and_future_fields_cannot_cross_the_boundary() {
        let (mut snapshot, _, _, task_id, sibling_task_id, caller, _) = fixture();
        snapshot.task_feed = vec![
            TaskFeedEntry {
                id: TaskFeedId::new(),
                task_id,
                actor: ActorRef {
                    kind: ActorKind::Agent,
                    id: Some(caller.to_string()),
                    display_name: Some("Caller".into()),
                },
                kind: TaskFeedKind::Comment,
                body: "allowed".into(),
                artifact_ids: Vec::new(),
                created_at: timestamp_now(),
            },
            TaskFeedEntry {
                id: TaskFeedId::new(),
                task_id: sibling_task_id,
                actor: ActorRef {
                    kind: ActorKind::Agent,
                    id: Some(caller.to_string()),
                    display_name: Some("Caller".into()),
                },
                kind: TaskFeedKind::Comment,
                body: "sibling secret".into(),
                artifact_ids: Vec::new(),
                created_at: timestamp_now(),
            },
        ];
        let mut wire = serde_json::to_value(&snapshot).unwrap();
        wire["future_sensitive"] = serde_json::json!({"token": "must not echo"});
        let decoded: Snapshot = serde_json::from_value(wire).unwrap();
        let scoped = decoded.scoped_to_task(task_id, caller).unwrap();
        assert_eq!(scoped.task_feed.len(), 1);
        assert_eq!(scoped.task_feed[0].body, "allowed");
        let output = serde_json::to_value(scoped).unwrap();
        assert!(output.get("future_sensitive").is_none());
        assert!(!output.to_string().contains("sibling secret"));
        assert!(!output.to_string().contains("must not echo"));
    }

    #[test]
    fn review_and_offer_projection_is_tied_to_the_caller() {
        let (mut snapshot, _, mission_id, task_id, _, caller, sibling_agent) = fixture();
        snapshot.work_offers = vec![
            WorkOfferView {
                id: WorkOfferId::new(),
                task_id,
                taskboard_id: TaskboardId::new(),
                agent_id: caller,
                role_id: None,
                status: WorkOfferStatus::Pending,
                attempt: 1,
                created_at: timestamp_now(),
                expires_at: timestamp_now(),
                responded_at: None,
            },
            WorkOfferView {
                id: WorkOfferId::new(),
                task_id,
                taskboard_id: TaskboardId::new(),
                agent_id: sibling_agent,
                role_id: None,
                status: WorkOfferStatus::Pending,
                attempt: 1,
                created_at: timestamp_now(),
                expires_at: timestamp_now(),
                responded_at: None,
            },
        ];
        snapshot.review_items = vec![
            ReviewWorkItemView {
                id: ReviewWorkItemId::new(),
                mission_id,
                source_task_id: task_id,
                source_agent: caller,
                reviewer_agent: sibling_agent,
                relation_id: "review-1".into(),
                contract: OrganizationHandoffContract::default(),
                status: ReviewWorkItemStatus::Pending,
                decision_reason: None,
                created_at: timestamp_now(),
                updated_at: timestamp_now(),
            },
            ReviewWorkItemView {
                id: ReviewWorkItemId::new(),
                mission_id,
                source_task_id: task_id,
                source_agent: sibling_agent,
                reviewer_agent: sibling_agent,
                relation_id: "review-2".into(),
                contract: OrganizationHandoffContract::default(),
                status: ReviewWorkItemStatus::Pending,
                decision_reason: None,
                created_at: timestamp_now(),
                updated_at: timestamp_now(),
            },
        ];
        let scoped = snapshot.scoped_to_task(task_id, caller).unwrap();
        assert_eq!(scoped.work_offers.len(), 1);
        assert_eq!(scoped.work_offers[0].agent_id, caller);
        assert_eq!(scoped.review_items.len(), 1);
        assert_eq!(scoped.review_items[0].source_agent, caller);
    }
}
