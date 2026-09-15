//! Owner-only Journal projection endpoint.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use frank_protocol::{
    AgentId, JournalEntryKind, JournalEntryView, JournalFilter, JournalOutcome, MissionId,
    ProjectId, TaskId,
};
use serde::Deserialize;

use crate::auth::authenticate;
use crate::{ServerState, api_error_response};

#[derive(Debug, Deserialize, Default)]
pub(crate) struct JournalQuery {
    pub before_sequence: Option<u64>,
    pub limit: Option<u32>,
    pub project_id: Option<String>,
    pub mission_id: Option<String>,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    /// Comma-separated enum names keep this endpoint usable from curl and
    /// from the small Flutter HTTP transport without a custom query encoder.
    pub kinds: Option<String>,
    pub outcomes: Option<String>,
}

pub(crate) async fn journal(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<JournalQuery>,
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
                "owner role required for Journal",
            ),
        );
    }
    let filter = match build_filter(query) {
        Ok(filter) => filter,
        Err(message) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                frank_protocol::ApiError::new(frank_protocol::ErrorCode::Validation, message),
            );
        }
    };
    match state.store.journal_page(&filter).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        ),
    }
}

fn build_filter(query: JournalQuery) -> Result<JournalFilter, String> {
    Ok(JournalFilter {
        before_sequence: query.before_sequence,
        limit: query.limit,
        project_id: parse_id(query.project_id, "project_id")?,
        mission_id: parse_id(query.mission_id, "mission_id")?,
        task_id: parse_id(query.task_id, "task_id")?,
        agent_id: parse_id(query.agent_id, "agent_id")?,
        kinds: parse_list(query.kinds, "kind", |value| {
            serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
        })?,
        outcomes: parse_list(query.outcomes, "outcome", |value| {
            serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
        })?,
    })
}

fn parse_id<T>(value: Option<String>, name: &str) -> Result<Option<T>, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    value
        .map(|value| {
            serde_json::from_value(serde_json::Value::String(value))
                .map_err(|_| format!("{name} is not a valid identifier"))
        })
        .transpose()
}

fn parse_list<T>(
    value: Option<String>,
    name: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse(value.trim()).ok_or_else(|| format!("unknown {name} filter: {value}")))
        .collect()
}

#[allow(dead_code)]
fn _journal_types_are_linked(
    _: Option<ProjectId>,
    _: Option<MissionId>,
    _: Option<TaskId>,
    _: Option<AgentId>,
    _: Option<JournalEntryKind>,
    _: Option<JournalOutcome>,
    _: Option<JournalEntryView>,
) {
}
