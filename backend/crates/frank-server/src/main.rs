use std::net::SocketAddr;
use std::path::PathBuf;

use std::io::{self, BufRead, Write};

use clap::{Parser, Subcommand};
use frank_server::{AuthManager, ServerConfig, ServerState, TlsIdentity, run};
use frank_store::Store;

#[tokio::main]
async fn main() {
    if let Err(error) = run_cli(Cli::parse()).await {
        eprintln!("frankd: {error}");
        std::process::exit(1);
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "frankd",
    version,
    about = "Frank headless remote multi-agent orchestrator"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<DaemonCommand>,
    /// Address to bind. Non-loopback addresses require --cert and --key.
    #[arg(long, global = true, default_value = "127.0.0.1:37465")]
    bind: SocketAddr,
    /// SQLite database path. Defaults to the Frank v1 platform data root.
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    /// PEM certificate for HTTPS. Loopback uses a persisted self-signed
    /// identity when omitted.
    #[arg(long, global = true)]
    cert: Option<PathBuf>,
    /// PEM private key matching --cert.
    #[arg(long, global = true)]
    key: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Manage the single local owner account.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Create the owner account during first-run setup.
    Init {
        /// Optional owner username. The password is always prompted without
        /// echo and is never accepted as an argument or environment value.
        #[arg(long)]
        username: Option<String>,
    },
    /// Replace the owner password and revoke all active sessions.
    ResetPassword,
    /// Revoke every persisted owner session.
    RevokeSessions,
}

async fn run_cli(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let Cli {
        command,
        bind,
        db,
        cert,
        key,
    } = cli;
    let db = db.unwrap_or_else(default_database_path);
    match command {
        Some(DaemonCommand::Auth { command }) => run_auth(command, db).await,
        None => run_daemon(bind, db, cert, key).await,
    }
}

async fn run_daemon(
    bind: SocketAddr,
    db: PathBuf,
    cert_path: Option<PathBuf>,
    key_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tls = match (cert_path, key_path) {
        (Some(cert), Some(key)) => {
            let certificate_pem = std::fs::read(cert)?;
            let private_key_pem = std::fs::read(key)?;
            let fingerprint = TlsIdentity::fingerprint_for_pem(&certificate_pem);
            Some(TlsIdentity {
                certificate_pem,
                private_key_pem,
                fingerprint,
            })
        }
        (None, None) => None,
        _ => return Err("--cert and --key must be supplied together".into()),
    };
    let store = Store::open(db).await?;
    let state = ServerState::new(
        store,
        ServerConfig {
            bind,
            tls,
            ..Default::default()
        },
    )
    .await?;
    eprintln!(
        "frankd listening on {} (server {})",
        state.config.bind,
        state.store.server_id()
    );
    run(state).await?;
    Ok(())
}

async fn run_auth(command: AuthCommand, db: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(&db).await?;
    let auth = AuthManager::new(store.clone());
    match command {
        AuthCommand::Init { username } => {
            eprintln!("Owner account requirements:");
            eprintln!("  Username: 3-32 ASCII characters; letters, numbers, '.', '_' or '-' only.");
            eprintln!("  Password: 15-128 characters.");
            let username = match username {
                Some(username) => username,
                None => prompt_owner_username()?,
            };
            let password = prompt_confirmed_password("New password: ")?;
            let owner = auth.initialize_owner(&username, &password).await?;
            println!(
                "initialized Frank owner '{}' ({})",
                owner.username, owner.id
            );
        }
        AuthCommand::ResetPassword => {
            eprintln!("Password requirement: 15-128 characters.");
            let password = prompt_confirmed_password("New password: ")?;
            auth.reset_owner_password(&password).await?;
            println!("owner password reset; all sessions revoked");
        }
        AuthCommand::RevokeSessions => {
            let count = auth.revoke_sessions_for_owner().await?;
            println!("revoked {count} owner session(s)");
        }
    }
    Ok(())
}

fn prompt_line(label: &str) -> Result<String, io::Error> {
    print!("{label}");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().lock().read_line(&mut value)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn prompt_owner_username() -> Result<String, Box<dyn std::error::Error>> {
    loop {
        let username = prompt_line("Username: ")?;
        match frank_server::normalize_username(&username) {
            Ok(username) => return Ok(username),
            Err(error) => eprintln!("Invalid username: {error}. Try again."),
        }
    }
}

fn prompt_confirmed_password(label: &str) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        let first = prompt_password(label)?;
        if let Err(error) = frank_server::validate_password(&first) {
            eprintln!("Invalid password: {error}. Try again.");
            continue;
        }

        let second = prompt_password("Confirm password: ")?;
        if first != second {
            eprintln!("Passwords do not match. Try again.");
            continue;
        }
        return Ok(first);
    }
}

fn prompt_password(label: &str) -> Result<String, io::Error> {
    print!("{label}");
    io::stdout().flush()?;
    #[cfg(unix)]
    let value = {
        // `stty` is available on the Unix platforms supported by the daemon
        // and lets bootstrap/reset remain dependency-free while ensuring a
        // password is not echoed into a terminal transcript.
        let _ = std::process::Command::new("stty")
            .arg("-echo")
            .stderr(std::process::Stdio::null())
            .status();
        let mut value = String::new();
        let read_result = io::stdin().lock().read_line(&mut value);
        let _ = std::process::Command::new("stty")
            .arg("echo")
            .stderr(std::process::Stdio::null())
            .status();
        println!();
        read_result?;
        value.trim_end_matches(['\r', '\n']).to_owned()
    };
    #[cfg(not(unix))]
    let value = {
        // Keep the command usable on Windows/other targets. Native terminal
        // password masking can be added without changing the auth contract.
        let mut value = String::new();
        io::stdin().lock().read_line(&mut value)?;
        value.trim_end_matches(['\r', '\n']).to_owned()
    };
    Ok(value)
}

fn default_database_path() -> PathBuf {
    let home = frank_safeio::home_dir().unwrap_or_else(|| PathBuf::from("."));
    data_root(&home)
        .join("frank")
        .join("v1")
        .join("frank.sqlite3")
}

fn data_root(home: &std::path::Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Library").join("Application Support"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
    }
}
