//! Device pairing command.

use frank_protocol::{DeviceRole, ErrorCode};

use super::context::{ClientFactory, ServerCommandContext};

pub(super) async fn run(context: &ServerCommandContext, role: &str, device_name: &str) -> i32 {
    let role = match parse_role(role) {
        Ok(role) => role,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let client = match context.client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    let ticket = match client.prepare_pairing(role).await {
        Ok(ticket) => ticket,
        Err(frank_client::ClientError::Api(ref error))
            if error.code == ErrorCode::PairingDisabled =>
        {
            eprintln!(
                "device pairing is disabled; initialize the owner with `frankd auth init` and log in with the owner username/password"
            );
            return 1;
        }
        Err(error) => {
            eprintln!("cannot create pairing ticket: {error}");
            return 1;
        }
    };
    if context.json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "address": context.address(),
                "device_name": device_name,
                "role": role_name(role),
                "ticket": ticket,
            }))
            .unwrap_or_else(|_| "{}".into())
        );
        return 0;
    }
    println!("Frank pairing ticket for {device_name}");
    println!("address: {}", context.address());
    println!("role: {}", role_name(role));
    println!(
        "secret: {}",
        ticket
            .get("secret")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>")
    );
    println!(
        "certificate fingerprint: {}",
        ticket
            .get("certificate_fingerprint")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>")
    );
    println!(
        "expires at: {}",
        ticket
            .get("expires_at")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>")
    );
    println!(
        "Enter the address, secret, and fingerprint in the Frank desktop client to finish pairing."
    );
    0
}

fn parse_role(value: &str) -> Result<DeviceRole, String> {
    match value.to_ascii_lowercase().as_str() {
        "owner" => Ok(DeviceRole::Owner),
        "operator" => Ok(DeviceRole::Operator),
        "observer" => Ok(DeviceRole::Observer),
        _ => Err("role must be owner, operator, or observer".into()),
    }
}

fn role_name(role: DeviceRole) -> &'static str {
    match role {
        DeviceRole::Owner => "owner",
        DeviceRole::Operator => "operator",
        DeviceRole::Observer => "observer",
    }
}
