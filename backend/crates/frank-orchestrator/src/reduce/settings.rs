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
                if let Some(Some(provider)) = patch.supervisor_provider {
                    let probe = self
                        .runtime
                        .doctor()
                        .await
                        .into_iter()
                        .find(|probe| probe.capability.provider == provider);
                    if !probe.as_ref().is_some_and(|probe| {
                        probe.capability.available && probe.capability.logged_in
                    }) {
                        let detail = probe
                            .and_then(|probe| probe.capability.diagnostic)
                            .unwrap_or_else(|| format!("{provider} is not available"));
                        return Err(OrchestratorError::ProviderUnavailable(detail));
                    }
                }
                apply_settings_patch(&mut snapshot.server, patch)?;
                let settings = snapshot.server.clone();
                Ok((
                    snapshot,
                    Event::SettingsChanged { settings },
                    CommandResult::Accepted,
                ))
            }
            _ => Err(OrchestratorError::Validation(
                "command was routed to the wrong reducer".into(),
            )),
        }
    }
}
