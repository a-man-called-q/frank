//! Per-user service installation and lifecycle commands.
//!
//! The service controller is deliberately an internal trait. Descriptor
//! rendering remains owned by `frank-app`; this module only coordinates the
//! platform controller and the safe descriptor write/rollback boundary.

use std::path::{Path, PathBuf};

use frank_app::service::{self, ServiceSpec};

#[derive(Debug, Clone, Copy)]
pub(super) enum ServiceAction {
    Status,
    Logs,
    Restart,
    Uninstall,
}

/// Platform-independent lifecycle seam. The production implementation below
/// retains the existing systemctl/launchctl/schtasks behavior, while a fake
/// controller can exercise install rollback and command mapping without
/// starting a host service in tests.
pub(crate) trait ServiceController {
    fn activate(&self, service_name: &str, descriptor: &Path) -> Result<(), String>;

    fn execute(&self, service_name: &str, action: ServiceAction) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PlatformServiceController;

impl ServiceController for PlatformServiceController {
    fn activate(&self, service_name: &str, descriptor: &Path) -> Result<(), String> {
        activate_service(service_name, descriptor)
    }

    fn execute(&self, service_name: &str, action: ServiceAction) -> Result<(), String> {
        run_service_command(service_name, action)
    }
}

pub(super) fn install_service(
    output: Option<PathBuf>,
    executable: Option<PathBuf>,
    database: Option<PathBuf>,
    bind: &str,
    preview: bool,
) -> i32 {
    let controller = PlatformServiceController;
    install_service_with_controller(output, executable, database, bind, preview, &controller)
}

fn install_service_with_controller<C: ServiceController + ?Sized>(
    output: Option<PathBuf>,
    executable: Option<PathBuf>,
    database: Option<PathBuf>,
    bind: &str,
    preview: bool,
    controller: &C,
) -> i32 {
    let home = frank_safeio::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let database = database.unwrap_or_else(|| {
        super::context::data_root_base(&home)
            .join("frank")
            .join("v1")
            .join("frank.sqlite3")
    });
    let executable = executable.unwrap_or_else(|| PathBuf::from("frankd"));
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
            match controller.activate(service::SERVICE_NAME, &output) {
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

pub(super) fn service_control(action: ServiceAction) -> i32 {
    let controller = PlatformServiceController;
    service_control_with_controller(action, &controller)
}

fn service_control_with_controller<C: ServiceController + ?Sized>(
    action: ServiceAction,
    controller: &C,
) -> i32 {
    let service_name = service::SERVICE_NAME;
    if matches!(action, ServiceAction::Uninstall) {
        return match controller.execute(service_name, ServiceAction::Uninstall) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("could not uninstall frankd service: {error}");
                1
            }
        };
    }
    match controller.execute(service_name, action) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("frankd service command failed: {error}");
            1
        }
    }
}

fn restore_service_descriptor(path: &Path, previous: Option<String>) -> Result<(), String> {
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

fn activate_service(service_name: &str, descriptor: &Path) -> Result<(), String> {
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
    // Removing a descriptor is part of uninstall. Preserve a useful service
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
    // The function is only used by the macOS launchd branch. Keeping a
    // platform-neutral fallback lets the CLI cross-compile for Windows.
    "0".into()
}
