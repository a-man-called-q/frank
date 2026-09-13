//! Read-only server inspection commands.

use frank_protocol::HandshakeRequest;

use super::context::{ClientFactory, ServerCommandContext};

#[derive(Debug, Clone, Copy)]
pub(super) enum SnapshotField {
    Settings,
    Projects,
    Agents,
    Missions,
    Tasks,
    Approvals,
    Artifacts,
    Usage,
    Operations,
}

pub(super) async fn health(context: &ServerCommandContext) -> i32 {
    let client = match context.client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    match client.health().await {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
            );
            0
        }
        Err(error) => {
            eprintln!("health check failed: {error}");
            1
        }
    }
}

pub(super) async fn doctor(context: &ServerCommandContext) -> i32 {
    let token = match std::env::var("FRANK_DEVICE_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            eprintln!("runtime doctor requires FRANK_DEVICE_TOKEN for owner authentication");
            return 2;
        }
    };
    let client = match context
        .client_config()
        .with_token(token)
        .pipe(frank_client::RemoteClient::new)
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("cannot connect to server: {error}");
            return 1;
        }
    };
    match client.diagnostics().await {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
            );
            if value.database == frank_protocol::HealthStatus::Unhealthy {
                1
            } else {
                0
            }
        }
        Err(error) => {
            eprintln!("runtime doctor failed: {error}");
            1
        }
    }
}

pub(super) async fn snapshot(context: &ServerCommandContext) -> i32 {
    let client = match context.authenticated_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    match client.snapshot().await {
        Ok(value) => {
            print_value(&value, context.json());
            0
        }
        Err(error) => {
            eprintln!("snapshot failed: {error}");
            1
        }
    }
}

pub(super) async fn snapshot_field(context: &ServerCommandContext, field: SnapshotField) -> i32 {
    let client = match context.authenticated_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let value = match client.snapshot().await {
        Ok(snapshot) => match field {
            SnapshotField::Settings => serde_json::to_value(snapshot.server),
            SnapshotField::Projects => serde_json::to_value(snapshot.projects),
            SnapshotField::Agents => serde_json::to_value(snapshot.agents),
            SnapshotField::Missions => serde_json::to_value(snapshot.missions),
            SnapshotField::Tasks => serde_json::to_value(snapshot.tasks),
            SnapshotField::Approvals => serde_json::to_value(snapshot.approvals),
            SnapshotField::Artifacts => serde_json::to_value(snapshot.artifacts),
            SnapshotField::Usage => serde_json::to_value(snapshot.usage),
            SnapshotField::Operations => serde_json::to_value(snapshot.operations),
        },
        Err(error) => {
            eprintln!("snapshot failed: {error}");
            return 1;
        }
    };
    match value {
        Ok(value) => print_value(&value, context.json()),
        Err(error) => {
            eprintln!("could not encode response: {error}");
            return 1;
        }
    }
    0
}

pub(super) async fn capabilities(context: &ServerCommandContext) -> i32 {
    let client = match context.client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    match client
        .handshake(HandshakeRequest {
            protocol_version: frank_protocol::PROTOCOL_VERSION,
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            client_kind: "frank-cli".to_string(),
            supported_versions: frank_protocol::VersionRange::current(),
        })
        .await
    {
        Ok(value) => {
            print_value(&value.capabilities, context.json());
            0
        }
        Err(error) => {
            eprintln!("capabilities failed: {error}");
            1
        }
    }
}

pub(super) async fn devices(context: &ServerCommandContext) -> i32 {
    let client = match context.authenticated_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    match client.devices().await {
        Ok(value) => {
            print_value(&value, context.json());
            0
        }
        Err(error) => {
            eprintln!("device listing failed: {error}");
            1
        }
    }
}

pub(super) fn print_value<T: serde::Serialize>(value: &T, json: bool) {
    let value = serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!({}));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
        );
        return;
    }
    match value {
        serde_json::Value::Array(values) => {
            if values.is_empty() {
                println!("(none)");
                return;
            }
            for item in values {
                if let serde_json::Value::Object(fields) = item {
                    let id = fields
                        .get("id")
                        .or_else(|| fields.get("device_id"))
                        .or_else(|| fields.get("operation_id"))
                        .map(value_label)
                        .unwrap_or_else(|| "-".into());
                    let label = fields
                        .get("name")
                        .or_else(|| fields.get("display_name"))
                        .or_else(|| fields.get("title"))
                        .or_else(|| fields.get("objective"))
                        .map(value_label)
                        .unwrap_or_else(|| "".into());
                    let status = fields
                        .get("status")
                        .or_else(|| fields.get("state"))
                        .map(value_label)
                        .unwrap_or_default();
                    println!(
                        "{}{}{}",
                        id,
                        if !label.is_empty() {
                            format!("  {label}")
                        } else {
                            String::new()
                        },
                        if !status.is_empty() {
                            format!("  [{status}]")
                        } else {
                            String::new()
                        }
                    );
                } else {
                    println!("{}", value_label(&item));
                }
            }
        }
        serde_json::Value::Object(_) => println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
        ),
        other => println!("{}", value_label(&other)),
    }
}

fn value_label(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Null => "-".into(),
        _ => value.to_string(),
    }
}

// This tiny extension keeps the doctor construction expression readable
// without changing the synchronous client factory boundary.
trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}

impl<T> Pipe for T {}
