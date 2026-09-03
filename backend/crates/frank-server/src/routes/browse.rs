//! Filesystem browsing for the project picker.

use crate::auth::*;
use crate::*;

#[derive(Debug, Deserialize)]
pub(crate) struct BrowseQuery {
    path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowseEntry {
    name: String,
    directory: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrowseResponse {
    path: String,
    entries: Vec<BrowseEntry>,
}

pub(crate) async fn browse_projects(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<BrowseQuery>,
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
            ApiError::new(
                ErrorCode::Forbidden,
                "owner role required to browse project roots",
            ),
        );
    }
    let roots = match state.store.snapshot().await {
        Ok(snapshot) => snapshot.server.allowed_project_roots,
        Err(_) => Vec::new(),
    };
    let path = query
        .path
        .unwrap_or_else(|| roots.first().cloned().unwrap_or_else(|| ".".into()));
    if path.len() > 4_096 || path.chars().any(|character| character.is_control()) {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "path is invalid"),
        );
    }
    if path.split(['/', '\\']).any(|component| component == "..") {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "path traversal is not allowed"),
        );
    }
    let canonical_roots = roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    if !roots.is_empty() && canonical_roots.len() != roots.len() {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::Internal,
                "an allowed project root is unavailable",
            ),
        );
    }
    let path = match canonicalize_allow_missing(std::path::Path::new(&path)) {
        Ok(path) => path,
        Err(_) => {
            return api_error_response(
                StatusCode::NOT_FOUND,
                ApiError::new(ErrorCode::NotFound, "directory not found"),
            );
        }
    };
    if !canonical_roots.is_empty() && !canonical_roots.iter().any(|root| path.starts_with(root)) {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "path is outside an allowed project root",
            ),
        );
    }
    if !path.is_dir() {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "directory not found"),
        );
    }
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(&path) {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return api_error_response(
                StatusCode::NOT_FOUND,
                ApiError::new(ErrorCode::NotFound, "directory not found"),
            );
        }
    };
    for entry in read_dir.flatten().take(256) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // Do not follow symlinks while presenting the server-side browser.
        // A symlink target can still be opened explicitly after it has passed
        // the canonical allowed-root check, but it is never presented as a
        // traversable directory by default.
        let directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        entries.push(BrowseEntry { name, directory });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    (
        StatusCode::OK,
        Json(BrowseResponse {
            path: path.to_string_lossy().into_owned(),
            entries,
        }),
    )
        .into_response()
}
