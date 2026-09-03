//! Artifact publication and resumable upload.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_artifact(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::PublishArtifact(spec) => {
                if spec.bytes.len() as u64 > MAX_ARTIFACT_BYTES {
                    return Err(OrchestratorError::Validation(
                        "artifact exceeds the configured size cap".into(),
                    ));
                }
                if spec.name.trim().is_empty()
                    || spec.name.len() > 256
                    || spec.name.chars().any(|character| character.is_control())
                    || spec.mime_type.trim().is_empty()
                    || spec.mime_type.len() > 256
                    || spec
                        .mime_type
                        .chars()
                        .any(|character| character.is_control())
                {
                    return Err(OrchestratorError::Validation(
                        "artifact name or MIME type is invalid".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                let id = ArtifactId::new();
                let mut digest = Sha256::new();
                digest.update(&spec.bytes);
                let artifact = ArtifactView {
                    id,
                    mission_id: spec.mission_id,
                    task_id: spec.task_id,
                    upload_id: None,
                    name: spec.name,
                    mime_type: spec.mime_type,
                    size: spec.bytes.len() as u64,
                    sha256: hex::encode(digest.finalize()),
                    pinned: false,
                    created_at: timestamp_now(),
                };
                snapshot.artifacts.push(artifact.clone());
                Ok((
                    snapshot,
                    Event::ArtifactPublished { artifact },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::BeginArtifactUpload(spec) => {
                if spec.size > MAX_ARTIFACT_BYTES
                    || spec.name.trim().is_empty()
                    || spec.name.len() > 256
                    || spec.mime_type.trim().is_empty()
                    || spec.mime_type.len() > 256
                    || spec.sha256.len() != 64
                    || !spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(OrchestratorError::Validation(
                        "artifact upload metadata is invalid".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                let upload = ArtifactUploadView {
                    id: UploadId::new(),
                    spec,
                    received: 0,
                    completed: false,
                    created_at: timestamp_now(),
                    expires_at: format!("{}", now_plus_seconds(600)),
                };
                snapshot.uploads.push(upload.clone());
                Ok((
                    snapshot,
                    Event::ArtifactUploadStarted {
                        upload: upload.clone(),
                    },
                    CommandResult::Upload(upload),
                ))
            }
            Command::FinalizeArtifactUpload {
                upload_id,
                sha256,
                size,
            } => {
                let upload = snapshot
                    .uploads
                    .iter_mut()
                    .find(|upload| upload.id == upload_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if upload.completed {
                    return Err(OrchestratorError::InvalidTransition(
                        "artifact upload is already complete".into(),
                    ));
                }
                let expires_at = upload.expires_at.parse::<u128>().map_err(|_| {
                    OrchestratorError::Validation(
                        "artifact upload expiry metadata is invalid".into(),
                    )
                })?;
                if expires_at <= epoch_seconds() as u128 {
                    return Err(OrchestratorError::InvalidTransition(
                        "artifact upload has expired; start a new upload".into(),
                    ));
                }
                if upload.spec.size != size || !sha256.eq_ignore_ascii_case(&upload.spec.sha256) {
                    return Err(OrchestratorError::Validation(
                        "artifact upload is incomplete or has the wrong digest".into(),
                    ));
                }
                let bytes = self
                    .store
                    .artifact_upload_bytes(upload_id)
                    .await?
                    .ok_or_else(|| {
                        OrchestratorError::Validation(
                            "artifact upload bytes are missing; retry the upload".into(),
                        )
                    })?;
                if bytes.len() as u64 != size {
                    return Err(OrchestratorError::Validation(
                        "artifact upload byte count does not match metadata".into(),
                    ));
                }
                // Chunk writes live in the upload table and intentionally do
                // not emit one event per network packet. Refresh the
                // projection's received count from those durable bytes at
                // finalize time so a reconnect or a stale snapshot cannot
                // make a complete upload appear incomplete.
                upload.received = bytes.len() as u64;
                let mut digest = Sha256::new();
                digest.update(&bytes);
                let actual_sha256 = hex::encode(digest.finalize());
                if !actual_sha256.eq_ignore_ascii_case(&upload.spec.sha256)
                    || !actual_sha256.eq_ignore_ascii_case(&sha256)
                {
                    return Err(OrchestratorError::Validation(
                        "artifact upload digest verification failed".into(),
                    ));
                }
                let artifact = ArtifactView {
                    id: ArtifactId::from(upload_id.0),
                    mission_id: upload.spec.mission_id,
                    task_id: upload.spec.task_id,
                    upload_id: Some(upload_id),
                    name: upload.spec.name.clone(),
                    mime_type: upload.spec.mime_type.clone(),
                    size,
                    sha256: actual_sha256,
                    pinned: false,
                    created_at: timestamp_now(),
                };
                upload.completed = true;
                if !snapshot
                    .artifacts
                    .iter()
                    .any(|candidate| candidate.id == artifact.id)
                {
                    snapshot.artifacts.push(artifact.clone());
                }
                Ok((
                    snapshot,
                    Event::ArtifactPublished {
                        artifact: artifact.clone(),
                    },
                    CommandResult::Created {
                        id: artifact.id.to_string(),
                    },
                ))
            }
            _ => super::misrouted(),
        }
    }
}
