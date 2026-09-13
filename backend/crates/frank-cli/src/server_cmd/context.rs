//! Shared context and client construction for `frank server` commands.
//!
//! Keeping address, transport policy, output mode, and credential lookup in
//! one value makes the command modules independent of clap's large enum while
//! preserving the exact client configuration used by the original command
//! handlers.

use frank_client::{ClientConfig, RemoteClient};

#[derive(Debug, Clone)]
pub(crate) struct ServerCommandContext {
    address: String,
    insecure_local: bool,
    json: bool,
}

impl ServerCommandContext {
    pub(crate) fn new(address: &str, insecure_local: bool, json: bool) -> Self {
        Self {
            address: address.to_owned(),
            insecure_local,
            json,
        }
    }

    pub(crate) fn address(&self) -> &str {
        &self.address
    }

    pub(crate) fn json(&self) -> bool {
        self.json
    }

    pub(crate) fn client_config(&self) -> ClientConfig {
        client_config(&self.address, self.insecure_local)
    }
}

/// Construction is a seam for command handlers and focused tests. The
/// production implementation remains the context itself so all commands use
/// the same CA, loopback, and token policy.
pub(crate) trait ClientFactory {
    fn client(&self) -> Result<RemoteClient, String>;

    fn authenticated_client(&self) -> Result<RemoteClient, String>;
}

impl ClientFactory for ServerCommandContext {
    fn client(&self) -> Result<RemoteClient, String> {
        RemoteClient::new(self.client_config())
            .map_err(|error| format!("cannot connect to server: {error}"))
    }

    fn authenticated_client(&self) -> Result<RemoteClient, String> {
        let token = std::env::var("FRANK_DEVICE_TOKEN").map_err(|_| {
            "this command requires FRANK_DEVICE_TOKEN for device authentication".to_string()
        })?;
        let mut config = self.client_config().with_token(token);
        if self.insecure_local {
            config = config.allow_insecure_local();
        }
        RemoteClient::new(config).map_err(|error| format!("cannot connect to server: {error}"))
    }
}

fn client_config(address: &str, insecure_local: bool) -> ClientConfig {
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
    if insecure_local {
        config = config.allow_insecure_local();
    }
    config
}

pub(super) fn data_root_base(home: &std::path::Path) -> std::path::PathBuf {
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
