use std::env;

use frank_agent_mcp::{ScopedBridge, SessionCapability, run_stdio};
use frank_client::{ClientConfig, RemoteClient};
use frank_protocol::{AgentId, TaskId};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("frank-agent-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let server = env::var("FRANK_SERVER").unwrap_or_else(|_| "https://127.0.0.1:37465".into());
    let agent_id = AgentId::parse(&env::var("FRANK_AGENT_ID")?)?;
    let task_id = TaskId::parse(&env::var("FRANK_TASK_ID")?)?;
    let session_token = env::var("FRANK_AGENT_SESSION_TOKEN")?;
    let mut config = ClientConfig::new(server);
    // Provider sessions normally authenticate with their short-lived scoped
    // capability. A device token remains optional for deployments that choose
    // to inject one through a credential helper, but it is never required or
    // persisted in the provider environment.
    if let Ok(token) = env::var("FRANK_DEVICE_TOKEN") {
        config = config.with_token(token);
    }
    if let Ok(fingerprint) = env::var("FRANK_CERTIFICATE_FINGERPRINT") {
        config = config.with_pin(fingerprint);
    }
    config = config.with_agent_session_token(session_token.clone());
    let client = RemoteClient::new(config)?;
    let capability = SessionCapability::from_token(session_token, agent_id, task_id);
    run_stdio(ScopedBridge { client, capability }).await?;
    Ok(())
}
