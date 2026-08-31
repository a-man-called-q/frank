//! Supervisor planning policy.
//!
//! A real Codex/Claude supervisor can supply a richer DAG through the same
//! commands, but the daemon always validates the result.  The deterministic
//! fallback here keeps missions useful in offline/fake-provider tests and
//! makes it impossible for a malformed provider response to bypass state
//! transitions.

use std::collections::{HashMap, VecDeque};

use frank_protocol::{
    AgentId, Budget, MissionId, Provider, SupervisorPlanProposal, TaskId, TaskSpec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorPolicy {
    pub provider: Option<Provider>,
    pub max_tasks: usize,
    pub auto_assign: bool,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            provider: None,
            max_tasks: 32,
            auto_assign: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorPlan {
    pub mission_id: MissionId,
    pub tasks: Vec<TaskSpec>,
    /// Provider-facing task keys in the same order as `tasks`.
    pub task_keys: Vec<String>,
    /// Provider-facing dependency keys in the same order as `tasks`.
    pub dependency_keys: Vec<Vec<String>>,
    pub notes: String,
}

/// Convert a provider-produced proposal into the daemon's internal task
/// order. Provider task dependencies use stable client keys because a
/// provider cannot know Frank's generated UUIDs. The conversion performs a
/// topological sort and refuses cycles before any task is persisted.
pub fn proposal_to_plan(proposal: SupervisorPlanProposal) -> Result<SupervisorPlan, String> {
    let mut by_key = HashMap::new();
    let mut input_keys = Vec::with_capacity(proposal.tasks.len());
    for task in proposal.tasks {
        if task.client_key.trim().is_empty() {
            return Err("supervisor proposal contains an empty task key".into());
        }
        let key = task.client_key.clone();
        if by_key.insert(key.clone(), task).is_some() {
            return Err("supervisor proposal contains duplicate task keys".into());
        }
        input_keys.push(key);
    }
    // Validate references before running Kahn's algorithm.  Otherwise an
    // unknown dependency is indistinguishable from a cycle because its
    // indegree can never reach zero, which makes provider diagnostics vague
    // and can leave a caller retrying a permanently invalid proposal.
    for task in by_key.values() {
        if task
            .dependencies
            .iter()
            .any(|dependency| dependency == &task.client_key || !by_key.contains_key(dependency))
        {
            return Err("supervisor proposal contains an unknown or self dependency".into());
        }
    }
    let mut indegree = by_key
        .iter()
        .map(|(key, task)| (key.clone(), task.dependencies.len()))
        .collect::<HashMap<_, _>>();
    // HashMap iteration is intentionally randomized.  Use the provider's
    // input order as a stable tie-breaker so the same proposal produces the
    // same task/event order across daemon restarts and fake-provider runs.
    let mut ready = input_keys
        .iter()
        .filter(|key| by_key[*key].dependencies.is_empty())
        .cloned()
        .collect::<VecDeque<_>>();
    let mut ordered = Vec::with_capacity(by_key.len());
    while let Some(key) = ready.pop_front() {
        ordered.push(key.clone());
        for dependent_key in &input_keys {
            let dependent = &by_key[dependent_key];
            if dependent
                .dependencies
                .iter()
                .any(|dependency| dependency == &key)
            {
                let Some(value) = indegree.get_mut(dependent_key) else {
                    return Err("supervisor proposal contains an unknown dependency".into());
                };
                *value = value.saturating_sub(1);
                if *value == 0 {
                    ready.push_back(dependent_key.clone());
                }
            }
        }
    }
    if ordered.len() != by_key.len() {
        return Err("supervisor proposal dependencies contain a cycle".into());
    }
    let mut tasks = Vec::with_capacity(ordered.len());
    let mut task_keys = Vec::with_capacity(ordered.len());
    let mut dependency_keys = Vec::with_capacity(ordered.len());
    for key in ordered {
        let task = by_key
            .get(&key)
            .ok_or_else(|| "supervisor proposal task disappeared".to_string())?;
        tasks.push(TaskSpec {
            mission_id: proposal.mission_id,
            title: task.title.clone(),
            objective: task.objective.clone(),
            dependencies: Vec::new(),
            priority: task.priority,
            assigned_agent: task
                .assigned_agent
                .or_else(|| task.candidate_agents.first().copied()),
            budget: task.budget.clone(),
        });
        task_keys.push(key);
        dependency_keys.push(task.dependencies.clone());
    }
    Ok(SupervisorPlan {
        mission_id: proposal.mission_id,
        tasks,
        task_keys,
        dependency_keys,
        notes: proposal.reasoning.unwrap_or(proposal.summary),
    })
}

pub fn decompose_objective(
    mission_id: MissionId,
    objective: &str,
    max_tasks: usize,
) -> SupervisorPlan {
    let mut tasks = objective
        .lines()
        .flat_map(|line| line.split([';', '.']))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .take(max_tasks.max(1))
        .enumerate()
        .map(|(index, part)| TaskSpec {
            mission_id,
            title: format!(
                "{}: {}",
                index + 1,
                part.chars().take(72).collect::<String>()
            ),
            objective: part.to_string(),
            dependencies: Vec::new(),
            priority: (max_tasks.saturating_sub(index)) as i32,
            assigned_agent: None,
            budget: Budget::unlimited(),
        })
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        tasks.push(TaskSpec {
            mission_id,
            title: "Mission objective".into(),
            objective: objective.trim().to_string(),
            dependencies: Vec::new(),
            priority: 0,
            assigned_agent: None,
            budget: Budget::unlimited(),
        });
    }
    let task_count = tasks.len();
    SupervisorPlan {
        mission_id,
        tasks,
        task_keys: (0..task_count)
            .map(|index| format!("task-{index}"))
            .collect(),
        dependency_keys: vec![Vec::new(); task_count],
        notes: "Generated by the daemon-safe supervisor planner; provider plans are validated by the same DAG rules.".into(),
    }
}

pub fn assign_ready_tasks(
    tasks: &mut [frank_protocol::TaskView],
    agents: &[frank_protocol::AgentView],
) -> Vec<(TaskId, AgentId)> {
    let mut assignments = Vec::new();
    for (agent_index, task) in tasks
        .iter_mut()
        .filter(|task| {
            task.status == frank_protocol::TaskStatus::Ready && task.assigned_agent.is_none()
        })
        .enumerate()
    {
        let Some(agent) = agents
            .iter()
            .filter(|agent| !agent.archived)
            .nth(agent_index)
        else {
            break;
        };
        task.assigned_agent = Some(agent.id);
        assignments.push((task.id, agent.id));
    }
    assignments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_decomposition_is_deterministic_and_bounded() {
        let plan = decompose_objective(MissionId::nil(), "research; build; test; ship", 2);
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].dependencies, Vec::<TaskId>::new());
    }

    #[test]
    fn proposal_order_is_stable_and_unknown_dependencies_are_rejected() {
        let proposal = SupervisorPlanProposal {
            mission_id: MissionId::nil(),
            summary: "stable DAG".into(),
            reasoning: None,
            tasks: vec![
                frank_protocol::SupervisorTaskProposal {
                    client_key: "second".into(),
                    title: "second".into(),
                    objective: "second".into(),
                    dependencies: vec!["first".into()],
                    priority: 0,
                    candidate_agents: Vec::new(),
                    assigned_agent: None,
                    policy_requirement: None,
                    budget: Budget::unlimited(),
                },
                frank_protocol::SupervisorTaskProposal {
                    client_key: "first".into(),
                    title: "first".into(),
                    objective: "first".into(),
                    dependencies: Vec::new(),
                    priority: 0,
                    candidate_agents: Vec::new(),
                    assigned_agent: None,
                    policy_requirement: None,
                    budget: Budget::unlimited(),
                },
            ],
        };
        let plan = proposal_to_plan(proposal).unwrap();
        assert_eq!(plan.task_keys, vec!["first", "second"]);

        let invalid = SupervisorPlanProposal {
            mission_id: MissionId::nil(),
            summary: "invalid".into(),
            reasoning: None,
            tasks: vec![frank_protocol::SupervisorTaskProposal {
                client_key: "task".into(),
                title: "task".into(),
                objective: "task".into(),
                dependencies: vec!["missing".into()],
                priority: 0,
                candidate_agents: Vec::new(),
                assigned_agent: None,
                policy_requirement: None,
                budget: Budget::unlimited(),
            }],
        };
        assert!(
            proposal_to_plan(invalid)
                .unwrap_err()
                .contains("unknown or self dependency")
        );
    }
}
