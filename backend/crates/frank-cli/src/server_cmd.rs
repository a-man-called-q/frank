//! `frank server` command dispatch.
//!
//! The public command envelope stays here because `main` constructs it. The
//! implementation is split by concern so transport setup, read-only
//! inspection, updates, pairing, and service lifecycle code do not grow one
//! exhaustive function into another.

mod context;
mod inspection;
mod pairing;
mod service;
mod update;

use std::path::PathBuf;

use context::ServerCommandContext;

#[derive(Debug, Clone)]
pub struct ServerArgs {
    pub command: ServerSubcommand,
}

#[derive(Debug, Clone)]
pub enum ServerSubcommand {
    Pair {
        address: String,
        role: String,
        device_name: String,
        insecure_local: bool,
        json: bool,
    },
    Health {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Doctor {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Snapshot {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Capabilities {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Devices {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Settings {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Projects {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Agents {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Missions {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Tasks {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Approvals {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Artifacts {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Ledger {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    Operations {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    RuntimeDoctor {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    UpdateCheck {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    UpdatePrepare {
        version: String,
        address: String,
        insecure_local: bool,
        json: bool,
    },
    UpdateApply {
        update_id: String,
        address: String,
        insecure_local: bool,
        json: bool,
    },
    UpdateRollback {
        address: String,
        insecure_local: bool,
        json: bool,
    },
    InstallService {
        output: Option<PathBuf>,
        executable: Option<PathBuf>,
        database: Option<PathBuf>,
        bind: String,
        preview: bool,
    },
    ServiceStatus,
    ServiceLogs,
    ServiceRestart,
    UninstallService,
}

pub fn run(args: ServerArgs) -> i32 {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to start async runtime: {error}");
            return 1;
        }
    };
    runtime.block_on(async move {
        match args.command {
            ServerSubcommand::Pair {
                address,
                role,
                device_name,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                pairing::run(&context, &role, &device_name).await
            }
            ServerSubcommand::Health {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::health(&context).await
            }
            ServerSubcommand::Doctor {
                address,
                insecure_local,
                json,
            }
            | ServerSubcommand::RuntimeDoctor {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::doctor(&context).await
            }
            ServerSubcommand::Snapshot {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot(&context).await
            }
            ServerSubcommand::Capabilities {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::capabilities(&context).await
            }
            ServerSubcommand::Devices {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::devices(&context).await
            }
            ServerSubcommand::Settings {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Settings).await
            }
            ServerSubcommand::Projects {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Projects).await
            }
            ServerSubcommand::Agents {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Agents).await
            }
            ServerSubcommand::Missions {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Missions).await
            }
            ServerSubcommand::Tasks {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Tasks).await
            }
            ServerSubcommand::Approvals {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Approvals).await
            }
            ServerSubcommand::Artifacts {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Artifacts).await
            }
            ServerSubcommand::Ledger {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Usage).await
            }
            ServerSubcommand::Operations {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                inspection::snapshot_field(&context, inspection::SnapshotField::Operations).await
            }
            ServerSubcommand::UpdateCheck {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                update::run(&context, update::UpdateAction::Check).await
            }
            ServerSubcommand::UpdatePrepare {
                version,
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                update::run(&context, update::UpdateAction::Prepare(version)).await
            }
            ServerSubcommand::UpdateApply {
                update_id,
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                update::run(&context, update::UpdateAction::Apply(update_id)).await
            }
            ServerSubcommand::UpdateRollback {
                address,
                insecure_local,
                json,
            } => {
                let context = ServerCommandContext::new(&address, insecure_local, json);
                update::run(&context, update::UpdateAction::Rollback).await
            }
            ServerSubcommand::InstallService {
                output,
                executable,
                database,
                bind,
                preview,
            } => service::install_service(output, executable, database, &bind, preview),
            ServerSubcommand::ServiceStatus => {
                service::service_control(service::ServiceAction::Status)
            }
            ServerSubcommand::ServiceLogs => service::service_control(service::ServiceAction::Logs),
            ServerSubcommand::ServiceRestart => {
                service::service_control(service::ServiceAction::Restart)
            }
            ServerSubcommand::UninstallService => {
                service::service_control(service::ServiceAction::Uninstall)
            }
        }
    })
}
