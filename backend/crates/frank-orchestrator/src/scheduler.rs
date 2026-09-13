//! Concurrency limits and task leases owned by the orchestrator.

use frank_protocol::*;

use crate::*;

#[derive(Debug, Clone)]
pub struct SchedulerLimits {
    pub max_concurrency: usize,
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
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
}

impl Scheduler {
    pub fn can_start(&self) -> bool {
        self.running.len() < self.limits.max_concurrency
    }

    pub fn start(&mut self, task: TaskId) -> bool {
        if !self.can_start() || !self.running.insert(task) {
            return false;
        }
        true
    }

    pub fn finish(&mut self, task: TaskId) {
        self.running.remove(&task);
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }
}
