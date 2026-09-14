//! Health, capability advertisement and the operator-facing diagnostics probe.

use crate::auth::*;
use crate::*;

#[derive(Debug, Serialize)]
pub(crate) struct Health {
    ok: bool,
    server_id: ServerId,
    protocol_version: u16,
    revision: u64,
}

pub(crate) async fn health(State(state): State<ServerState>) -> impl IntoResponse {
    // Keep liveness cheap but do not report a healthy daemon when SQLite has
    // failed integrity checks.  The detailed owner-only diagnostics endpoint
    // carries the rest of the component matrix; this endpoint is suitable for
    // service managers and load balancers.
    let database_ok = state.store.integrity_check().await.is_ok();
    let revision = state.store.current_revision().await.unwrap_or_default();
    Json(Health {
        ok: database_ok,
        server_id: state.store.server_id(),
        protocol_version: PROTOCOL_VERSION,
        revision,
    })
}

/// Return the owner-only, sanitized daemon diagnostic bundle.  The ordinary
/// health endpoint remains intentionally cheap and unauthenticated; this
/// endpoint is the detailed operational surface used by the GUI/CLI doctor
/// screens and never includes raw command output or server filesystem paths.
pub(crate) async fn diagnostics(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required for diagnostics"),
        );
    }

    let generated_at = timestamp_now();
    let database = match state.store.integrity_check().await {
        Ok(()) => HealthStatus::Healthy,
        Err(_) => HealthStatus::Unhealthy,
    };
    let audit_backlog = state.store.audit_backlog_count().await.unwrap_or_default();
    let audit_exporter = if audit_backlog == 0 {
        HealthStatus::Healthy
    } else {
        HealthStatus::Degraded
    };
    let snapshot = state.store.snapshot().await.ok();
    let (retention, operation_backlog, git) = if let Some(snapshot) = snapshot {
        let operation_backlog = snapshot
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation.status,
                    OperationStatus::Queued
                        | OperationStatus::Running
                        | OperationStatus::Waiting
                        | OperationStatus::Recovering
                )
            })
            .count() as u64;
        let pending_uploads = snapshot
            .uploads
            .iter()
            .filter(|upload| !upload.completed)
            .count() as u64;
        let roots_ok = snapshot
            .server
            .allowed_project_roots
            .iter()
            .all(|root| std::fs::metadata(root).is_ok_and(|metadata| metadata.is_dir()));
        let projects_ok = snapshot
            .projects
            .iter()
            .filter(|project| !project.archived)
            .all(|project| {
                std::fs::metadata(&project.path).is_ok_and(|metadata| metadata.is_dir())
            });
        let git = if roots_ok && projects_ok {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };
        (
            RetentionView {
                event_days: snapshot.server.event_retention_days,
                terminal_days: snapshot.server.terminal_retention_days,
                artifact_days: snapshot.server.artifact_retention_days,
                terminal_max_bytes: 10 * 1024 * 1024,
                pending_uploads,
                pending_operations: operation_backlog,
            },
            operation_backlog,
            git,
        )
    } else {
        (
            RetentionView {
                event_days: 0,
                terminal_days: 0,
                artifact_days: 0,
                terminal_max_bytes: 10 * 1024 * 1024,
                pending_uploads: 0,
                pending_operations: 0,
            },
            0,
            HealthStatus::Unknown,
        )
    };
    let probes = state.orchestrator.runtime_doctor().await;
    let providers = probes
        .into_iter()
        .map(|probe| {
            let status = if probe.capability.available && probe.capability.configured {
                HealthStatus::Healthy
            } else if probe.capability.available {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unhealthy
            };
            let detail = if probe.capability.available {
                if probe.capability.configured {
                    None
                } else {
                    Some("provider credential is required".to_string())
                }
            } else {
                Some(
                    probe
                        .capability
                        .diagnostic
                        .unwrap_or_else(|| "provider API is unavailable".to_string()),
                )
            };
            RuntimeDoctorCheck {
                component: "openrouter".to_string(),
                status,
                version: probe.capability.version,
                detail,
                remediation: Some("run `frank server doctor` on the daemon host".to_string()),
            }
        })
        .collect();
    let installed = frank_service::is_installed();
    let service = Some(ServiceStatusView {
        service_name: frank_service::SERVICE_NAME.to_string(),
        installed,
        running: true,
        pid: std::process::id().into(),
        descriptor_path: None,
        health: if installed {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        },
        detail: (!installed).then(|| {
            "frankd is running without a detected per-user service descriptor".to_string()
        }),
        checked_at: generated_at.clone(),
    });
    (
        StatusCode::OK,
        Json(DiagnosticSnapshot {
            generated_at,
            database,
            audit_exporter,
            git,
            openrouter: providers,
            service,
            retention,
            operation_backlog,
            disk_free_bytes: None,
            redactions: vec![
                "provider environment and credentials omitted".into(),
                "server filesystem paths omitted".into(),
                "terminal input and bearer tokens omitted".into(),
            ],
        }),
    )
        .into_response()
}

pub(crate) async fn capabilities(State(state): State<ServerState>) -> impl IntoResponse {
    Json(capability_document(&state).await)
}

pub(crate) async fn capability_document(state: &ServerState) -> Capabilities {
    // Before bootstrap, capabilities are a minimal discovery surface. Avoid
    // probing provider executables or exposing feature inventory until an
    // owner session exists; health, handshake, capabilities, and auth status
    // are the only useful endpoints in this state.
    let configured = state.store.owner().await.ok().flatten().is_some();
    let probes = if configured {
        state.orchestrator.runtime_doctor().await
    } else {
        Vec::new()
    };
    let openrouter = probes
        .into_iter()
        .next()
        .map(|probe| {
            let mut capability = probe.capability;
            // Capabilities are intentionally unauthenticated bootstrap data.
            capability.diagnostic = capability.diagnostic.map(|_| {
                "OpenRouter is unavailable or requires a credential; run frank server doctor".into()
            });
            capability.credential_source = None;
            capability.catalog_refreshed_at = None;
            capability
        })
        .unwrap_or_default();
    let settings = state
        .store
        .snapshot()
        .await
        .ok()
        .map(|snapshot| snapshot.server);
    let settings = settings.unwrap_or_default();
    let mut features = vec![LOCAL_AUTH_FEATURE.into()];
    if configured {
        features.extend([
            "projects".into(),
            "missions".into(),
            "kanban".into(),
            "team-roles".into(),
            "team".into(),
            "organization".into(),
            "taskboard-routing".into(),
            "connector-registry".into(),
            "task-claims".into(),
            "task-feed".into(),
            "task-artifacts".into(),
            "agent-messaging".into(),
            "approvals".into(),
            "terminal-lease".into(),
            "terminal-replay".into(),
            "artifact-upload".into(),
            "durable-operations".into(),
            "signed-updates".into(),
            "audit-jsonl".into(),
        ]);
    }
    Capabilities {
        protocol_version: PROTOCOL_VERSION,
        supported_versions: VersionRange::current(),
        minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
        server_id: state.store.server_id(),
        certificate_fingerprint: state.config.fingerprint(),
        server_version: state.config.server_version.clone(),
        features,
        openrouter,
        limits: CapabilityLimits {
            max_concurrency: settings.max_concurrency,
            ..CapabilityLimits::default()
        },
    }
}
