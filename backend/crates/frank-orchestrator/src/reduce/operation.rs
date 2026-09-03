//! Cancelling and retrying long-running operations.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_operation(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CancelOperation { operation_id } => {
                let index = snapshot
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
                    .map(|operation| {
                        snapshot
                            .operations
                            .iter()
                            .position(|candidate| candidate.id == operation.id)
                            .unwrap_or_default()
                    })
                    .ok_or(OrchestratorError::NotFound)?;
                let mut operation = snapshot.operations[index].clone();
                if matches!(
                    operation.status,
                    OperationStatus::Succeeded | OperationStatus::Cancelled
                ) {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation.clone()),
                    ));
                }
                operation.status = OperationStatus::Cancelled;
                operation.updated_at = timestamp_now();
                operation.error = Some("cancelled by actor".into());
                snapshot.operations[index] = operation.clone();
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation.clone()),
                ))
            }
            Command::RetryOperation { operation_id } => {
                let index = snapshot
                    .operations
                    .iter()
                    .position(|operation| operation.id == operation_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let operation = &mut snapshot.operations[index];
                if operation.status != OperationStatus::Failed
                    && !(operation.status == OperationStatus::Waiting
                        && operation.phase == "awaiting-helper")
                {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a failed or helper-waiting operation can be retried".into(),
                    ));
                }
                operation.status = OperationStatus::Queued;
                operation.phase = "retry-queued".into();
                operation.error = None;
                operation.updated_at = timestamp_now();
                let operation = operation.clone();
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            _ => super::misrouted(),
        }
    }
}
