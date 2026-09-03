//! TLS identity for the local daemon: loading it from the store, minting a
//! self-signed one on first run, and installing the process-wide crypto
//! provider.
//!
//! Kept apart from the request surface because the certificate fingerprint is
//! also the pairing identity -- regenerating it silently would invalidate every
//! paired device.

use std::io::Cursor;

use frank_store::Store;
use sha2::{Digest, Sha256};

use crate::{Result, ServerError};

#[derive(Debug, Clone)]
pub struct TlsIdentity {
    pub certificate_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
    pub fingerprint: String,
}

impl TlsIdentity {
    pub fn fingerprint_for(certificate_der: &[u8]) -> String {
        let mut digest = Sha256::new();
        digest.update(certificate_der);
        format!("sha256:{}", hex::encode(digest.finalize()))
    }

    /// Compute the pin from the DER certificate contained in a PEM bundle.
    /// Falling back to the raw bytes keeps malformed custom identities
    /// diagnosable while preserving a deterministic fingerprint for them.
    pub fn fingerprint_for_pem(certificate_pem: &[u8]) -> String {
        let mut reader = Cursor::new(certificate_pem);
        if let Some(Ok(certificate)) = rustls_pemfile::certs(&mut reader).next() {
            return Self::fingerprint_for(&certificate);
        }
        Self::fingerprint_for(certificate_pem)
    }
}

/// Rustls 0.23 deliberately refuses to guess when both its `ring` and
/// `aws-lc-rs` backends are present.  Frank uses the aws-lc-rs provider for every
/// HTTPS/WebSocket process; install it before axum-server or a pinned client
/// builder asks Rustls for the process-wide provider.  Calling this more than
/// once is harmless because Rustls returns an error when another thread won
/// the one-time installation race.
pub(crate) fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Load the loopback identity from the versioned server data root, creating it
/// exactly once on first boot.  The certificate fingerprint is a trust anchor
/// for paired clients, so silently generating a new certificate after every
/// restart would revoke every device and create a confusing trust reset.
pub(crate) fn load_or_create_local_identity(store: &Store) -> Result<TlsIdentity> {
    let Some(data_root) = store.database_path().and_then(std::path::Path::parent) else {
        return generate_local_identity();
    };
    frank_safeio::ensure_dir(data_root).map_err(|error| ServerError::Tls(error.to_string()))?;
    let certificate_path = data_root.join("server-cert.pem");
    let key_path = data_root.join("server-key.pem");
    let certificate_exists = std::fs::symlink_metadata(&certificate_path).is_ok();
    let key_exists = std::fs::symlink_metadata(&key_path).is_ok();
    match (certificate_exists, key_exists) {
        (true, true) => {
            for path in [&certificate_path, &key_path] {
                if std::fs::symlink_metadata(path)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    return Err(ServerError::Tls(
                        "persisted TLS identity may not be a symlink".into(),
                    ));
                }
            }
            let certificate_pem =
                frank_safeio::read_text_capped(&certificate_path, frank_safeio::MAX_CONFIG_BYTES)
                    .map_err(|error| ServerError::Tls(error.to_string()))?
                    .into_bytes();
            let private_key_pem =
                frank_safeio::read_text_capped(&key_path, frank_safeio::MAX_CONFIG_BYTES)
                    .map_err(|error| ServerError::Tls(error.to_string()))?
                    .into_bytes();
            if certificate_pem.is_empty() || private_key_pem.is_empty() {
                return Err(ServerError::Tls(
                    "persisted TLS identity is empty".to_string(),
                ));
            }
            // Harden identities created by older development builds as well
            // as newly generated keys. A key that was accidentally left
            // group/world-readable must not remain usable merely because it
            // already exists on disk.
            set_private_key_permissions(&key_path)?;
            Ok(TlsIdentity {
                fingerprint: TlsIdentity::fingerprint_for_pem(&certificate_pem),
                certificate_pem,
                private_key_pem,
            })
        }
        (false, false) => {
            let identity = generate_local_identity()?;
            frank_safeio::write_text_atomic(
                &certificate_path,
                &String::from_utf8_lossy(&identity.certificate_pem),
                frank_safeio::MAX_CONFIG_BYTES,
            )
            .map_err(|error| ServerError::Tls(error.to_string()))?;
            frank_safeio::write_text_atomic(
                &key_path,
                &String::from_utf8_lossy(&identity.private_key_pem),
                frank_safeio::MAX_CONFIG_BYTES,
            )
            .map_err(|error| ServerError::Tls(error.to_string()))?;
            set_private_key_permissions(&key_path)?;
            Ok(identity)
        }
        _ => Err(ServerError::Tls(
            "persisted TLS certificate and private key must be provided together".to_string(),
        )),
    }
}

pub(crate) fn generate_local_identity() -> Result<TlsIdentity> {
    let certificate = rcgen::generate_simple_self_signed(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .map_err(|error| ServerError::Tls(error.to_string()))?;
    let certificate_pem = certificate
        .serialize_pem()
        .map_err(|error| ServerError::Tls(error.to_string()))?
        .into_bytes();
    Ok(TlsIdentity {
        fingerprint: TlsIdentity::fingerprint_for_pem(&certificate_pem),
        certificate_pem,
        private_key_pem: certificate.serialize_private_key_pem().into_bytes(),
    })
}

pub(crate) fn set_private_key_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(ServerError::Io)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
