//! Server settings, and the pairing command the reducer refuses.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_settings(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::Pair(_) => Err(OrchestratorError::Forbidden),
            Command::UpdateSettings { patch } => {
                if let Some(Some(model)) = &patch.supervisor_model
                    && model.trim().is_empty()
                {
                    return Err(OrchestratorError::Validation(
                        "supervisor model cannot be empty".into(),
                    ));
                }
                if !patch.clear_supervisor_model
                    && let Some(Some(model)) = &patch.supervisor_model
                {
                    self.validate_openrouter_model(Some(model)).await?;
                }
                apply_settings_patch(&mut snapshot.server, patch)?;
                let settings = snapshot.server.clone();
                Ok((
                    snapshot,
                    Event::SettingsChanged { settings },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}
