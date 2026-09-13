//! Authenticated update commands.

use frank_protocol::{Command, ErrorCode, UpdateId};

use super::context::{ClientFactory, ServerCommandContext};
use super::inspection::print_value;

#[derive(Debug, Clone)]
pub(super) enum UpdateAction {
    Check,
    Prepare(String),
    Apply(String),
    Rollback,
}

pub(super) async fn run(context: &ServerCommandContext, action: UpdateAction) -> i32 {
    let client = match context.authenticated_client() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let command = match action {
        UpdateAction::Check => Command::CheckForUpdate,
        UpdateAction::Prepare(version) => Command::PrepareUpdate { version },
        UpdateAction::Apply(update_id) => match UpdateId::parse(&update_id) {
            Ok(update_id) => Command::ApplyUpdate { update_id },
            Err(_) => {
                eprintln!("update id is not a UUID");
                return 2;
            }
        },
        UpdateAction::Rollback => Command::RollbackUpdate,
    };
    match client.command(command, None).await {
        Ok(response) => {
            if let Some(result) = response.result {
                print_value(&result, context.json());
            } else {
                print_value(&response, context.json());
            }
            0
        }
        Err(error) => {
            eprintln!("update command failed: {error}");
            if matches!(error, frank_client::ClientError::Api(ref api) if api.code == ErrorCode::Forbidden)
            {
                2
            } else {
                1
            }
        }
    }
}
