//! Google Workspace connector tool domain.

use base64::Engine;
use frank_protocol::*;
use reqwest::Client;
use serde_json::{Value, json};

use super::{CONNECTOR_RESPONSE_CAP, CONNECTOR_TIMEOUT, ToolExecutionContext, required_string};
use crate::organization_tool_profile;

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let profile = organization_tool_profile(context.snapshot, context.agent_id, name)
        .ok_or_else(|| "Organization connector profile is unavailable".to_string())?;
    if profile.kind != ConnectorKind::GoogleWorkspace {
        return Err("connector profile kind does not support this tool".into());
    }
    let secret = context
        .orchestrator
        .connector_secret(profile.id)
        .await
        .map_err(|error| format!("Google credential lookup failed: {error}"))?
        .ok_or_else(|| "Google Workspace credential is not configured".to_string())?;
    Ok(Some(
        execute_google_workspace(&profile, name, input, &secret).await?,
    ))
}

fn oauth_access_token(secret: &str) -> String {
    serde_json::from_str::<Value>(secret)
        .ok()
        .and_then(|value| {
            value
                .get("access_token")
                .or_else(|| value.get("token"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|token| !token.trim().is_empty())
        .unwrap_or_else(|| secret.trim().to_owned())
}

async fn execute_google_workspace(
    profile: &ConnectorProfileView,
    name: &str,
    input: &Value,
    secret: &str,
) -> Result<Value, String> {
    let token = oauth_access_token(secret);
    if token.is_empty() {
        return Err("Google Workspace credential is empty".into());
    }
    let client = Client::builder()
        .timeout(CONNECTOR_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let request = |method: reqwest::Method, url: String| {
        client
            .request(method, url)
            .bearer_auth(&token)
            .header(reqwest::header::ACCEPT, "application/json")
    };
    let response = match name {
        "email_search" => {
            let query = required_string(input, "query")?;
            let mut url = reqwest::Url::parse(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages",
            )
            .map_err(|error| error.to_string())?;
            url.query_pairs_mut().append_pair("q", &query);
            request(reqwest::Method::GET, url.to_string()).send().await
        }
        "email_read" => {
            let id = required_string(input, "message_id")?;
            request(
                reqwest::Method::GET,
                format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=full"),
            )
            .send()
            .await
        }
        "email_send" => {
            let to = required_string(input, "to")?;
            let subject = required_string(input, "subject")?;
            let body = required_string(input, "body")?;
            let raw = format!(
                "To: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{body}"
            );
            let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
            request(
                reqwest::Method::POST,
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/send".into(),
            )
            .json(&json!({"raw": encoded}))
            .send()
            .await
        }
        "calendar_list" => {
            let mut url = reqwest::Url::parse(
                "https://www.googleapis.com/calendar/v3/calendars/primary/events",
            )
            .map_err(|error| error.to_string())?;
            if let Some(time_min) = input.get("time_min").and_then(Value::as_str) {
                url.query_pairs_mut().append_pair("timeMin", time_min);
            }
            if let Some(time_max) = input.get("time_max").and_then(Value::as_str) {
                url.query_pairs_mut().append_pair("timeMax", time_max);
            }
            request(reqwest::Method::GET, url.to_string()).send().await
        }
        "calendar_create" => {
            let event = input
                .get("event")
                .cloned()
                .ok_or_else(|| "calendar event is required".to_string())?;
            request(
                reqwest::Method::POST,
                "https://www.googleapis.com/calendar/v3/calendars/primary/events".into(),
            )
            .json(&event)
            .send()
            .await
        }
        "calendar_update" => {
            let id = required_string(input, "event_id")?;
            let event = input
                .get("event")
                .cloned()
                .ok_or_else(|| "calendar event is required".to_string())?;
            request(
                reqwest::Method::PATCH,
                format!("https://www.googleapis.com/calendar/v3/calendars/primary/events/{id}"),
            )
            .json(&event)
            .send()
            .await
        }
        "drive_search" => {
            let query = required_string(input, "query")?;
            let mut url = reqwest::Url::parse("https://www.googleapis.com/drive/v3/files")
                .map_err(|error| error.to_string())?;
            url.query_pairs_mut()
                .append_pair("q", &query)
                .append_pair("spaces", "drive")
                .append_pair("fields", "files(id,name,mimeType,size,modifiedTime)");
            request(reqwest::Method::GET, url.to_string()).send().await
        }
        "drive_read" => {
            let id = required_string(input, "file_id")?;
            request(
                reqwest::Method::GET,
                format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media"),
            )
            .send()
            .await
        }
        "drive_write" => {
            let id = required_string(input, "file_id")?;
            let content = required_string(input, "content")?;
            request(
                reqwest::Method::PATCH,
                format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media"),
            )
            .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(content)
            .send()
            .await
        }
        "drive_share" => {
            let id = required_string(input, "file_id")?;
            let permission = input
                .get("permission")
                .cloned()
                .ok_or_else(|| "Drive permission is required".to_string())?;
            request(
                reqwest::Method::POST,
                format!("https://www.googleapis.com/drive/v3/files/{id}/permissions"),
            )
            .query(&[("sendNotificationEmail", "true")])
            .json(&permission)
            .send()
            .await
        }
        _ => return Err(format!("unsupported Google Workspace tool: {name}")),
    }
    .map_err(|error| format!("Google Workspace request failed: {error}"))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Google Workspace response failed: {error}"))?;
    if bytes.len() > CONNECTOR_RESPONSE_CAP {
        return Err("Google Workspace response exceeds the configured limit".into());
    }
    if !status.is_success() {
        return Err(format!("Google Workspace returned {status}"));
    }
    if name == "drive_read" {
        return Ok(json!({
            "profile_id": profile.id.to_string(),
            "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        }));
    }
    let value = serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({"text": String::from_utf8_lossy(&bytes).to_string()}));
    Ok(json!({"profile_id": profile.id.to_string(), "data": value}))
}
