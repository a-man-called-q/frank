use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use frank_server::{ServerConfig, ServerState, TlsIdentity, run};
use frank_store::Store;

#[tokio::main]
async fn main() {
    if let Err(error) = run_daemon(Cli::parse()).await {
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
    /// Address to bind. Non-loopback addresses require --cert and --key.
    #[arg(long, default_value = "127.0.0.1:37465")]
    bind: SocketAddr,
    /// SQLite database path. Defaults to the Frank v1 platform data root.
    #[arg(long)]
    db: Option<PathBuf>,
    /// PEM certificate for HTTPS. Loopback uses a persisted self-signed
    /// identity when omitted.
    #[arg(long)]
    cert: Option<PathBuf>,
    /// PEM private key matching --cert.
    #[arg(long)]
    key: Option<PathBuf>,
}

async fn run_daemon(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let bind = cli.bind;
    let db = cli.db.unwrap_or_else(default_database_path);
    let cert_path = cli.cert;
    let key_path = cli.key;
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
