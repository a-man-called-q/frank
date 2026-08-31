use frank_app::service::{self, ServiceSpec};
use frank_client::{ClientConfig, RemoteClient};
use frank_protocol::{Command, DeviceRole, ErrorCode, UpdateId};

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
        output: Option<std::path::PathBuf>,
        executable: Option<std::path::PathBuf>,
        database: Option<std::path::PathBuf>,
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
            } => pair(&address, &role, &device_name, insecure_local, json).await,
            ServerSubcommand::Health {
                address,
                insecure_local,
                json,
            } => health(&address, insecure_local, json).await,
            ServerSubcommand::Doctor {
                address,
                insecure_local,
                json,
            } => doctor(&address, insecure_local, json).await,
            ServerSubcommand::RuntimeDoctor {
                address,
                insecure_local,
                json,
            } => doctor(&address, insecure_local, json).await,
            ServerSubcommand::Snapshot {
                address,
                insecure_local,
                json,
            } => snapshot(&address, insecure_local, json).await,
            ServerSubcommand::Capabilities {
                address,
                insecure_local,
                json,
            } => capabilities(&address, insecure_local, json).await,
            ServerSubcommand::Devices {
                address,
                insecure_local,
                json,
            } => devices(&address, insecure_local, json).await,
            ServerSubcommand::Settings {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Settings).await,
            ServerSubcommand::Projects {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Projects).await,
            ServerSubcommand::Agents {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Agents).await,
            ServerSubcommand::Missions {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Missions).await,
            ServerSubcommand::Tasks {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Tasks).await,
            ServerSubcommand::Approvals {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Approvals).await,
            ServerSubcommand::Artifacts {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Artifacts).await,
            ServerSubcommand::Ledger {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Usage).await,
            ServerSubcommand::Operations {
                address,
                insecure_local,
                json,
            } => snapshot_field(&address, insecure_local, json, SnapshotField::Operations).await,
            ServerSubcommand::UpdateCheck {
                address,
                insecure_local,
                json,
            } => update_command(&address, insecure_local, json, UpdateAction::Check).await,
            ServerSubcommand::UpdatePrepare {
                version,
                address,
                insecure_local,
                json,
            } => {
                update_command(
                    &address,
                    insecure_local,
                    json,
                    UpdateAction::Prepare(version),
                )
                .await
            }
            ServerSubcommand::UpdateApply {
                update_id,
                address,
                insecure_local,
                json,
            } => {
                update_command(
                    &address,
                    insecure_local,
                    json,
                    UpdateAction::Apply(update_id),
                )
                .await
            }
            ServerSubcommand::UpdateRollback {
                address,
                insecure_local,
                json,
            } => update_command(&address, insecure_local, json, UpdateAction::Rollback).await,
            ServerSubcommand::InstallService {
                output,
                executable,
                database,
                bind,
                preview,
            } => install_service(output, executable, database, &bind, preview),
            ServerSubcommand::ServiceStatus => service_control(ServiceAction::Status),
            ServerSubcommand::ServiceLogs => service_control(ServiceAction::Logs),
            ServerSubcommand::ServiceRestart => service_control(ServiceAction::Restart),
            ServerSubcommand::UninstallService => service_control(ServiceAction::Uninstall),
        }
    })
}

async fn pair(
    address: &str,
    role: &str,
    device_name: &str,
    insecure_local: bool,
    json: bool,
) -> i32 {
    let role = match parse_role(role) {
        Ok(role) => role,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let mut config = client_config(address);
    if insecure_local {
        config = config.allow_insecure_local();
    }
    let client = match RemoteClient::new(config) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("cannot connect to server: {error}");
            return 1;
        }
    };
    let ticket = match client.prepare_pairing(role).await {
        Ok(ticket) => ticket,
        Err(error) => {
            eprintln!("cannot create pairing ticket: {error}");
            return 1;
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "address": address,
                "device_name": device_name,
                "role": role_name(role),
                "ticket": ticket,
            }))
            .unwrap_or_else(|_| "{}".into())
        );
        return 0;
    }
    println!("Frank pairing ticket for {device_name}");
    println!("address: {address}");
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

async fn health(address: &str, insecure_local: bool, _json: bool) -> i32 {
    let mut config = client_config(address);
    if insecure_local {
        config = config.allow_insecure_local();
    }
    let client = match RemoteClient::new(config) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("cannot connect to server: {error}");
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

async fn doctor(address: &str, insecure_local: bool, _json: bool) -> i32 {
    let mut config = client_config(address);
    if let Ok(token) = std::env::var("FRANK_DEVICE_TOKEN") {
        config = config.with_token(token);
    } else {
        eprintln!("runtime doctor requires FRANK_DEVICE_TOKEN for owner authentication");
        return 2;
    }
    if insecure_local {
        config = config.allow_insecure_local();
    }
    let client = match RemoteClient::new(config) {
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

/// Build an authenticated client for remote administration commands. The CLI
/// intentionally accepts the short-lived device token only through the
/// process environment; it never prints or persists bearer credentials. GUI
/// users use the OS credential store through `frank-client` instead.
fn authenticated_client(address: &str, insecure_local: bool) -> Result<RemoteClient, String> {
    let token = std::env::var("FRANK_DEVICE_TOKEN").map_err(|_| {
        "this command requires FRANK_DEVICE_TOKEN for device authentication".to_string()
    })?;
    let mut config = client_config(address).with_token(token);
    if insecure_local {
        config = config.allow_insecure_local();
    }
    RemoteClient::new(config).map_err(|error| format!("cannot connect to server: {error}"))
}

fn print_value<T: serde::Serialize>(value: &T, json: bool) {
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

#[derive(Debug, Clone, Copy)]
enum SnapshotField {
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

async fn snapshot(address: &str, insecure_local: bool, json: bool) -> i32 {
    let client = match authenticated_client(address, insecure_local) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    match client.snapshot().await {
        Ok(value) => {
            print_value(&value, json);
            0
        }
        Err(error) => {
            eprintln!("snapshot failed: {error}");
            1
        }
    }
}

async fn snapshot_field(
    address: &str,
    insecure_local: bool,
    json: bool,
    field: SnapshotField,
) -> i32 {
    let client = match authenticated_client(address, insecure_local) {
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
        Ok(value) => print_value(&value, json),
        Err(error) => {
            eprintln!("could not encode response: {error}");
            return 1;
        }
    }
    0
}

async fn capabilities(address: &str, insecure_local: bool, json: bool) -> i32 {
    let mut config = client_config(address);
    if insecure_local {
        config = config.allow_insecure_local();
    }
    let client = match RemoteClient::new(config) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("cannot connect to server: {error}");
            return 1;
        }
    };
    match client
        .handshake(frank_protocol::HandshakeRequest {
            protocol_version: frank_protocol::PROTOCOL_VERSION,
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            client_kind: "frank-cli".to_string(),
            supported_versions: frank_protocol::VersionRange::current(),
        })
        .await
    {
        Ok(value) => {
            print_value(&value.capabilities, json);
            0
        }
        Err(error) => {
            eprintln!("capabilities failed: {error}");
            1
        }
    }
}

async fn devices(address: &str, insecure_local: bool, json: bool) -> i32 {
    let client = match authenticated_client(address, insecure_local) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    match client.devices().await {
        Ok(value) => {
            print_value(&value, json);
            0
        }
        Err(error) => {
            eprintln!("device listing failed: {error}");
            1
        }
    }
}

enum UpdateAction {
    Check,
    Prepare(String),
    Apply(String),
    Rollback,
}

async fn update_command(
    address: &str,
    insecure_local: bool,
    json: bool,
    action: UpdateAction,
) -> i32 {
    let client = match authenticated_client(address, insecure_local) {
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
                print_value(&result, json);
            } else {
                print_value(&response, json);
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

fn client_config(address: &str) -> ClientConfig {
    let mut config = ClientConfig::new(address);
    if let Ok(path) = std::env::var("FRANK_CA_CERTIFICATE") {
        if let Ok(pem) = std::fs::read(path) {
            config = config.with_ca_certificate_pem(pem);
        }
    } else if is_loopback(address)
        && let Some(home) = frank_safeio::home_dir()
    {
        let path = data_root_base(&home)
            .join("frank")
            .join("v1")
            .join("server-cert.pem");
        if let Ok(pem) = std::fs::read(path) {
            config = config.with_ca_certificate_pem(pem);
        }
    }
    config
}

fn data_root_base(home: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join("Library").join("Application Support"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
    }
}

fn is_loopback(address: &str) -> bool {
    address
        .split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .and_then(|host| host.rsplit_once(':').map(|(host, _)| host).or(Some(host)))
        .is_some_and(|host| {
            matches!(
                host.trim_matches(|character| character == '[' || character == ']'),
                "127.0.0.1" | "localhost" | "::1"
            )
        })
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

fn install_service(
    output: Option<std::path::PathBuf>,
    executable: Option<std::path::PathBuf>,
    database: Option<std::path::PathBuf>,
    bind: &str,
    preview: bool,
) -> i32 {
    let home = frank_safeio::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let database = database.unwrap_or_else(|| {
        data_root_base(&home)
            .join("frank")
            .join("v1")
            .join("frank.sqlite3")
    });
    let executable = executable.unwrap_or_else(|| std::path::PathBuf::from("frankd"));
    if let Err(error) = service::validate_executable_path(&executable) {
        eprintln!("invalid frankd executable: {error}");
        return 2;
    }
    let spec = ServiceSpec {
        executable,
        database,
        bind: bind.to_string(),
        service_name: service::SERVICE_NAME.to_string(),
    };
    let output = output.unwrap_or_else(service::default_descriptor_path);
    let Some(root) = output.parent() else {
        eprintln!("service descriptor path has no parent");
        return 2;
    };
    let rendered = match service::descriptor(&spec, root) {
        Ok((path, content)) if path == output => content,
        Ok(_) => {
            eprintln!("service descriptor path does not match requested output");
            return 2;
        }
        Err(error) => {
            eprintln!("invalid service configuration: {error}");
            return 2;
        }
    };
    if std::fs::symlink_metadata(&output)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        eprintln!("refusing to overwrite a symlinked service descriptor");
        return 2;
    }
    if preview {
        println!("service descriptor preview: {}", output.display());
        print!("{rendered}");
        return 0;
    }
    let previous = match frank_safeio::read_text_capped(&output, frank_safeio::MAX_CONFIG_BYTES) {
        Ok(content) => Some(content),
        Err(frank_safeio::SafeIoError::Io(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            None
        }
        Err(error) => {
            eprintln!("could not read existing service descriptor: {error}");
            return 1;
        }
    };
    match frank_safeio::ensure_dir(root).and_then(|_| {
        frank_safeio::write_text_atomic(&output, &rendered, frank_safeio::MAX_CONFIG_BYTES)
    }) {
        Ok(()) => {
            println!(
                "wrote per-user frankd service descriptor: {}",
                output.display()
            );
            if std::env::var_os("FRANK_SERVICE_PREVIEW").is_some() {
                println!("FRANK_SERVICE_PREVIEW is set; service activation skipped");
                return 0;
            }
            match activate_service(service::SERVICE_NAME, &output) {
                Ok(()) => {
                    println!(
                        "frankd per-user service installed and started: {}",
                        service::SERVICE_NAME
                    );
                    0
                }
                Err(error) => {
                    eprintln!("descriptor written but service activation failed: {error}");
                    if let Err(rollback_error) = restore_service_descriptor(&output, previous) {
                        eprintln!("service descriptor rollback failed: {rollback_error}");
                    } else {
                        eprintln!("service descriptor rolled back after activation failure");
                    }
                    1
                }
            }
        }
        Err(error) => {
            eprintln!("could not write service descriptor: {error}");
            1
        }
    }
}

fn restore_service_descriptor(
    path: &std::path::Path,
    previous: Option<String>,
) -> Result<(), String> {
    match previous {
        Some(content) => {
            frank_safeio::write_text_atomic(path, &content, frank_safeio::MAX_CONFIG_BYTES)
                .map_err(|error| error.to_string())
        }
        None => match frank_safeio::remove_file_if_contains(path, service::SERVICE_NAME) {
            Ok(true) => Ok(()),
            Ok(false) => Err("service descriptor was replaced before rollback".into()),
            Err(error) => Err(error.to_string()),
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum ServiceAction {
    Status,
    Logs,
    Restart,
    Uninstall,
}

fn service_control(action: ServiceAction) -> i32 {
    let service_name = service::SERVICE_NAME;
    if matches!(action, ServiceAction::Uninstall) {
        return match run_service_command(service_name, ServiceAction::Uninstall) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("could not uninstall frankd service: {error}");
                1
            }
        };
    }
    match run_service_command(service_name, action) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("frankd service command failed: {error}");
            1
        }
    }
}

fn activate_service(service_name: &str, descriptor: &std::path::Path) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        run_command("systemctl", &["--user", "daemon-reload"])?;
        run_command("systemctl", &["--user", "enable", "--now", service_name])
    } else if cfg!(target_os = "macos") {
        let uid = current_uid();
        run_command(
            "launchctl",
            &[
                "bootstrap",
                &format!("gui/{uid}"),
                &descriptor.to_string_lossy(),
            ],
        )
    } else if cfg!(target_os = "windows") {
        run_command("cmd", &["/C", &descriptor.to_string_lossy()])
    } else {
        Err("unsupported operating system service controller".into())
    }
}

fn run_service_command(service_name: &str, action: ServiceAction) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        let args: Vec<&str> = match action {
            ServiceAction::Status => vec!["--user", "status", service_name, "--no-pager"],
            ServiceAction::Logs => vec!["--user", "-u", service_name, "-n", "100", "--no-pager"],
            ServiceAction::Restart => vec!["--user", "restart", service_name],
            ServiceAction::Uninstall => vec!["--user", "disable", "--now", service_name],
        };
        let program = if matches!(action, ServiceAction::Logs) {
            "journalctl"
        } else {
            "systemctl"
        };
        let result = run_command(program, &args);
        if matches!(action, ServiceAction::Uninstall) {
            remove_service_descriptor(result)
        } else {
            result
        }
    } else if cfg!(target_os = "macos") {
        let uid = current_uid();
        let domain = format!("gui/{uid}");
        match action {
            ServiceAction::Status => {
                run_command("launchctl", &["print", &format!("{domain}/{service_name}")])
            }
            ServiceAction::Logs => run_command(
                "log",
                &[
                    "show",
                    "--last",
                    "10m",
                    "--predicate",
                    "process == 'frankd'",
                ],
            ),
            ServiceAction::Restart => run_command(
                "launchctl",
                &["kickstart", "-k", &format!("{domain}/{service_name}")],
            ),
            ServiceAction::Uninstall => {
                let result = run_command(
                    "launchctl",
                    &["bootout", &format!("{domain}/{service_name}")],
                );
                remove_service_descriptor(result)
            }
        }
    } else if cfg!(target_os = "windows") {
        match action {
            ServiceAction::Status => run_command("schtasks.exe", &["/Query", "/TN", service_name]),
            ServiceAction::Logs => run_command(
                "wevtutil.exe",
                &[
                    "qe",
                    "Application",
                    "/q:*[System[(Provider[@Name='frankd'])]]",
                    "/c:100",
                    "/f:text",
                ],
            ),
            ServiceAction::Restart => run_command("schtasks.exe", &["/Run", "/TN", service_name]),
            ServiceAction::Uninstall => {
                let result = run_command("schtasks.exe", &["/Delete", "/TN", service_name, "/F"]);
                remove_service_descriptor(result)
            }
        }
    } else {
        Err("unsupported operating system service controller".into())
    }
}

fn remove_service_descriptor(controller_result: Result<(), String>) -> Result<(), String> {
    // Removing a descriptor is part of uninstall.  Preserve a useful service
    // controller error, but still attempt cleanup so a failed/absent service
    // cannot leave a stale autostart definition behind.
    let path = service::default_descriptor_path();
    let cleanup = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not inspect service descriptor: {error}")),
        Ok(_) => match frank_safeio::remove_file_if_contains(&path, service::SERVICE_NAME) {
            Ok(true) => Ok(()),
            Ok(false) => Err(
                "refusing to remove service descriptor: file is not a Frank-managed descriptor"
                    .into(),
            ),
            Err(error) => Err(format!(
                "could not remove service descriptor safely: {error}"
            )),
        },
    };
    controller_result.and(cleanup)
}

fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if output.status.success() {
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(unix)]
fn current_uid() -> String {
    // SAFETY: getuid has no preconditions and does not mutate process state.
    unsafe { libc::getuid() }.to_string()
}

#[cfg(not(unix))]
fn current_uid() -> String {
    // The function is only used by the macOS launchd branch.  Keeping a
    // platform-neutral fallback lets the CLI cross-compile for Windows.
    "0".into()
}
