//! Concurrency limits and the per-provider lease the scheduler hands out.

use std::collections::HashMap;

use frank_protocol::*;

use crate::*;

#[derive(Debug, Clone)]
pub struct SchedulerLimits {
    pub max_concurrency: usize,
    pub max_provider_concurrency: usize,
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_provider_concurrency: DEFAULT_MAX_PROVIDER_CONCURRENCY,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    pub limits: SchedulerLimits,
    // Crate-visible rather than private: the reducer tests in lib.rs build a
    // Scheduler in a specific running state directly, which a child module's
    // private fields would no longer allow.
    pub(crate) running: HashSet<TaskId>,
    pub(crate) running_by_provider: HashMap<Provider, usize>,
    pub(crate) providers_by_task: HashMap<TaskId, Provider>,
}

impl Scheduler {
    pub fn can_start(&self, provider: Provider) -> bool {
        self.running.len() < self.limits.max_concurrency
            && self
                .running_by_provider
                .get(&provider)
                .copied()
                .unwrap_or(0)
                < self.limits.max_provider_concurrency
    }

    pub fn start(&mut self, task: TaskId, provider: Provider) -> bool {
        if !self.can_start(provider) || !self.running.insert(task) {
            return false;
        }
        *self.running_by_provider.entry(provider).or_default() += 1;
        self.providers_by_task.insert(task, provider);
        true
    }

    pub fn finish(&mut self, task: TaskId, provider: Provider) {
        if self.running.remove(&task) {
            let provider = self.providers_by_task.remove(&task).unwrap_or(provider);
            if let Some(count) = self.running_by_provider.get_mut(&provider) {
                *count = count.saturating_sub(1);
            }
        }
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }
}
