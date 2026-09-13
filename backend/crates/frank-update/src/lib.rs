//! Signed, target-aware update metadata for Frank.
//!
//! The operating-system package is intentionally not treated as a trust root:
//! the detached Ed25519 signature is checked before a manifest is parsed or
//! staged. The detached file uses the minisign packet layout (and the
//! conventional `.minisig` suffix), while the release helper remains
//! dependency-free and keeps the private key outside the repository.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
/// Hard upper bound shared with the wire protocol.  Update metadata is
/// untrusted input, so reject an artifact that could otherwise force a
/// staging helper to allocate more than the daemon accepts for artifacts.
pub const MAX_UPDATE_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
pub const UPDATE_FEED_URL: &str =
    "https://github.com/a-man-called-q/frank/releases/latest/download/frank-update-v1.json";
pub const UPDATE_SIGNATURE_URL: &str =
    "https://github.com/a-man-called-q/frank/releases/latest/download/frank-update-v1.json.minisig";

/// Public key embedded in released binaries. Release automation may replace
/// this value at compile time for a rotated key, but a non-zero key is kept in
/// source so accidental unsigned/development artifacts fail closed.
const DEVELOPMENT_PUBLIC_KEY_B64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";

/// The release pipeline supplies `FRANK_UPDATE_PUBLIC_KEY_B64` while
/// compiling every Frank binary. Development builds retain a deterministic
/// fixture key, but a published manifest must use the same public key that is
/// embedded in the binaries it accompanies.
pub const EMBEDDED_PUBLIC_KEY_B64: &str = match option_env!("FRANK_UPDATE_PUBLIC_KEY_B64") {
    Some(value) if !value.is_empty() => value,
    _ => DEVELOPMENT_PUBLIC_KEY_B64,
};

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_SIGNATURE_BYTES: usize = 64 * 1024;

/// Network and local-artifact boundary for self-updates.
///
/// The orchestrator owns durable operation state, while this trait owns all
/// untrusted I/O: bounded downloads, detached signature verification, and
/// artifact digest/size verification. Keeping the boundary here makes update
/// flows deterministic in air-gapped tests and prevents a second HTTP client
/// from appearing in the daemon.
#[async_trait]
pub trait UpdateSource: Send + Sync {
    async fn fetch_verified_manifest(&self) -> Result<UpdateManifest>;
    async fn download_verified_artifact(&self, artifact: &UpdateArtifact) -> Result<Vec<u8>>;
}

#[derive(Clone)]
pub struct HttpUpdateSource {
    client: reqwest::Client,
    manifest_url: String,
    signature_url: String,
    public_key: Vec<u8>,
    network_enabled: bool,
    local_manifest: Option<PathBuf>,
    local_signature: Option<PathBuf>,
    local_artifact: Option<PathBuf>,
}

impl HttpUpdateSource {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(300))
            .user_agent(format!("frank/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        let public_key = base64::engine::general_purpose::STANDARD
            .decode(EMBEDDED_PUBLIC_KEY_B64)
            .map_err(|_| UpdateError::InvalidSignature)?;
        Ok(Self {
            client,
            manifest_url: UPDATE_FEED_URL.into(),
            signature_url: UPDATE_SIGNATURE_URL.into(),
            public_key,
            network_enabled: std::env::var_os("FRANK_UPDATE_DISABLE_NETWORK").is_none(),
            local_manifest: std::env::var_os("FRANK_UPDATE_MANIFEST").map(PathBuf::from),
            local_signature: std::env::var_os("FRANK_UPDATE_SIGNATURE").map(PathBuf::from),
            local_artifact: std::env::var_os("FRANK_UPDATE_ARTIFACT").map(PathBuf::from),
        })
    }

    pub fn with_local_manifest(
        mut self,
        manifest: impl Into<PathBuf>,
        signature: impl Into<PathBuf>,
    ) -> Self {
        self.local_manifest = Some(manifest.into());
        self.local_signature = Some(signature.into());
        self
    }

    pub fn with_local_artifact(mut self, artifact: impl Into<PathBuf>) -> Self {
        self.local_artifact = Some(artifact.into());
        self
    }

    pub fn with_network_enabled(mut self, enabled: bool) -> Self {
        self.network_enabled = enabled;
        self
    }

    async fn bounded_response(response: reqwest::Response, cap: usize) -> Result<Vec<u8>> {
        if response
            .content_length()
            .is_some_and(|length| length > cap as u64)
        {
            return Err(UpdateError::ResponseTooLarge);
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| UpdateError::Network(error.to_string()))?
        {
            if chunk.len() > cap || body.len().saturating_add(chunk.len()) > cap {
                return Err(UpdateError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn get_bounded(&self, url: &str, cap: usize) -> Result<Vec<u8>> {
        if !self.network_enabled {
            return Err(UpdateError::Network(
                "network update checks are disabled".into(),
            ));
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| UpdateError::Network(error.to_string()))?
            .error_for_status()
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        Self::bounded_response(response, cap).await
    }

    fn read_local(path: &Path, cap: u64) -> Result<Vec<u8>> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| UpdateError::Staging(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UpdateError::Staging(
                "update path is not a regular file".into(),
            ));
        }
        if metadata.len() > cap {
            return Err(UpdateError::ResponseTooLarge);
        }
        std::fs::read(path).map_err(|error| UpdateError::Staging(error.to_string()))
    }
}

#[async_trait]
impl UpdateSource for HttpUpdateSource {
    async fn fetch_verified_manifest(&self) -> Result<UpdateManifest> {
        let (manifest, signature) = match (&self.local_manifest, &self.local_signature) {
            (Some(manifest), Some(signature)) => (
                Self::read_local(manifest, MAX_MANIFEST_BYTES as u64)?,
                Self::read_local(signature, MAX_SIGNATURE_BYTES as u64)?,
            ),
            (None, None) => (
                self.get_bounded(&self.manifest_url, MAX_MANIFEST_BYTES)
                    .await?,
                self.get_bounded(&self.signature_url, MAX_SIGNATURE_BYTES)
                    .await?,
            ),
            _ => {
                return Err(UpdateError::InvalidArtifact);
            }
        };
        let signature = std::str::from_utf8(&signature)
            .map_err(|error| UpdateError::Network(error.to_string()))?;
        parse_verified_manifest(&manifest, signature, &self.public_key)
    }

    async fn download_verified_artifact(&self, artifact: &UpdateArtifact) -> Result<Vec<u8>> {
        let bytes = if let Some(path) = &self.local_artifact {
            Self::read_local(path, MAX_UPDATE_ARTIFACT_BYTES)?
        } else {
            let cap = usize::try_from(MAX_UPDATE_ARTIFACT_BYTES).unwrap_or(usize::MAX);
            self.get_bounded(&artifact.url, cap).await?
        };
        verify_artifact_bytes(artifact, &bytes)?;
        Ok(bytes)
    }
}

impl Default for HttpUpdateSource {
    fn default() -> Self {
        Self::new().expect("embedded Frank update key and HTTP client must be valid")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub schema_version: u16,
    pub frank_version: String,
    pub release_timestamp: String,
    pub protocol_min: u16,
    pub protocol_max: u16,
    pub minimum_rollback_version: String,
    pub artifacts: Vec<UpdateArtifact>,
    pub release_notes_url: String,
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateArtifact {
    pub target: String,
    pub package_kind: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("manifest JSON is invalid: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("manifest signature is invalid")]
    InvalidSignature,
    #[error("manifest signature is not valid base64")]
    SignatureEncoding,
    #[error("manifest schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("manifest version is invalid: {0}")]
    InvalidVersion(String),
    #[error("manifest does not contain a compatible artifact")]
    ArtifactNotFound,
    #[error("artifact target or package kind is invalid")]
    InvalidArtifact,
    #[error("artifact digest mismatch")]
    DigestMismatch,
    #[error("artifact size mismatch")]
    SizeMismatch,
    #[error("refusing to downgrade from {current} to {candidate}")]
    Downgrade { current: String, candidate: String },
    #[error("safe staging failed: {0}")]
    Staging(String),
    #[error("update network request failed: {0}")]
    Network(String),
    #[error("update response exceeds the configured size cap")]
    ResponseTooLarge,
}

pub type Result<T> = std::result::Result<T, UpdateError>;

impl UpdateManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(UpdateError::UnsupportedSchema(self.schema_version));
        }
        let release_version = parse_version(&self.frank_version)?;
        let rollback_version = parse_version(&self.minimum_rollback_version)?;
        if rollback_version > release_version {
            return Err(UpdateError::InvalidVersion(
                "minimum rollback version is newer than the release".into(),
            ));
        }
        if self.protocol_min == 0 || self.protocol_min > self.protocol_max {
            return Err(UpdateError::InvalidArtifact);
        }
        if self.key_id.trim().is_empty()
            || self.release_notes_url.trim().is_empty()
            || !self.release_notes_url.starts_with("https://")
            || self.artifacts.is_empty()
        {
            return Err(UpdateError::InvalidArtifact);
        }
        for artifact in &self.artifacts {
            if artifact.target.trim().is_empty()
                || artifact.package_kind.trim().is_empty()
                || !artifact.url.starts_with("https://")
                || artifact.size > MAX_UPDATE_ARTIFACT_BYTES
                || artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(UpdateError::InvalidArtifact);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    pub fn artifact(&self, target: &str, package_kind: &str) -> Result<&UpdateArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target == target && artifact.package_kind == package_kind)
            .ok_or(UpdateError::ArtifactNotFound)
    }

    pub fn accepts_protocol(&self, version: u16) -> bool {
        version >= self.protocol_min && version <= self.protocol_max
    }

    pub fn rejects_downgrade_from(&self, current: &str) -> Result<()> {
        let current = parse_version(current)?;
        let candidate = parse_version(&self.frank_version)?;
        if candidate < current {
            return Err(UpdateError::Downgrade {
                current: current.to_string(),
                candidate: candidate.to_string(),
            });
        }
        Ok(())
    }
}

/// Sign canonical manifest bytes using a minisign-compatible detached
/// signature packet. The private key is an RFC 8032 PKCS#8 document as
/// emitted by `ring::signature::Ed25519KeyPair::generate_pkcs8`.
pub fn sign_manifest(manifest: &[u8], private_key_pkcs8: &[u8]) -> Result<String> {
    let pair =
        Ed25519KeyPair::from_pkcs8(private_key_pkcs8).map_err(|_| UpdateError::InvalidSignature)?;
    let signature = pair.sign(manifest);
    let digest = Sha256::digest(pair.public_key().as_ref());
    let mut packet = Vec::with_capacity(74);
    packet.extend_from_slice(b"Ed");
    packet.extend_from_slice(&digest[..8]);
    packet.extend_from_slice(signature.as_ref());
    Ok(format!(
        "untrusted comment: signature from Frank update key\n{}\ntrusted comment: Frank update manifest\n",
        base64::engine::general_purpose::STANDARD.encode(packet)
    ))
}

/// Derive the raw Ed25519 public key in the base64 form accepted by release
/// configuration. This is intended for an offline key ceremony; callers
/// should never pass the private key through CI arguments or logs.
pub fn public_key_base64_from_pkcs8(private_key_pkcs8: &[u8]) -> Result<String> {
    let pair =
        Ed25519KeyPair::from_pkcs8(private_key_pkcs8).map_err(|_| UpdateError::InvalidSignature)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(pair.public_key().as_ref()))
}

/// Verify a detached signature before parsing the manifest. In addition to a
/// bare base64 Ed25519 signature, the first payload of a standard minisign
/// file is accepted. Minisign wraps the Ed25519 signature with a two-byte
/// algorithm tag and an eight-byte key id; the final 64 bytes are the message
/// signature. Comments are metadata and never become a trust decision.
pub fn verify_detached_signature(
    manifest: &[u8],
    signature_text: &str,
    public_key: &[u8],
) -> Result<()> {
    let encoded = signature_text
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with("untrusted comment:")
                && !line.starts_with("trusted comment:")
        })
        .and_then(|line| line.strip_prefix("frank-ed25519-v1:").or(Some(line)))
        .ok_or(UpdateError::SignatureEncoding)?;
    let packet = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| UpdateError::SignatureEncoding)?;
    // A minisign signature packet is exactly `Ed` + the eight-byte key id +
    // the 64-byte Ed25519 signature. Accepting arbitrary longer payloads and
    // taking their last 64 bytes would let a malformed packet smuggle
    // unchecked metadata past the verifier. A bare 64-byte signature remains
    // accepted for development fixtures and older Frank 1.0 builds.
    let (signature, packet_key_id) = match packet.as_slice() {
        bytes if bytes.len() == 64 => (bytes, None),
        bytes if bytes.len() == 74 && &bytes[..2] == b"Ed" => (
            bytes.get(10..).ok_or(UpdateError::InvalidSignature)?,
            Some(&bytes[2..10]),
        ),
        _ => return Err(UpdateError::InvalidSignature),
    };
    let public_key = match public_key {
        bytes if bytes.len() == 32 => bytes,
        // Accept a decoded minisign public-key packet as a convenience for
        // release tooling while keeping the binary's embedded raw key small.
        bytes if bytes.len() == 42 && &bytes[..2] == b"Ed" => &bytes[10..],
        _ => return Err(UpdateError::InvalidSignature),
    };
    if let Some(packet_key_id) = packet_key_id {
        let digest = Sha256::digest(public_key);
        if packet_key_id != &digest[..8] {
            return Err(UpdateError::InvalidSignature);
        }
    }
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(manifest, signature)
        .map_err(|_| UpdateError::InvalidSignature)
}

pub fn parse_verified_manifest(
    manifest: &[u8],
    signature_text: &str,
    public_key: &[u8],
) -> Result<UpdateManifest> {
    verify_detached_signature(manifest, signature_text, public_key)?;
    let parsed: UpdateManifest = serde_json::from_slice(manifest)?;
    parsed.validate()?;
    Ok(parsed)
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

pub fn verify_artifact_bytes(artifact: &UpdateArtifact, bytes: &[u8]) -> Result<()> {
    if artifact.size > MAX_UPDATE_ARTIFACT_BYTES {
        return Err(UpdateError::InvalidArtifact);
    }
    if bytes.len() as u64 != artifact.size {
        return Err(UpdateError::SizeMismatch);
    }
    if !sha256_bytes(bytes).eq_ignore_ascii_case(&artifact.sha256) {
        return Err(UpdateError::DigestMismatch);
    }
    Ok(())
}

/// Write a verified payload into a new staging file.  Existing files are not
/// overwritten, making interrupted or malicious updates recoverable.
pub fn stage_artifact(
    artifact: &UpdateArtifact,
    bytes: &[u8],
    staging_root: impl AsRef<Path>,
) -> Result<PathBuf> {
    verify_artifact_bytes(artifact, bytes)?;
    let root = staging_root.as_ref();
    reject_symlink_components(root)?;
    std::fs::create_dir_all(root).map_err(|error| UpdateError::Staging(error.to_string()))?;
    // Re-check after creating the directory.  This closes the common
    // "missing staging root" window where an attacker could replace a newly
    // created component before the temporary file is opened.  The final
    // rename remains confined to this validated directory.
    reject_symlink_components(root)?;
    let path = staged_artifact_path(artifact, root);
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UpdateError::Staging(
                "existing staged artifact is not a regular file".into(),
            ));
        }
        let existing =
            std::fs::read(&path).map_err(|error| UpdateError::Staging(error.to_string()))?;
        verify_artifact_bytes(artifact, &existing)?;
        return Ok(path);
    }
    let temp = root.join(format!(".{}.tmp", uuid_like_name()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| UpdateError::Staging(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| UpdateError::Staging(error.to_string()))?;
    file.sync_all()
        .map_err(|error| UpdateError::Staging(error.to_string()))?;
    match std::fs::rename(&temp, &path) {
        Ok(()) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another updater process may have won the race after our
            // create-new temporary file was written.  Re-read the winner and
            // preserve idempotency only when its digest and size are valid.
            let existing = std::fs::read(&path)
                .map_err(|read_error| UpdateError::Staging(read_error.to_string()))?;
            verify_artifact_bytes(artifact, &existing)?;
            let _ = std::fs::remove_file(&temp);
            Ok(path)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(UpdateError::Staging(error.to_string()))
        }
    }
}

/// Return the deterministic path used for an already verified staged
/// artifact.  Keeping this derivation public lets the daemon recover the
/// exact payload after a restart without persisting a server filesystem path
/// in the remote snapshot or trusting an environment variable supplied by a
/// client.
pub fn staged_artifact_path(artifact: &UpdateArtifact, staging_root: impl AsRef<Path>) -> PathBuf {
    staging_root.as_ref().join(format!(
        "{}-{}.staged",
        sanitize_component(&artifact.target),
        artifact.sha256
    ))
}

/// Reject symlinks at the staging path and its first existing parent.  A check
/// of only the leaf misses a symlinked parent (`/tmp/link/staged`), which would
/// otherwise redirect a verified update payload outside the intended root.
/// We intentionally stop at the first normal ancestor: macOS commonly exposes
/// the temporary directory through a system `/var` symlink, and rejecting
/// unrelated ancestors would make safe staging unusable on that platform.
/// Missing components are allowed because callers create them immediately
/// after this validation; permission and other metadata errors fail closed.
fn reject_symlink_components(path: &Path) -> Result<()> {
    let mut current = path;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(UpdateError::Staging(
                    "staging path may not contain symlink components".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(UpdateError::Staging(error.to_string())),
        }
        let Some(parent) = current.parent() else {
            break;
        };
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(UpdateError::Staging(
                    "staging path may not contain symlink components".into(),
                ));
            }
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => current = parent,
            Err(error) => return Err(UpdateError::Staging(error.to_string())),
        }
    }
    Ok(())
}

fn sanitize_component(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect::<String>();
    if value.is_empty() {
        "artifact".into()
    } else {
        value
    }
}

fn uuid_like_name() -> String {
    // Use OS randomness so two concurrent staging attempts cannot collide
    // even when they share a process and clock tick. The fallback remains
    // unique enough for a private local staging directory and never weakens
    // the digest verification performed before rename.
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_ok() {
        return hex::encode(bytes);
    }
    format!(
        "{}-{}-{}",
        std::process::id(),
        unix_seconds(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
    )
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version<'a> {
    major: u64,
    minor: u64,
    patch: u64,
    suffix: &'a str,
}

impl std::fmt::Display for Version<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}.{}.{}{}",
            self.major, self.minor, self.patch, self.suffix
        )
    }
}

fn parse_version(value: &str) -> Result<Version<'_>> {
    let value = value.strip_prefix('v').unwrap_or(value);
    let (numbers, suffix) = value.split_once('-').unwrap_or((value, ""));
    let mut parts = numbers.split('.');
    let parse = |part: Option<&str>| {
        part.ok_or_else(|| UpdateError::InvalidVersion(value.to_string()))?
            .parse::<u64>()
            .map_err(|_| UpdateError::InvalidVersion(value.to_string()))
    };
    let version = Version {
        major: parse(parts.next())?,
        minor: parse(parts.next())?,
        patch: parse(parts.next())?,
        suffix,
    };
    if parts.next().is_some() {
        return Err(UpdateError::InvalidVersion(value.to_string()));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;

    fn manifest() -> UpdateManifest {
        UpdateManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            frank_version: "1.0.1".into(),
            release_timestamp: "2026-08-28T00:00:00Z".into(),
            protocol_min: 1,
            protocol_max: 1,
            minimum_rollback_version: "1.0.0".into(),
            artifacts: vec![UpdateArtifact {
                target: "aarch64-apple-darwin".into(),
                package_kind: "dmg".into(),
                url: "https://example.invalid/frank.dmg".into(),
                size: 3,
                sha256: sha256_bytes(b"abc"),
            }],
            release_notes_url: "https://example.invalid/notes".into(),
            key_id: "test".into(),
        }
    }

    #[test]
    fn signed_manifest_round_trips() {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let derived = public_key_base64_from_pkcs8(pkcs8.as_ref()).unwrap();
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(pair.public_key().as_ref()),
            derived
        );
        let bytes = manifest().canonical_bytes().unwrap();
        let signature = sign_manifest(&bytes, pkcs8.as_ref()).unwrap();
        let packet = signature
            .lines()
            .find(|line| !line.starts_with("untrusted") && !line.starts_with("trusted"))
            .and_then(|line| base64::engine::general_purpose::STANDARD.decode(line).ok())
            .unwrap();
        assert_eq!(packet.len(), 74);
        assert_eq!(&packet[..2], b"Ed");
        let parsed =
            parse_verified_manifest(&bytes, &signature, pair.public_key().as_ref()).unwrap();
        assert_eq!(parsed.frank_version, "1.0.1");
    }

    #[test]
    fn bad_digest_and_downgrade_fail_closed() {
        let value = manifest();
        assert!(value.rejects_downgrade_from("1.0.2").is_err());
        let artifact = value.artifacts.first().unwrap();
        assert!(verify_artifact_bytes(artifact, b"bad").is_err());
    }

    #[test]
    fn rollback_floor_cannot_be_newer_than_release() {
        let mut value = manifest();
        value.minimum_rollback_version = "2.0.0".into();
        assert!(matches!(
            value.validate(),
            Err(UpdateError::InvalidVersion(message)) if message.contains("rollback")
        ));
    }

    #[test]
    fn manifest_rejects_oversized_artifact() {
        let mut value = manifest();
        value.artifacts[0].size = MAX_UPDATE_ARTIFACT_BYTES.saturating_add(1);
        assert!(matches!(
            value.validate(),
            Err(UpdateError::InvalidArtifact)
        ));
    }

    #[test]
    fn malformed_minisign_packet_is_rejected_before_crypto() {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let bytes = manifest().canonical_bytes().unwrap();
        let signature = sign_manifest(&bytes, pkcs8.as_ref()).unwrap();
        let payload = signature
            .lines()
            .find(|line| !line.starts_with("untrusted") && !line.starts_with("trusted"))
            .unwrap();
        let mut packet = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        packet.push(0);
        let malformed = base64::engine::general_purpose::STANDARD.encode(packet);
        assert!(matches!(
            verify_detached_signature(&bytes, &malformed, pair.public_key().as_ref()),
            Err(UpdateError::InvalidSignature)
        ));
    }

    #[test]
    fn staging_is_digest_checked_and_idempotent() {
        let value = manifest();
        let artifact = value.artifacts.first().unwrap();
        let root = tempfile::tempdir().unwrap();
        let first = stage_artifact(artifact, b"abc", root.path()).unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), b"abc");
        assert_eq!(first, staged_artifact_path(artifact, root.path()));
        let second = stage_artifact(artifact, b"abc", root.path()).unwrap();
        assert_eq!(first, second);
        assert!(matches!(
            stage_artifact(artifact, b"tampered", root.path()),
            Err(UpdateError::SizeMismatch | UpdateError::DigestMismatch)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let value = manifest();
        let artifact = value.artifacts.first().unwrap();
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let link = root.path().join("link");
        std::fs::create_dir(&real).unwrap();
        symlink(&real, &link).unwrap();
        let result = stage_artifact(artifact, b"abc", link.join("staged"));
        assert!(
            matches!(result, Err(UpdateError::Staging(message)) if message.contains("symlink"))
        );
        assert!(!real.join("staged").exists());
    }
}
