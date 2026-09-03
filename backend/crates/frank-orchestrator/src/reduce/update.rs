//! The self-update command surface.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_update(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CheckForUpdate => {
                let mut update = snapshot.update.clone().unwrap_or(UpdateView {
                    id: UpdateId::new(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    state: UpdateState::Idle,
                    target: String::new(),
                    size: 0,
                    sha256: String::new(),
                    error: None,
                    checked_at: timestamp_now(),
                });
                // Tests and air-gapped operators can point the daemon at a
                // local, already-downloaded manifest/signature pair. Normal
                // production checks fetch Frank's HTTPS feed at the server
                // edge. In both cases signature verification happens before
                // any manifest fields influence state.
                let result = if let (Some(manifest_path), Some(signature_path)) = (
                    std::env::var_os("FRANK_UPDATE_MANIFEST"),
                    std::env::var_os("FRANK_UPDATE_SIGNATURE"),
                ) {
                    (|| -> std::result::Result<_, String> {
                        let manifest =
                            std::fs::read(manifest_path).map_err(|error| error.to_string())?;
                        let signature = std::fs::read_to_string(signature_path)
                            .map_err(|error| error.to_string())?;
                        frank_update::parse_verified_manifest(
                            &manifest,
                            &signature,
                            &base64::engine::general_purpose::STANDARD
                                .decode(frank_update::EMBEDDED_PUBLIC_KEY_B64)
                                .map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())
                    })()
                } else {
                    fetch_update_manifest()
                        .await
                        .map_err(|error| error.to_string())
                };
                match result {
                    Ok(manifest) => {
                        let current = env!("CARGO_PKG_VERSION");
                        let target = std::env::var("FRANK_UPDATE_TARGET")
                            .unwrap_or_else(|_| current_update_target());
                        let package_kind = std::env::var("FRANK_UPDATE_PACKAGE_KIND")
                            .unwrap_or_else(|_| current_update_package_kind());
                        // A signed manifest may carry entries for every
                        // release target, but a host must never silently
                        // fall back to the first entry when its target or
                        // package kind is absent. Doing so could stage an
                        // artifact for another architecture/OS and turn a
                        // wrong-target release into a destructive swap.
                        let artifact = select_update_artifact(&manifest, &target, &package_kind);
                        let compatible = manifest.accepts_protocol(PROTOCOL_VERSION)
                            && manifest.rejects_downgrade_from(current).is_ok()
                            && artifact.is_some();
                        if !compatible {
                            update.state = UpdateState::Failed;
                            update.error =
                                Some("update manifest is incompatible with this host".into());
                        } else if let Some(artifact) = artifact {
                            update.version = manifest.frank_version.clone();
                            update.target = artifact.target.clone();
                            update.size = artifact.size;
                            update.sha256 = artifact.sha256.clone();
                            update.state = UpdateState::Available;
                            update.error = None;
                        }
                        update.checked_at = timestamp_now();
                    }
                    Err(_error) => {
                        update.state = UpdateState::Failed;
                        update.error = Some("update manifest verification failed".into());
                        // Keep detailed diagnostics out of the wire snapshot;
                        // they may contain a local path, URL, or proxy data.
                        update.checked_at = timestamp_now();
                    }
                }
                snapshot.update = Some(update.clone());
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Update(update),
                ))
            }
            Command::PrepareUpdate { version } => {
                if version.trim().is_empty() || version.len() > 64 {
                    return Err(OrchestratorError::Validation(
                        "update version is invalid".into(),
                    ));
                }
                let update = snapshot
                    .update
                    .as_ref()
                    .filter(|update| update.version == version)
                    .ok_or_else(|| {
                        OrchestratorError::Validation(
                            "no verified update manifest is available for this version".into(),
                        )
                    })?;
                if !matches!(update.state, UpdateState::Available | UpdateState::Failed) {
                    if update.state == UpdateState::Staged
                        && let Some(operation) = snapshot
                            .operations
                            .iter()
                            .find(|operation| {
                                operation.kind == OperationKind::StageUpdate
                                    && serde_json::from_str::<StageUpdateOperation>(
                                        &operation.resource,
                                    )
                                    .is_ok_and(|request| request.update_id == update.id)
                                    && operation.status == OperationStatus::Succeeded
                            })
                            .cloned()
                    {
                        return Ok((
                            snapshot,
                            Event::OperationChanged {
                                operation: operation.clone(),
                            },
                            CommandResult::Operation(operation),
                        ));
                    }
                    return Err(OrchestratorError::InvalidTransition(
                        "update is not available for staging".into(),
                    ));
                }
                if let Some(operation) = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::StageUpdate
                            && serde_json::from_str::<StageUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| request.update_id == update.id)
                            && !matches!(
                                operation.status,
                                OperationStatus::Failed | OperationStatus::Cancelled
                            )
                    })
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::StageUpdate,
                    status: OperationStatus::Queued,
                    resource: serde_json::to_string(&StageUpdateOperation {
                        update_id: update.id,
                    })
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?,
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::ApplyUpdate { update_id } => {
                let update = snapshot
                    .update
                    .as_mut()
                    .filter(|update| update.id == update_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if update.state != UpdateState::Staged {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a staged update can be applied".into(),
                    ));
                }
                update.state = UpdateState::Applying;
                update.checked_at = timestamp_now();
                let update = update.clone();
                // Applying a bundle is a host side effect. Persist its
                // operation in the same transaction as the state transition
                // so a response loss or daemon crash cannot strand an update
                // in `Applying` without a retryable journal entry.
                let operation = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::HostUpdate
                            && serde_json::from_str::<HostUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| {
                                    request.update_id == update_id
                                        && request.action == HostUpdateAction::Apply
                                })
                            && !matches!(
                                operation.status,
                                OperationStatus::Succeeded
                                    | OperationStatus::Cancelled
                                    | OperationStatus::Failed
                            )
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        let now = timestamp_now();
                        let operation = OperationView {
                            id: OperationId::new(),
                            kind: OperationKind::HostUpdate,
                            status: OperationStatus::Queued,
                            resource: serde_json::to_string(&HostUpdateOperation {
                                update_id,
                                action: HostUpdateAction::Apply,
                            })
                            .unwrap_or_else(|_| update_id.to_string()),
                            phase: "queued".into(),
                            attempt: 0,
                            error: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        snapshot.operations.push(operation.clone());
                        operation
                    });
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::RollbackUpdate => {
                let update = snapshot
                    .update
                    .as_mut()
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(update.state, UpdateState::Applying | UpdateState::Failed) {
                    return Err(OrchestratorError::InvalidTransition(
                        "no failed or applying update can be rolled back".into(),
                    ));
                }
                // Rollback is also a host side effect. Keep the update in an
                // applying state until frank-updater confirms the swap, and
                // persist a separate journal row for idempotent recovery.
                update.state = UpdateState::Applying;
                update.error = Some("rollback queued".into());
                update.checked_at = timestamp_now();
                let update = update.clone();
                let operation = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::HostUpdate
                            && serde_json::from_str::<HostUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| {
                                    request.update_id == update.id
                                        && request.action == HostUpdateAction::Rollback
                                })
                            && !matches!(
                                operation.status,
                                OperationStatus::Succeeded
                                    | OperationStatus::Cancelled
                                    | OperationStatus::Failed
                            )
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        let now = timestamp_now();
                        let operation = OperationView {
                            id: OperationId::new(),
                            kind: OperationKind::HostUpdate,
                            status: OperationStatus::Queued,
                            resource: serde_json::to_string(&HostUpdateOperation {
                                update_id: update.id,
                                action: HostUpdateAction::Rollback,
                            })
                            .unwrap_or_else(|_| update.id.to_string()),
                            phase: "rollback-queued".into(),
                            attempt: 0,
                            error: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        snapshot.operations.push(operation.clone());
                        operation
                    });
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            _ => Err(OrchestratorError::Validation(
                "command was routed to the wrong reducer".into(),
            )),
        }
    }
}
