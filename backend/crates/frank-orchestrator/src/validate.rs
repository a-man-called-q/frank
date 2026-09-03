//! Authorization, spec validation and patch application.
//!
//! Everything here is a pure function over a snapshot or a DTO: no IO, no
//! provider calls, no store access. That is what makes these the cheapest
//! things in the crate to test.

use std::collections::{HashMap, HashSet};

use frank_protocol::*;

use crate::*;

/// Return the next retry state after a failed attempt.  The default policy
/// permits two attempts total; once exhausted the task is blocked and never
/// loops indefinitely.
pub fn retry_after_failure(task: &mut TaskView) -> TaskStatus {
    task.attempt = task.attempt.saturating_add(1);
    if task.attempt < task.max_attempts {
        TaskStatus::Ready
    } else {
        TaskStatus::Blocked
    }
}

pub(crate) fn authorize(command: &Command, actor_kind: ActorKind, role: DeviceRole) -> Result<()> {
    if matches!(command, Command::Pair(_)) {
        return Ok(());
    }
    if actor_kind == ActorKind::System {
        return Ok(());
    }
    if actor_kind == ActorKind::Agent
        && !matches!(
            command,
            Command::UpdateTask { .. }
                | Command::CreateTask(_)
                | Command::SetTaskStatus { .. }
                | Command::SendMessage(_)
                | Command::AckMessage { .. }
                | Command::CompleteMessage { .. }
                | Command::RequestApproval(_)
                | Command::ProposeMemory { .. }
                | Command::ReadMemory { .. }
                | Command::PublishArtifact(_)
                | Command::BeginArtifactUpload(_)
                | Command::FinalizeArtifactUpload { .. }
        )
    {
        return Err(OrchestratorError::Forbidden);
    }
    if !role.can_mutate() {
        return Err(OrchestratorError::Forbidden);
    }
    let owner_only = matches!(
        command,
        Command::UpdateSettings { .. }
            | Command::CreateProject(_)
            | Command::CloneProject { .. }
            | Command::ArchiveProject { .. }
            | Command::CreateAgent(_)
            | Command::UpdateAgent { .. }
            | Command::ArchiveAgent { .. }
            | Command::PrepareUpdate { .. }
            | Command::ApplyUpdate { .. }
            | Command::RollbackUpdate
    );
    if owner_only && !role.can_admin() {
        return Err(OrchestratorError::Forbidden);
    }
    Ok(())
}

pub(crate) fn validate_supervisor_proposal(
    snapshot: &Snapshot,
    mission_id: MissionId,
    proposal: &SupervisorPlanProposal,
) -> Result<()> {
    if proposal.mission_id != mission_id {
        return Err(OrchestratorError::Validation(
            "supervisor proposal mission does not match the command".into(),
        ));
    }
    if proposal.tasks.is_empty() || proposal.tasks.len() > 32 {
        return Err(OrchestratorError::Validation(
            "supervisor proposal must contain between one and 32 tasks".into(),
        ));
    }
    if !snapshot
        .missions
        .iter()
        .any(|mission| mission.id == mission_id)
    {
        return Err(OrchestratorError::NotFound);
    }
    let mut keys = HashSet::new();
    for task in &proposal.tasks {
        if task.client_key.trim().is_empty()
            || task.client_key.len() > 128
            || !keys.insert(task.client_key.clone())
            || task.title.trim().is_empty()
            || task.title.len() > 256
            || task.objective.trim().is_empty()
            || task.objective.len() > MAX_MESSAGE_BODY_BYTES
        {
            return Err(OrchestratorError::Validation(
                "supervisor proposal contains an invalid or duplicate task key".into(),
            ));
        }
        if let Some(agent_id) = task.assigned_agent
            && !snapshot
                .agents
                .iter()
                .any(|agent| agent.id == agent_id && !agent.archived)
        {
            return Err(OrchestratorError::NotFound);
        }
        for agent_id in &task.candidate_agents {
            if !snapshot
                .agents
                .iter()
                .any(|agent| agent.id == *agent_id && !agent.archived)
            {
                return Err(OrchestratorError::NotFound);
            }
        }
        if let (Some(assigned), Some(requirement)) =
            (task.assigned_agent, task.policy_requirement.as_ref())
        {
            let profile = snapshot
                .agents
                .iter()
                .find(|agent| agent.id == assigned && !agent.archived)
                .ok_or(OrchestratorError::NotFound)?;
            if !policy_satisfies(&profile.policy, requirement) {
                return Err(OrchestratorError::Validation(
                    "assigned agent policy cannot satisfy supervisor task requirement".into(),
                ));
            }
        }
        if let Some(requirement) = task.policy_requirement.as_ref() {
            for agent_id in &task.candidate_agents {
                if let Some(profile) = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.id == *agent_id && !agent.archived)
                    && !policy_satisfies(&profile.policy, requirement)
                {
                    return Err(OrchestratorError::Validation(
                        "supervisor candidate policy cannot satisfy task requirement".into(),
                    ));
                }
            }
        }
    }
    for task in &proposal.tasks {
        if task
            .dependencies
            .iter()
            .any(|dependency| dependency == &task.client_key || !keys.contains(dependency))
        {
            return Err(OrchestratorError::Validation(
                "supervisor proposal contains an unknown or self dependency".into(),
            ));
        }
    }
    // Kahn's algorithm rejects cycles before a provider proposal can reach
    // the task projection.  The actual TaskId graph is checked again by the
    // normal CreateTask reducer when the proposal is materialized.
    let mut indegree = proposal
        .tasks
        .iter()
        .map(|task| (task.client_key.as_str(), task.dependencies.len()))
        .collect::<HashMap<_, _>>();
    let mut remaining = proposal
        .tasks
        .iter()
        .filter(|task| task.dependencies.is_empty())
        .map(|task| task.client_key.as_str())
        .collect::<Vec<_>>();
    let mut visited = 0usize;
    while let Some(key) = remaining.pop() {
        visited += 1;
        for dependent in proposal
            .tasks
            .iter()
            .filter(|task| task.dependencies.iter().any(|dependency| dependency == key))
        {
            if let Some(value) = indegree.get_mut(dependent.client_key.as_str()) {
                *value = value.saturating_sub(1);
                if *value == 0 {
                    remaining.push(dependent.client_key.as_str());
                }
            }
        }
    }
    if visited != proposal.tasks.len() {
        return Err(OrchestratorError::Validation(
            "supervisor proposal dependencies contain a cycle".into(),
        ));
    }
    Ok(())
}

/// Return whether a profile can provide at least the capabilities requested
/// by a supervisor proposal. Policy values are deliberately compared without
/// relying on enum ordering so adding a new value cannot silently widen an
/// existing task's permissions.
pub(crate) fn policy_satisfies(profile: &AgentPolicy, requirement: &AgentPolicy) -> bool {
    let filesystem_ok = match requirement.filesystem {
        FilesystemPolicy::ReadOnly => true,
        FilesystemPolicy::WorkspaceWrite => profile.filesystem == FilesystemPolicy::WorkspaceWrite,
    };
    let shell_ok = match requirement.shell {
        ShellPolicy::Deny => profile.shell == ShellPolicy::Deny,
        ShellPolicy::Ask => !matches!(profile.shell, ShellPolicy::Deny),
        ShellPolicy::Allow => profile.shell == ShellPolicy::Allow,
    };
    let network_ok = match requirement.network {
        NetworkPolicy::Deny => profile.network == NetworkPolicy::Deny,
        NetworkPolicy::Ask => !matches!(profile.network, NetworkPolicy::Deny),
        NetworkPolicy::Allow => profile.network == NetworkPolicy::Allow,
    };
    let approval_ok = match requirement.approval {
        ApprovalPolicy::Never => profile.approval == ApprovalPolicy::Never,
        ApprovalPolicy::Ask => true,
    };
    filesystem_ok && shell_ok && network_ok && approval_ok
}

pub(crate) fn parse_supervisor_json(
    text: &str,
    mission_id: MissionId,
) -> Option<SupervisorPlanProposal> {
    let trimmed = text.trim();
    let candidate = trimmed
        .strip_prefix("```")
        .and_then(|value| value.find('{').map(|index| &value[index..]))
        .and_then(|value| value.rsplit_once("```").map(|(json, _)| json.trim()))
        .unwrap_or(trimmed);
    let start = candidate.find('{')?;
    let end = candidate.rfind('}')?;
    let proposal = serde_json::from_str::<SupervisorPlanProposal>(&candidate[start..=end]).ok()?;
    (proposal.mission_id == mission_id).then_some(proposal)
}

pub(crate) fn apply_settings_patch(
    settings: &mut ServerSettings,
    patch: SettingsPatch,
) -> Result<()> {
    if let Some(name) = patch.name {
        if name.trim().is_empty() || name.len() > 128 {
            return Err(OrchestratorError::Validation(
                "server name is invalid".into(),
            ));
        }
        settings.name = name;
    }
    if let Some(bind) = patch.bind_address {
        if bind.trim().is_empty() {
            return Err(OrchestratorError::Validation(
                "bind address is empty".into(),
            ));
        }
        if bind.len() > 256 || bind.chars().any(|character| character.is_control()) {
            return Err(OrchestratorError::Validation(
                "bind address is invalid".into(),
            ));
        }
        if let Ok(address) = bind.parse::<std::net::IpAddr>()
            && !address.is_loopback()
            && settings.tls_fingerprint.trim().is_empty()
        {
            return Err(OrchestratorError::Validation(
                "non-loopback binds require a configured TLS identity".into(),
            ));
        }
        settings.bind_address = bind;
    }
    if let Some(port) = patch.port {
        if port == 0 {
            return Err(OrchestratorError::Validation(
                "port must be non-zero".into(),
            ));
        }
        settings.port = port;
    }
    if let Some(roots) = patch.allowed_project_roots {
        if roots.len() > 64
            || roots.iter().any(|root| {
                root.trim().is_empty()
                    || root.len() > 4_096
                    || root.chars().any(|character| character.is_control())
            })
        {
            return Err(OrchestratorError::Validation(
                "allowed project roots are invalid".into(),
            ));
        }
        settings.allowed_project_roots = roots;
    }
    if let Some(root) = patch.worktree_root {
        if root.len() > 4_096 || root.chars().any(|character| character.is_control()) {
            return Err(OrchestratorError::Validation(
                "worktree root is invalid".into(),
            ));
        }
        settings.worktree_root = root;
    }
    if let Some(value) = patch.max_concurrency {
        settings.max_concurrency = value.clamp(1, 64);
    }
    if let Some(value) = patch.max_provider_concurrency {
        settings.max_provider_concurrency = value.clamp(1, 32);
    }
    if let Some(budget) = patch.default_budget {
        settings.default_budget = budget;
    }
    if let Some(value) = patch.approval_ttl_seconds {
        settings.approval_ttl_seconds = value.clamp(30, 86_400);
    }
    if let Some(value) = patch.event_retention_days {
        settings.event_retention_days = value.clamp(1, 3_650);
    }
    if let Some(value) = patch.terminal_retention_days {
        settings.terminal_retention_days = value.clamp(1, 3_650);
    }
    if let Some(value) = patch.artifact_retention_days {
        settings.artifact_retention_days = value.clamp(1, 3_650);
    }
    if let Some(provider) = patch.supervisor_provider {
        settings.supervisor_provider = provider;
    }
    Ok(())
}

pub(crate) fn validate_project_spec(spec: &ProjectSpec, settings: &ServerSettings) -> Result<()> {
    if !valid_agent_text(&spec.name, 128)
        || !valid_agent_text(&spec.base_branch, 256)
        || spec.remote.as_deref().is_some_and(|remote| {
            remote.trim().is_empty()
                || remote.len() > 256
                || remote.chars().any(|character| character.is_control())
        })
    {
        return Err(OrchestratorError::Validation(
            "project name and base branch are required".into(),
        ));
    }
    if spec.path.is_some() == spec.clone_url.is_some() {
        return Err(OrchestratorError::Validation(
            "provide exactly one local path or clone URL".into(),
        ));
    }
    if let Some(path) = &spec.path
        && !is_allowed_path(path, &settings.allowed_project_roots)
    {
        return Err(OrchestratorError::Validation(
            "project path is outside an allowed root".into(),
        ));
    }
    if spec.check_commands.len() > 64
        || spec
            .check_commands
            .iter()
            .any(|command| command.len() > 4096 || command.contains('\n') || command.contains('\0'))
    {
        return Err(OrchestratorError::Validation(
            "check command is invalid".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_task_spec(spec: &TaskSpec, tasks: &[TaskView]) -> Result<()> {
    if !valid_agent_text(&spec.title, 256)
        || !valid_content_text(&spec.objective, MAX_MESSAGE_BODY_BYTES)
    {
        return Err(OrchestratorError::Validation(
            "task title and objective are required".into(),
        ));
    }
    if spec.dependencies.len() > 256 {
        return Err(OrchestratorError::Validation(
            "task has too many dependencies".into(),
        ));
    }
    let mut unique_dependencies = HashSet::with_capacity(spec.dependencies.len());
    if spec
        .dependencies
        .iter()
        .any(|dependency| !unique_dependencies.insert(*dependency))
    {
        return Err(OrchestratorError::Validation(
            "task dependencies must be unique".into(),
        ));
    }
    if spec.dependencies.iter().any(|dependency| {
        !tasks
            .iter()
            .any(|task| task.id == *dependency && task.mission_id == spec.mission_id)
    }) {
        return Err(OrchestratorError::Validation(
            "task dependency does not exist in this mission".into(),
        ));
    }
    if spec
        .dependencies
        .iter()
        .any(|dependency| *dependency == TaskId::nil())
    {
        return Err(OrchestratorError::Validation(
            "task dependency id is invalid".into(),
        ));
    }
    let candidate = TaskView {
        id: TaskId::nil(),
        mission_id: spec.mission_id,
        title: spec.title.clone(),
        objective: spec.objective.clone(),
        dependencies: spec.dependencies.clone(),
        priority: spec.priority,
        budget: spec.budget.clone(),
        status: TaskStatus::Backlog,
        assigned_agent: spec.assigned_agent,
        attempt: 0,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        worktree: None,
        branch: None,
        result_artifact: None,
    };
    validate_task_view(&candidate, tasks)
}

pub(crate) fn validate_task_view(candidate: &TaskView, tasks: &[TaskView]) -> Result<()> {
    if !valid_agent_text(&candidate.title, 256)
        || !valid_content_text(&candidate.objective, MAX_MESSAGE_BODY_BYTES)
        || candidate.dependencies.len() > 256
    {
        return Err(OrchestratorError::Validation(
            "task title, objective, or dependencies are invalid".into(),
        ));
    }
    let mut unique_dependencies = HashSet::with_capacity(candidate.dependencies.len());
    if candidate
        .dependencies
        .iter()
        .any(|dependency| !unique_dependencies.insert(*dependency))
    {
        return Err(OrchestratorError::Validation(
            "task dependencies must be unique".into(),
        ));
    }
    if candidate.dependencies.contains(&candidate.id) {
        return Err(OrchestratorError::Validation(
            "task cannot depend on itself".into(),
        ));
    }
    if candidate
        .dependencies
        .iter()
        .any(|dependency| !tasks.iter().any(|task| task.id == *dependency))
    {
        return Err(OrchestratorError::Validation(
            "task dependency does not exist".into(),
        ));
    }
    if candidate.dependencies.iter().any(|dependency| {
        tasks
            .iter()
            .any(|task| task.id == *dependency && task.mission_id != candidate.mission_id)
    }) {
        return Err(OrchestratorError::Validation(
            "task dependency crosses mission boundary".into(),
        ));
    }
    let mut graph: HashMap<TaskId, Vec<TaskId>> = tasks
        .iter()
        .map(|task| (task.id, task.dependencies.clone()))
        .collect();
    graph.insert(candidate.id, candidate.dependencies.clone());
    for id in graph.keys().copied() {
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        if dfs_cycle(id, &graph, &mut visiting, &mut visited) {
            return Err(OrchestratorError::Validation(
                "task dependencies must form a DAG".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn dfs_cycle(
    id: TaskId,
    graph: &HashMap<TaskId, Vec<TaskId>>,
    visiting: &mut HashSet<TaskId>,
    visited: &mut HashSet<TaskId>,
) -> bool {
    if visiting.contains(&id) {
        return true;
    }
    if !visited.insert(id) {
        return false;
    }
    visiting.insert(id);
    let cycle = graph
        .get(&id)
        .map(|deps| {
            deps.iter()
                .any(|dependency| dfs_cycle(*dependency, graph, visiting, visited))
        })
        .unwrap_or(false);
    visiting.remove(&id);
    cycle
}

pub(crate) fn apply_agent_patch(agent: &mut AgentView, patch: AgentPatch) {
    if let Some(value) = patch.display_name {
        agent.display_name = value;
    }
    if let Some(value) = patch.model {
        agent.model = value;
    }
    if let Some(value) = patch.pack_id {
        agent.pack_id = value;
    }
    if let Some(value) = patch.pack_level {
        agent.pack_level = value;
    }
    if let Some(value) = patch.instructions {
        agent.instructions = value;
    }
    if let Some(value) = patch.policy {
        agent.policy = value;
    }
    if let Some(value) = patch.budget {
        agent.budget = value;
    }
    if let Some(value) = patch.avatar {
        agent.avatar = value;
    }
}

pub(crate) fn valid_agent_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(|character| character.is_control())
}

pub(crate) fn valid_content_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.contains('\0')
}

pub(crate) fn valid_optional_agent_text(value: Option<&str>, max_bytes: usize) -> bool {
    value.is_none_or(|value| {
        value.len() <= max_bytes && !value.chars().any(|character| character.is_control())
    })
}

pub(crate) fn apply_task_patch(task: &mut TaskView, patch: TaskPatch) {
    if let Some(value) = patch.title {
        task.title = value;
    }
    if let Some(value) = patch.objective {
        task.objective = value;
    }
    if let Some(value) = patch.dependencies {
        task.dependencies = value;
    }
    if let Some(value) = patch.priority {
        task.priority = value;
    }
    if let Some(value) = patch.assigned_agent {
        task.assigned_agent = value;
    }
    if let Some(value) = patch.budget {
        task.budget = value;
    }
}
