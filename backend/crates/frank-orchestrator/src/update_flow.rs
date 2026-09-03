//! Self-update: manifest fetch, artifact download, staging, and the host swap.
//!
//! Signing, target selection and rollback validation belong to `frank-update`;
//! this module only sequences them and records progress on the snapshot.
//!
//! OPEN QUESTION: the HTTP client here means the orchestrator reaches the
//! network directly. Moving the fetch/download half into `frank-update` would
//! keep that dependency out of the orchestration crate, but it changes the
//! dependency graph (which xtask's architecture-check pins) and pulls reqwest
//! across a crate boundary. Deliberately left as a design decision.

use std::path::PathBuf;

use frank_protocol::*;

use crate::*;

/// Metadata for a host update operation.  The operation deliberately stores
/// only stable identifiers and an action; filesystem locations are resolved
/// from the daemon's environment at execution time so snapshots never leak
/// updater paths to remote clients.  Keeping the action in the journal makes
/// apply and rollback idempotent across a daemon restart.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct HostUpdateOperation {
    pub(crate) update_id: UpdateId,
    pub(crate) action: HostUpdateAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct StageUpdateOperation {
    pub(crate) update_id: UpdateId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostUpdateAction {
    Apply,
    Rollback,
}

pub(crate) fn current_update_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        "linux" => "unknown-linux-gnu",
        other => other,
    };
    format!("{arch}-{os}")
}

/// Download and verify the signed release feed. The response advertises and
/// enforces a hard body cap before parsing so an endpoint cannot make the
/// daemon retain an unbounded manifest. Signature verification happens before
/// the JSON manifest is deserialized or any artifact metadata is used.
pub(crate) async fn fetch_update_manifest()
-> std::result::Result<frank_update::UpdateManifest, String> {
    const MAX_MANIFEST_BYTES: usize = 256 * 1024;
    const MAX_SIGNATURE_BYTES: usize = 64 * 1024;
    if std::env::var_os("FRANK_UPDATE_DISABLE_NETWORK").is_some() {
        return Err("network update checks are disabled".into());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(format!("frank/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let manifest_response = client
        .get(frank_update::UPDATE_FEED_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let manifest = read_bounded_http_body(manifest_response, MAX_MANIFEST_BYTES).await?;
    let signature_response = client
        .get(frank_update::UPDATE_SIGNATURE_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let signature = read_bounded_http_body(signature_response, MAX_SIGNATURE_BYTES).await?;
    let signature = std::str::from_utf8(&signature).map_err(|error| error.to_string())?;
    let public_key = base64::engine::general_purpose::STANDARD
        .decode(frank_update::EMBEDDED_PUBLIC_KEY_B64)
        .map_err(|error| error.to_string())?;
    frank_update::parse_verified_manifest(&manifest, signature, &public_key)
        .map_err(|error| error.to_string())
}

/// Fetch one manifest-selected payload with the same bounded streaming rules
/// used for the signed feed. Tests and air-gapped operators may provide a
/// local file through `FRANK_UPDATE_ARTIFACT`; that path is checked as a
/// regular file and still passes through the exact digest/size verifier.
pub(crate) async fn download_update_artifact(
    artifact: &frank_update::UpdateArtifact,
) -> Result<Vec<u8>> {
    let bytes = if let Some(path) = std::env::var_os("FRANK_UPDATE_ARTIFACT") {
        let path = PathBuf::from(path);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            OrchestratorError::Validation(format!(
                "update artifact could not be inspected: {error}"
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(OrchestratorError::Validation(
                "update artifact path is not a regular file".into(),
            ));
        }
        if metadata.len() > frank_update::MAX_UPDATE_ARTIFACT_BYTES {
            return Err(OrchestratorError::Validation(
                "update artifact exceeds the configured size cap".into(),
            ));
        }
        let file = std::fs::File::open(&path).map_err(|error| {
            OrchestratorError::Validation(format!("update artifact could not be opened: {error}"))
        })?;
        let mut bytes = Vec::new();
        let mut limited = file.take(frank_update::MAX_UPDATE_ARTIFACT_BYTES.saturating_add(1));
        limited.read_to_end(&mut bytes).map_err(|error| {
            OrchestratorError::Validation(format!("update artifact could not be read: {error}"))
        })?;
        bytes
    } else {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(300))
            .user_agent(format!("frank/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let response = client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?
            .error_for_status()
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let cap = usize::try_from(frank_update::MAX_UPDATE_ARTIFACT_BYTES).unwrap_or(usize::MAX);
        read_bounded_http_body(response, cap)
            .await
            .map_err(OrchestratorError::Validation)?
    };
    frank_update::verify_artifact_bytes(artifact, &bytes)
        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
    Ok(bytes)
}

/// Resolve the daemon-owned staging root without exposing it through a
/// snapshot. Package/service deployments can set an explicit root; local
/// development falls back to a sibling of the current bundle, then the v1
/// database directory, and finally a private temporary root for in-memory
/// tests.
pub(crate) fn update_staging_root(store: &Store) -> PathBuf {
    if let Some(path) = std::env::var_os("FRANK_UPDATE_STAGING") {
        return PathBuf::from(path);
    }
    if let Some(current) = std::env::var_os("FRANK_UPDATE_CURRENT")
        && let Some(parent) = Path::new(&current).parent()
    {
        return parent.join("staging");
    }
    if let Some(database) = store.database_path()
        && let Some(parent) = database.parent()
    {
        return parent.join("updates").join("staging");
    }
    std::env::temp_dir()
        .join("frank")
        .join("v1")
        .join("updates")
        .join("staging")
}

pub(crate) fn current_update_package_kind() -> String {
    match std::env::consts::OS {
        "macos" => "dmg",
        "windows" => "msi",
        _ => "tar.gz",
    }
    .into()
}

pub(crate) fn select_update_artifact<'a>(
    manifest: &'a frank_update::UpdateManifest,
    target: &str,
    package_kind: &str,
) -> Option<&'a frank_update::UpdateArtifact> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.target == target && artifact.package_kind == package_kind)
}

pub(crate) async fn read_bounded_http_body(
    mut response: reqwest::Response,
    cap: usize,
) -> std::result::Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > cap as u64)
    {
        return Err("update response exceeds the configured size cap".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if chunk.len() > cap || body.len().saturating_add(chunk.len()) > cap {
            return Err("update response exceeds the configured size cap".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

impl Orchestrator {
    /// Download, verify, and stage a release payload as a durable operation.
    /// The command that requests preparation only records this intent; the
    /// side effect happens after the transaction commits and can be resumed
    /// idempotently by the startup reconciler.
    pub(crate) async fn run_stage_update_operation(&self, operation: OperationView) -> Result<()> {
        let request: StageUpdateOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("stage update metadata is invalid".into())
            })?;
        let result = async {
            let update = self
                .store
                .snapshot()
                .await?
                .update
                .filter(|update| update.id == request.update_id)
                .ok_or(OrchestratorError::NotFound)?;
            if !matches!(update.state, UpdateState::Available | UpdateState::Failed) {
                if update.state == UpdateState::Staged {
                    return Ok::<(), OrchestratorError>(());
                }
                return Err(OrchestratorError::InvalidTransition(
                    "update is not available for staging".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "manifest", None)
                .await?;
            let manifest = fetch_update_manifest()
                .await
                .map_err(OrchestratorError::Validation)?;
            if manifest.frank_version != update.version
                || !manifest.accepts_protocol(PROTOCOL_VERSION)
                || manifest
                    .rejects_downgrade_from(env!("CARGO_PKG_VERSION"))
                    .is_err()
            {
                return Err(OrchestratorError::Validation(
                    "verified update manifest no longer matches the selected update".into(),
                ));
            }
            let target =
                std::env::var("FRANK_UPDATE_TARGET").unwrap_or_else(|_| current_update_target());
            let package_kind = std::env::var("FRANK_UPDATE_PACKAGE_KIND")
                .unwrap_or_else(|_| current_update_package_kind());
            let artifact =
                select_update_artifact(&manifest, &target, &package_kind).ok_or_else(|| {
                    OrchestratorError::Validation(
                        "update artifact is not available for this host".into(),
                    )
                })?;
            if artifact.size != update.size || !artifact.sha256.eq_ignore_ascii_case(&update.sha256)
            {
                return Err(OrchestratorError::Validation(
                    "update artifact metadata changed since the check".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "download", None)
                .await?;
            let bytes = download_update_artifact(artifact).await?;
            self.update_operation(&operation, OperationStatus::Running, "stage", None)
                .await?;
            let staging_root = update_staging_root(&self.store);
            let staged = frank_update::stage_artifact(artifact, &bytes, &staging_root)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if !staged.is_file() {
                return Err(OrchestratorError::Validation(
                    "staged update payload is missing".into(),
                ));
            }
            self.set_update_state(request.update_id, UpdateState::Staged, None)
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                self.set_update_state(request.update_id, UpdateState::Failed, Some(reason.clone()))
                    .await?;
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    /// Execute a staged host update through the detached helper binary.  The
    /// daemon never replaces its own files directly: the helper owns the
    /// directory-level swap and keeps the previous bundle for rollback.  All
    /// paths come from the host service environment rather than the wire
    /// snapshot, and missing helper configuration is a durable waiting state
    /// that an owner can resume with `RetryOperation` after configuring it.
    pub(crate) async fn run_host_update_operation(&self, operation: OperationView) -> Result<()> {
        let request: HostUpdateOperation = serde_json::from_str(&operation.resource)
            .map_err(|_| OrchestratorError::Validation("host update metadata is invalid".into()))?;
        let update = self
            .store
            .snapshot()
            .await?
            .update
            .filter(|update| update.id == request.update_id)
            .ok_or(OrchestratorError::NotFound)?;
        let missing = ["FRANK_UPDATE_CURRENT", "FRANK_UPDATE_PREVIOUS"]
            .iter()
            .filter(|name| std::env::var_os(name).is_none())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.update_operation(
                &operation,
                OperationStatus::Waiting,
                "awaiting-helper",
                Some(format!(
                    "configure updater paths before retrying ({})",
                    missing.join(", ")
                )),
            )
            .await?;
            return Ok(());
        }
        // Keep a consistent SQLite recovery point before the helper can swap
        // the host bundle. The backup path is deterministic per update so a
        // daemon restart/retry reuses the already-verified copy instead of
        // creating a second snapshot or refusing an otherwise safe retry.
        if let Err(error) = self.backup_before_host_update(request.update_id).await {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    &format!("database backup before update failed: {error}"),
                )
                .await;
        }
        self.update_operation(&operation, OperationStatus::Running, "verify", None)
            .await?;
        let helper = std::env::var_os("FRANK_UPDATER_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("frank-updater"));
        if helper.components().count() > 1
            && std::fs::symlink_metadata(&helper)
                .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
                .unwrap_or(true)
        {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    "configured updater helper is not a regular file",
                )
                .await;
        }
        let current = std::env::var_os("FRANK_UPDATE_CURRENT")
            .map(PathBuf::from)
            .ok_or_else(|| {
                OrchestratorError::Validation("FRANK_UPDATE_CURRENT is missing".into())
            })?;
        let previous = std::env::var_os("FRANK_UPDATE_PREVIOUS")
            .map(PathBuf::from)
            .ok_or_else(|| {
                OrchestratorError::Validation("FRANK_UPDATE_PREVIOUS is missing".into())
            })?;
        let mut command = AsyncCommand::new(helper);
        match request.action {
            HostUpdateAction::Apply => {
                // Prefer an explicitly configured path for package managers
                // that stage bundles outside Frank's data root. Otherwise
                // derive the same deterministic path used by the staging
                // operation, so a daemon restart can resume without a
                // server-path value ever crossing the wire.
                let staged = if let Some(path) = std::env::var_os("FRANK_UPDATE_STAGED") {
                    PathBuf::from(path)
                } else {
                    let artifact = frank_update::UpdateArtifact {
                        target: update.target.clone(),
                        package_kind: current_update_package_kind(),
                        url: "https://invalid.local/frank-update".into(),
                        size: update.size,
                        sha256: update.sha256.clone(),
                    };
                    frank_update::staged_artifact_path(&artifact, update_staging_root(&self.store))
                };
                command.args([
                    "swap",
                    "--staged",
                    &staged.to_string_lossy(),
                    "--current",
                    &current.to_string_lossy(),
                    "--previous",
                    &previous.to_string_lossy(),
                ]);
            }
            HostUpdateAction::Rollback => {
                command.args([
                    "rollback",
                    "--current",
                    &current.to_string_lossy(),
                    "--previous",
                    &previous.to_string_lossy(),
                ]);
            }
        }
        self.update_operation(&operation, OperationStatus::Running, "swap", None)
            .await?;
        command.kill_on_drop(true);
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            command.output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return self
                    .finish_host_update_failure(
                        &operation,
                        request.update_id,
                        &format!("updater helper failed to start: {error}"),
                    )
                    .await;
            }
            Err(_) => {
                return self
                    .finish_host_update_failure(
                        &operation,
                        request.update_id,
                        "updater helper timed out",
                    )
                    .await;
            }
        };
        if output.stdout.len() > MAX_COMMAND_BODY_BYTES
            || output.stderr.len() > MAX_COMMAND_BODY_BYTES
        {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    "updater helper output exceeded the configured limit",
                )
                .await;
        }
        if !output.status.success() {
            let diagnostic = bounded_text(&[output.stdout, output.stderr].concat());
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    &format!("updater helper exited unsuccessfully: {diagnostic}"),
                )
                .await;
        }
        let state = match request.action {
            HostUpdateAction::Apply => UpdateState::Succeeded,
            HostUpdateAction::Rollback => UpdateState::RolledBack,
        };
        self.set_update_state(request.update_id, state, None)
            .await?;
        self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
            .await
    }

    pub(crate) async fn finish_host_update_failure(
        &self,
        operation: &OperationView,
        update_id: UpdateId,
        reason: &str,
    ) -> Result<()> {
        let reason = bounded_text(reason.as_bytes());
        self.set_update_state(update_id, UpdateState::Failed, Some(reason.clone()))
            .await?;
        self.finish_operation(operation.id, OperationStatus::Failed, reason)
            .await
    }

    pub(crate) async fn backup_before_host_update(&self, update_id: UpdateId) -> Result<()> {
        let Some(database) = self.store.database_path().map(Path::to_path_buf) else {
            // In-memory stores are used by deterministic unit/fake-provider
            // tests and have no durable file to back up.
            return Ok(());
        };
        let parent = database
            .parent()
            .ok_or_else(|| OrchestratorError::Validation("database path has no parent".into()))?;
        let file_name = database
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| OrchestratorError::Validation("database file name is invalid".into()))?;
        let destination = parent.join(format!(".{file_name}.update-{update_id}.backup"));
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(OrchestratorError::Validation(
                    "database update backup path is not a regular file".into(),
                ));
            }
            Ok(_) => {
                // Validate an existing recovery point before reusing it. A
                // truncated/foreign file must never be treated as a valid
                // backup just because its name matches the update id.
                let backup = Store::open(&destination).await?;
                backup.integrity_check().await?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(OrchestratorError::Validation(format!(
                    "could not inspect database update backup path: {error}"
                )));
            }
        }
        self.store.backup_to(&destination).await?;
        Ok(())
    }

    pub(crate) async fn set_update_state(
        &self,
        update_id: UpdateId,
        state: UpdateState,
        error: Option<String>,
    ) -> Result<()> {
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            let Some(update) = snapshot
                .update
                .as_mut()
                .filter(|update| update.id == update_id)
            else {
                return Err(OrchestratorError::NotFound);
            };
            update.state = state.clone();
            update.error = error.clone();
            update.checked_at = timestamp_now();
            let update = update.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::UpdateStateChanged { update },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }
}
