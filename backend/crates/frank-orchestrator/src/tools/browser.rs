//! Browser connector tool domain.

use base64::Engine;
use frank_protocol::*;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};

use super::{CONNECTOR_TIMEOUT, ToolExecutionContext, required_string};
use crate::{BrowserCdpSession, BrowserLaunchConfig, BrowserPolicy};

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let profile = crate::organization_tool_profile(context.snapshot, context.agent_id, name)
        .ok_or_else(|| "Organization connector profile is unavailable".to_string())?;
    if profile.kind != ConnectorKind::Browser {
        return Err("connector profile kind does not support this tool".into());
    }
    let result = execute_browser(&profile, name, input).await?;
    if name == "browser_download" {
        let encoded = result
            .get("bytes_base64")
            .and_then(Value::as_str)
            .ok_or_else(|| "browser download did not return bytes".to_string())?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| format!("browser download encoding failed: {error}"))?;
        let artifact_name = result
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("browser-download.bin")
            .chars()
            .take(256)
            .collect::<String>();
        let mime_type = result
            .get("mime_type")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_owned();
        let artifact = context
            .orchestrator
            .command_from_agent(
                context.agent_id,
                Command::PublishArtifact(ArtifactSpec {
                    mission_id: context.task.mission_id,
                    task_id: Some(context.task_id),
                    name: artifact_name,
                    mime_type,
                    bytes,
                }),
            )
            .await?;
        return Ok(Some(json!({"download": artifact})));
    }
    Ok(Some(result))
}

async fn execute_browser(
    profile: &ConnectorProfileView,
    name: &str,
    input: &Value,
) -> Result<Value, String> {
    let url = required_string(input, "url")?;
    let object = profile
        .config
        .as_object()
        .ok_or_else(|| "browser connector profile config is invalid".to_string())?;
    let allowed_domains = object
        .get("allowed_domains")
        .and_then(Value::as_array)
        .map(|domains| {
            domains
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let policy = BrowserPolicy {
        allowed_domains,
        max_response_bytes: object
            .get("max_response_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(BrowserPolicy::default().max_response_bytes as u64)
            .min(usize::MAX as u64) as usize,
        max_download_bytes: object
            .get("max_download_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(BrowserPolicy::default().max_download_bytes as u64)
            .min(usize::MAX as u64) as usize,
    };
    let url = policy
        .validate_url(&url)
        .map_err(|error| error.to_string())?;
    let executable = object
        .get("browser_executable")
        .or_else(|| object.get("executable"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("FRANK_CHROME_PATH")
                .ok()
                .filter(|path| !path.trim().is_empty())
        });
    if let Some(executable) = executable {
        let user_data_dir = std::env::temp_dir()
            .join("frank-browser")
            .join(format!("task-{}", uuid::Uuid::new_v4()));
        let mut session =
            BrowserCdpSession::launch(BrowserLaunchConfig::new(executable, user_data_dir))
                .await
                .map_err(|error| error.to_string())?;
        let result = session
            .browse(&url, name, &policy)
            .await
            .map_err(|error| error.to_string());
        session.close().await;
        return result;
    }
    let client = Client::builder()
        .timeout(CONNECTOR_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url.clone())
        .header(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,text/plain",
        )
        .send()
        .await
        .map_err(|error| format!("browser request failed: {error}"))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    if !status.is_success() {
        return Err(format!("browser returned {status}"));
    }
    let limit = if name == "browser_download" {
        policy.max_download_bytes
    } else {
        policy.max_response_bytes
    };
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(if name == "browser_download" {
            "browser download exceeds the configured limit".into()
        } else {
            "browser response exceeds the configured limit".into()
        });
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("browser response failed: {error}"))?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(if name == "browser_download" {
                "browser download exceeds the configured limit".into()
            } else {
                "browser response exceeds the configured limit".into()
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    if name == "browser_download" {
        return Ok(json!({
            "url": url.to_string(),
            "name": url.path_segments().and_then(|mut segments| segments.next_back()).filter(|name| !name.is_empty()).unwrap_or("browser-download.bin"),
            "mime_type": content_type,
            "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        }));
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok(json!({"url": url.to_string(), "status": status.as_u16(), "content": text}))
}
