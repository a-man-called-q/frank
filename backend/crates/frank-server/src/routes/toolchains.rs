//! Built-in and local-manifest toolchain requirement projection.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use frank_protocol::{ToolchainManifest, ToolchainRequirementView};
use serde::Deserialize;

use crate::auth::authenticate;
use crate::{ServerState, api_error_response};

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ToolchainQuery {
    pub project_path: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct ToolchainResponse {
    platform: String,
    requirements: Vec<ToolchainRequirementView>,
}

pub(crate) async fn toolchains(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<ToolchainQuery>,
) -> Response {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Unauthorized,
                "device authentication required",
            ),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "owner role required for toolchain requirements",
            ),
        );
    }
    let project_path = query
        .project_path
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| ".".into());
    let data_directory = state
        .store
        .database_path()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let platform = frank_toolchain::platform_key();
    let manifests = match manifests_for_project(Path::new(&project_path)) {
        Ok(manifests) => manifests,
        Err(error) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                frank_protocol::ApiError::new(frank_protocol::ErrorCode::Validation, error),
            );
        }
    };
    let mut requirements = Vec::new();
    for manifest in manifests {
        match frank_toolchain::resolve_requirement_without_host_probe(
            &manifest,
            &project_path,
            &platform,
            &data_directory,
        ) {
            Ok(mut requirement) => {
                match state
                    .store
                    .toolchain_installation_status(&manifest.id, &manifest.version)
                    .await
                {
                    Ok(Some((status, install_path))) => {
                        requirement.status = status;
                        if install_path.is_some() {
                            requirement.detected_version = Some(manifest.version.clone());
                            requirement.diagnostic = None;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return api_error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            frank_store::api_error(&error),
                        );
                    }
                }
                requirements.push(requirement);
            }
            Err(error) => requirements.push(failed_requirement(&manifest, error.to_string())),
        }
    }
    (
        StatusCode::OK,
        Json(ToolchainResponse {
            platform,
            requirements,
        }),
    )
        .into_response()
}

pub(crate) fn manifests_for_project(project_path: &Path) -> Result<Vec<ToolchainManifest>, String> {
    let mut manifests = frank_toolchain::builtin_manifests();
    if let Some(local_dir) = local_manifest_directory(project_path) {
        let local = frank_toolchain::load_local_manifests(local_dir)
            .map_err(|error| format!("local toolchain manifests are invalid: {error}"))?;
        manifests.extend(local);
    }
    let mut seen = std::collections::HashSet::new();
    for manifest in &manifests {
        if !seen.insert(manifest.id.clone()) {
            return Err(format!("duplicate toolchain manifest id: {}", manifest.id));
        }
    }
    Ok(manifests)
}

fn local_manifest_directory(project_path: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(project_path).ok()?;
    let directory = root.join(".frank/toolchains");
    let metadata = std::fs::symlink_metadata(&directory).ok()?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Some(directory)
    } else {
        None
    }
}

fn failed_requirement(
    manifest: &ToolchainManifest,
    diagnostic: String,
) -> ToolchainRequirementView {
    ToolchainRequirementView {
        manifest_id: manifest.id.clone(),
        label: manifest.label.clone(),
        required_version: manifest.version.clone(),
        status: frank_protocol::ToolchainRequirementStatus::Failed,
        detected_version: None,
        diagnostic: Some(diagnostic),
        install_plan: None,
    }
}
