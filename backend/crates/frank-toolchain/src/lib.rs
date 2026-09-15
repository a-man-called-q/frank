//! Manifest-driven language toolchains.
//!
//! The daemon only asks this crate for detection and an immutable install
//! preview.  A host runner performs the actual download/install and check;
//! this keeps SDKs out of the frankd container and keeps shell interpretation
//! out of the task execution boundary.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use frank_protocol::{
    ToolchainArtifact, ToolchainArtifactSource, ToolchainCheck, ToolchainDetection,
    ToolchainInstallPlan, ToolchainManifest, ToolchainProbe,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_DETECTION_DEPTH: usize = 8;
const MAX_DETECTION_ENTRIES: usize = 10_000;

#[derive(Debug, Error)]
pub enum ToolchainError {
    #[error("toolchain manifest IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("toolchain manifest is invalid: {0}")]
    Manifest(String),
    #[error("toolchain artifact checksum mismatch")]
    ChecksumMismatch,
    #[error("toolchain artifact size mismatch")]
    SizeMismatch,
    #[error("no artifact is available for platform '{0}'")]
    UnsupportedPlatform(String),
    #[error("toolchain path is invalid: {0}")]
    InvalidPath(String),
    #[error("toolchain executable is unavailable: {0}")]
    ExecutableUnavailable(String),
}

pub type Result<T> = std::result::Result<T, ToolchainError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDetection {
    pub detected: bool,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProbeResult {
    pub executable: String,
    pub available: bool,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<ToolchainManifest> {
    let path = path.as_ref();
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ToolchainError::InvalidPath(path.display().to_string()));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ToolchainError::Manifest(format!(
            "manifest exceeds the {} byte limit",
            MAX_MANIFEST_BYTES
        )));
    }
    let content = std::fs::read_to_string(path)?;
    let manifest = toml::from_str::<ToolchainManifest>(&content)
        .map_err(|error| ToolchainError::Manifest(error.to_string()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Load all local manifests from a single, operator-selected directory.
/// Discovery is deliberately non-recursive so a project cannot smuggle a
/// manifest into an unrelated parent directory or cause an unbounded walk.
pub fn load_local_manifests(directory: impl AsRef<Path>) -> Result<Vec<ToolchainManifest>> {
    let directory = directory.as_ref();
    let metadata = std::fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ToolchainError::InvalidPath(directory.display().to_string()));
    }
    let mut paths = std::fs::read_dir(directory)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut manifests = Vec::with_capacity(paths.len());
    let mut ids = HashSet::new();
    for path in paths {
        let manifest = load_manifest(path)?;
        if !ids.insert(manifest.id.clone()) {
            return Err(ToolchainError::Manifest(format!(
                "duplicate local toolchain id '{}'",
                manifest.id
            )));
        }
        manifests.push(manifest);
    }
    Ok(manifests)
}

pub fn parse_manifest(content: &str) -> Result<ToolchainManifest> {
    let manifest = toml::from_str::<ToolchainManifest>(content)
        .map_err(|error| ToolchainError::Manifest(error.to_string()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_manifest(manifest: &ToolchainManifest) -> Result<()> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(ToolchainError::Manifest(format!(
            "unsupported schema version {}",
            manifest.schema_version
        )));
    }
    if !valid_component(&manifest.id) || !valid_component(&manifest.version) {
        return Err(ToolchainError::Manifest(
            "id and version must be non-empty path components".into(),
        ));
    }
    if manifest.detect.files.is_empty()
        && manifest.detect.extensions.is_empty()
        && manifest.detect.probe.is_none()
    {
        return Err(ToolchainError::Manifest(
            "at least one project detection rule is required".into(),
        ));
    }
    if manifest.label.trim().is_empty() || manifest.label.chars().any(char::is_control) {
        return Err(ToolchainError::Manifest("label must not be empty".into()));
    }
    validate_relative_path(&manifest.install.directory, "install.directory")?;
    if let Some(executable) = &manifest.install.executable {
        validate_executable(executable)?;
    }
    for (key, value) in &manifest.install.environment {
        if key.is_empty()
            || key.contains('=')
            || key.contains('\0')
            || value.contains('\0')
            || is_sensitive_environment_key(key)
        {
            return Err(ToolchainError::Manifest(
                "install environment contains an invalid or sensitive entry".into(),
            ));
        }
    }
    for marker in &manifest.detect.files {
        validate_relative_path(marker, "detect.files")?;
    }
    for extension in &manifest.detect.extensions {
        if extension.trim().is_empty()
            || extension.contains('/')
            || extension.contains('\\')
            || extension.contains('\0')
        {
            return Err(ToolchainError::Manifest(
                "detect.extensions contains an invalid value".into(),
            ));
        }
    }
    if let Some(probe) = &manifest.detect.probe {
        validate_executable(&probe.program)?;
        if probe.version_arg.contains('\0') || probe.args.iter().any(|arg| arg.contains('\0')) {
            return Err(ToolchainError::Manifest(
                "toolchain probe arguments contain a NUL byte".into(),
            ));
        }
    }
    let mut check_ids = HashSet::new();
    for check in &manifest.checks {
        validate_check(check)?;
        if check.id.trim().is_empty() || !check_ids.insert(check.id.clone()) {
            return Err(ToolchainError::Manifest(format!(
                "check id is empty or duplicated: '{}'",
                check.id
            )));
        }
        if check.timeout_seconds == 0 || check.timeout_seconds > 86_400 {
            return Err(ToolchainError::Manifest(format!(
                "check '{}' timeout is outside 1..86400 seconds",
                check.id
            )));
        }
    }
    let mut artifact_platforms = HashSet::new();
    for artifact in &manifest.artifacts {
        if artifact.platform.trim().is_empty()
            || artifact.platform.chars().any(char::is_control)
            || !artifact_platforms.insert(artifact.platform.clone())
        {
            return Err(ToolchainError::Manifest(format!(
                "artifact platform is empty or duplicated: '{}'",
                artifact.platform
            )));
        }
        if artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ToolchainError::Manifest(format!(
                "artifact for platform '{}' has an invalid SHA-256",
                artifact.platform
            )));
        }
        if artifact.size_bytes == 0 {
            return Err(ToolchainError::Manifest(
                "artifact size must be greater than zero".into(),
            ));
        }
        match &artifact.source {
            ToolchainArtifactSource::Url { url }
                if url.trim().is_empty() || !url.starts_with("https://") || url.contains('\0') =>
            {
                return Err(ToolchainError::Manifest(
                    "artifact URL must be a non-empty HTTPS URL".into(),
                ));
            }
            ToolchainArtifactSource::Host { executable } => validate_executable(executable)?,
            ToolchainArtifactSource::Url { .. } => {}
        }
        if artifact.archive.as_deref().is_some_and(|archive| {
            archive.trim().is_empty() || archive.chars().any(char::is_control)
        }) {
            return Err(ToolchainError::Manifest(
                "artifact archive label is invalid".into(),
            ));
        }
    }
    Ok(())
}

fn validate_executable(program: &str) -> Result<()> {
    if program.trim().is_empty()
        || program.contains('\0')
        || program.contains('/')
        || program.contains('\\')
    {
        return Err(ToolchainError::Manifest(format!(
            "executable must be a bare program name: {program:?}"
        )));
    }
    Ok(())
}

fn validate_check(check: &ToolchainCheck) -> Result<()> {
    validate_executable(&check.program)?;
    if is_forbidden_program(&check.program)
        || check.args.iter().any(|arg| arg.contains('\0'))
        || check.environment.iter().any(|(key, value)| {
            key.is_empty()
                || key.contains('=')
                || key.contains('\0')
                || value.contains('\0')
                || is_sensitive_environment_key(key)
        })
    {
        return Err(ToolchainError::Manifest(format!(
            "check '{}' contains a forbidden executable or environment entry",
            check.id
        )));
    }
    Ok(())
}

fn is_forbidden_program(program: &str) -> bool {
    matches!(
        program.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "sudo"
            | "doas"
            | "pkexec"
    )
}

fn is_sensitive_environment_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "credential",
        "authorization",
        "private_key",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn valid_component(value: &str) -> bool {
    !value.trim().is_empty()
        && !value.chars().any(char::is_control)
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
}

fn validate_relative_path(path: &str, field: &str) -> Result<()> {
    let candidate = Path::new(path);
    if path.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || path.contains('\0')
    {
        return Err(ToolchainError::Manifest(format!(
            "{field} must be a non-empty relative path without parent traversal"
        )));
    }
    Ok(())
}

pub fn detect_project(root: impl AsRef<Path>, detection: &ToolchainDetection) -> ProjectDetection {
    let root = root.as_ref();
    let mut evidence = Vec::new();
    for marker in &detection.files {
        if std::fs::symlink_metadata(root.join(marker))
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        {
            evidence.push(marker.clone());
        }
    }
    if !detection.extensions.is_empty() {
        let mut stack = vec![(root.to_path_buf(), 0usize)];
        let mut visited = HashSet::new();
        let mut entries_seen = 0usize;
        while let Some((directory, depth)) = stack.pop() {
            if depth > MAX_DETECTION_DEPTH || entries_seen >= MAX_DETECTION_ENTRIES {
                continue;
            }
            let Ok(directory_key) = std::fs::canonicalize(&directory) else {
                continue;
            };
            if !visited.insert(directory_key) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                entries_seen = entries_seen.saturating_add(1);
                let path = entry.path();
                let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    stack.push((path, depth.saturating_add(1)));
                } else if metadata.is_file()
                    && let Some(extension) = path.extension().and_then(|value| value.to_str())
                    && detection
                        .extensions
                        .iter()
                        .any(|expected| expected.trim_start_matches('.') == extension)
                {
                    let marker = format!("*.{extension}");
                    if !evidence.contains(&marker) {
                        evidence.push(marker);
                    }
                }
            }
        }
    }
    ProjectDetection {
        detected: !evidence.is_empty(),
        evidence,
    }
}

/// Return a stable platform key used by manifests. It intentionally avoids
/// network or registry resolution; a manifest must contain an exact matching
/// artifact or the caller receives an explicit unsupported-platform result.
pub fn platform_key() -> String {
    format!(
        "{}-{}",
        std::env::consts::OS,
        match std::env::consts::ARCH {
            "aarch64" => "aarch64",
            "x86_64" => "x86_64",
            other => other,
        }
    )
}

/// Probe a host executable using an argv vector. This is for display and
/// requirement resolution only; job execution belongs to frank-runner.
pub fn probe_runtime(probe: &ToolchainProbe, _timeout: Duration) -> RuntimeProbeResult {
    let mut command = Command::new(&probe.program);
    command
        .env_clear()
        .args(&probe.args)
        .arg(&probe.version_arg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(path) = std::env::var("PATH") {
        command.env("PATH", path);
    }
    let result = run_probe(command, _timeout);
    match result {
        Ok(ProbeOutput {
            status: Some(status),
            stdout,
            stderr,
        }) if status.success() => {
            let version = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().chars().take(256).collect());
            RuntimeProbeResult {
                executable: probe.program.clone(),
                available: true,
                version,
                diagnostic: None,
            }
        }
        Ok(ProbeOutput {
            status: Some(status),
            stderr,
            ..
        }) => RuntimeProbeResult {
            executable: probe.program.clone(),
            available: false,
            version: None,
            diagnostic: Some(if stderr.trim().is_empty() {
                format!("{} exited with {status}", probe.program)
            } else {
                sanitize_probe_error(stderr.trim())
            }),
        },
        Ok(ProbeOutput { status: None, .. }) => RuntimeProbeResult {
            executable: probe.program.clone(),
            available: false,
            version: None,
            diagnostic: Some("runtime probe timed out".into()),
        },
        Err(error) => RuntimeProbeResult {
            executable: probe.program.clone(),
            available: false,
            version: None,
            diagnostic: Some(sanitize_probe_error(&error.to_string())),
        },
    }
}

struct ProbeOutput {
    status: Option<std::process::ExitStatus>,
    stdout: String,
    stderr: String,
}

fn run_probe(mut command: Command, timeout: Duration) -> std::io::Result<ProbeOutput> {
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .map(|mut stream| thread::spawn(move || read_capped(&mut stream)));
    let stderr = child
        .stderr
        .take()
        .map(|mut stream| thread::spawn(move || read_capped(&mut stream)));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.map(|reader| reader.join());
            let _ = stderr.map(|reader| reader.join());
            return Ok(ProbeOutput {
                status: None,
                stdout: String::new(),
                stderr: "runtime probe timed out".into(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout
        .map(|reader| reader.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr
        .map(|reader| reader.join().unwrap_or_default())
        .unwrap_or_default();
    Ok(ProbeOutput {
        status: Some(status),
        stdout,
        stderr,
    })
}

fn read_capped(reader: &mut impl Read) -> String {
    const MAX_PROBE_OUTPUT_BYTES: usize = 64 * 1024;
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => {
                if output.len() < MAX_PROBE_OUTPUT_BYTES {
                    let remaining = MAX_PROBE_OUTPUT_BYTES - output.len();
                    output.extend_from_slice(&buffer[..length.min(remaining)]);
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn sanitize_probe_error(error: &str) -> String {
    error.chars().take(256).collect()
}

/// Resolve the UI-facing requirement without downloading anything. A host
/// executable is Ready only when the probe succeeds; URL artifacts remain
/// NeedsApproval until a task-scoped install is explicitly approved.
pub fn resolve_requirement(
    manifest: &ToolchainManifest,
    project_root: impl AsRef<Path>,
    platform: &str,
    data_directory: impl AsRef<Path>,
) -> Result<frank_protocol::ToolchainRequirementView> {
    validate_manifest(manifest)?;
    let detection = detect_project(project_root, &manifest.detect);
    let plan = resolve_install_plan(manifest, platform, data_directory)?;
    let artifact_source = plan.artifact.as_ref().map(|artifact| &artifact.source);
    let probe = detection
        .detected
        .then(|| {
            manifest
                .detect
                .probe
                .as_ref()
                .map(|probe| probe_runtime(probe, Duration::from_secs(10)))
        })
        .flatten();
    let (status, detected_version, diagnostic) = if !detection.detected {
        (
            frank_protocol::ToolchainRequirementStatus::Incompatible,
            None,
            Some("toolchain is not detected in this project".into()),
        )
    } else {
        match probe {
            Some(probe) if probe.available && version_satisfies(&manifest.version, &probe) => (
                frank_protocol::ToolchainRequirementStatus::Ready,
                probe.version,
                None,
            ),
            Some(probe) if probe.available => (
                if matches!(artifact_source, Some(ToolchainArtifactSource::Host { .. })) {
                    frank_protocol::ToolchainRequirementStatus::ManualRequirement
                } else {
                    frank_protocol::ToolchainRequirementStatus::NeedsApproval
                },
                probe.version.clone(),
                Some(format!(
                    "required version '{}' does not match detected runtime",
                    manifest.version
                )),
            ),
            Some(probe)
                if matches!(artifact_source, Some(ToolchainArtifactSource::Host { .. })) =>
            {
                (
                    frank_protocol::ToolchainRequirementStatus::ManualRequirement,
                    None,
                    probe
                        .diagnostic
                        .or_else(|| Some("host toolchain is required".into())),
                )
            }
            _ => (
                frank_protocol::ToolchainRequirementStatus::NeedsApproval,
                None,
                Some("install approval is required".into()),
            ),
        }
    };
    Ok(frank_protocol::ToolchainRequirementView {
        manifest_id: manifest.id.clone(),
        label: manifest.label.clone(),
        required_version: manifest.version.clone(),
        status,
        detected_version,
        diagnostic,
        install_plan: Some(plan),
    })
}

/// Resolve a requirement without executing a host process. `frankd` may run
/// in Docker, so it must not claim that a host SDK is Ready based on a probe
/// performed inside the daemon. The host runner can replace this display
/// state with a verified result after it connects.
pub fn resolve_requirement_without_host_probe(
    manifest: &ToolchainManifest,
    project_root: impl AsRef<Path>,
    platform: &str,
    data_directory: impl AsRef<Path>,
) -> Result<frank_protocol::ToolchainRequirementView> {
    validate_manifest(manifest)?;
    let detection = detect_project(project_root, &manifest.detect);
    let plan = resolve_install_plan(manifest, platform, data_directory)?;
    let artifact_source = plan.artifact.as_ref().map(|artifact| &artifact.source);
    let (status, diagnostic) = if !detection.detected {
        (
            frank_protocol::ToolchainRequirementStatus::Incompatible,
            "toolchain is not detected in this project".to_string(),
        )
    } else if matches!(artifact_source, Some(ToolchainArtifactSource::Host { .. })) {
        (
            frank_protocol::ToolchainRequirementStatus::ManualRequirement,
            "host runner probe is required; frankd does not execute SDKs".to_string(),
        )
    } else {
        (
            frank_protocol::ToolchainRequirementStatus::NeedsApproval,
            "install approval is required".to_string(),
        )
    };
    Ok(frank_protocol::ToolchainRequirementView {
        manifest_id: manifest.id.clone(),
        label: manifest.label.clone(),
        required_version: manifest.version.clone(),
        status,
        detected_version: None,
        diagnostic: Some(diagnostic),
        install_plan: Some(plan),
    })
}

fn version_satisfies(required: &str, probe: &RuntimeProbeResult) -> bool {
    if !probe.available {
        return false;
    }
    let required = required.trim();
    if required.is_empty() || !required.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    probe
        .version
        .as_deref()
        .is_some_and(|version| version.contains(required))
}

pub fn resolve_install_plan(
    manifest: &ToolchainManifest,
    platform: &str,
    data_directory: impl AsRef<Path>,
) -> Result<ToolchainInstallPlan> {
    validate_manifest(manifest)?;
    let artifact = select_artifact(&manifest.artifacts, platform)?;
    let install_path = data_directory
        .as_ref()
        .join(&manifest.install.directory)
        .join(&manifest.id)
        .join(&manifest.version);
    let source = match &artifact.source {
        ToolchainArtifactSource::Url { url } => url.clone(),
        ToolchainArtifactSource::Host { executable } => format!("host:{executable}"),
    };
    Ok(ToolchainInstallPlan {
        manifest_id: manifest.id.clone(),
        version: manifest.version.clone(),
        source,
        sha256: artifact.sha256.clone(),
        size_bytes: artifact.size_bytes,
        install_path: install_path.to_string_lossy().into_owned(),
        checks: manifest
            .checks
            .iter()
            .map(|check| check.id.clone())
            .collect(),
        approval_scope: "one install: user-local toolchain directory".into(),
        artifact: Some(artifact.clone()),
    })
}

fn select_artifact<'a>(
    artifacts: &'a [ToolchainArtifact],
    platform: &str,
) -> Result<&'a ToolchainArtifact> {
    artifacts
        .iter()
        .find(|artifact| artifact.platform == platform)
        .or_else(|| artifacts.iter().find(|artifact| artifact.platform == "any"))
        .ok_or_else(|| ToolchainError::UnsupportedPlatform(platform.to_string()))
}

pub fn verify_artifact(bytes: &[u8], artifact: &ToolchainArtifact) -> Result<()> {
    if bytes.len() as u64 != artifact.size_bytes {
        return Err(ToolchainError::SizeMismatch);
    }
    let mut digest = Sha256::new();
    digest.update(bytes);
    let actual = hex::encode(digest.finalize());
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(ToolchainError::ChecksumMismatch);
    }
    Ok(())
}

pub fn check_command(
    check: &ToolchainCheck,
) -> (String, Vec<String>, BTreeMap<String, String>, u64) {
    (
        check.program.clone(),
        check.args.clone(),
        check.environment.clone(),
        check.timeout_seconds,
    )
}

pub fn probe_spec(detection: &ToolchainDetection) -> Option<&ToolchainProbe> {
    detection.probe.as_ref()
}

/// Built-in manifests intentionally use host tools in v1.  Downloaded
/// artifacts are accepted only when a local manifest supplies an exact URL,
/// size, and SHA-256; Frank never resolves a moving “latest” registry entry.
pub fn builtin_manifests() -> Vec<ToolchainManifest> {
    vec![
        host_manifest(
            "flutter",
            "Flutter",
            "3.47.1",
            vec![".prototools", "pubspec.yaml"],
            vec!["dart"],
            "flutter",
            vec![("test", vec!["test"]), ("build-web", vec!["build", "web"])],
        ),
        host_manifest(
            "odin",
            "Odin",
            "locked",
            vec!["ols.json"],
            vec!["odin"],
            "odin",
            vec![("test", vec!["test"])],
        ),
        host_manifest(
            "dotnet",
            ".NET",
            "lts-lockfile",
            vec!["global.json"],
            vec!["cs"],
            "dotnet",
            vec![("test", vec!["test"])],
        ),
        cpp_manifest(),
    ]
}

fn cpp_manifest() -> ToolchainManifest {
    ToolchainManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        id: "cpp".into(),
        label: "C++ / CMake / CTest".into(),
        version: "llvm-locked".into(),
        detect: ToolchainDetection {
            files: vec!["CMakeLists.txt".into()],
            extensions: vec!["cpp".into(), "cc".into(), "cxx".into()],
            probe: Some(ToolchainProbe {
                program: "clang++".into(),
                args: Vec::new(),
                version_arg: "--version".into(),
            }),
        },
        artifacts: vec![ToolchainArtifact {
            platform: "any".into(),
            source: ToolchainArtifactSource::Host {
                executable: "clang++".into(),
            },
            sha256: "0".repeat(64),
            size_bytes: 1,
            archive: None,
        }],
        install: frank_protocol::ToolchainInstallSpec {
            directory: "toolchains".into(),
            executable: Some("clang++".into()),
            environment: BTreeMap::new(),
        },
        checks: vec![
            frank_protocol::ToolchainCheck {
                id: "cmake-build".into(),
                program: "cmake".into(),
                args: vec![
                    "--build".into(),
                    ".".into(),
                    "--target".into(),
                    "test".into(),
                ],
                environment: BTreeMap::new(),
                timeout_seconds: 600,
            },
            frank_protocol::ToolchainCheck {
                id: "ctest".into(),
                program: "ctest".into(),
                args: vec![
                    "--test-dir".into(),
                    ".".into(),
                    "--output-on-failure".into(),
                ],
                environment: BTreeMap::new(),
                timeout_seconds: 600,
            },
        ],
    }
}

fn host_manifest(
    id: &str,
    label: &str,
    version: &str,
    files: Vec<&str>,
    extensions: Vec<&str>,
    executable: &str,
    checks: Vec<(&str, Vec<&str>)>,
) -> ToolchainManifest {
    ToolchainManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        id: id.into(),
        label: label.into(),
        version: version.into(),
        detect: ToolchainDetection {
            files: files.into_iter().map(str::to_string).collect(),
            extensions: extensions.into_iter().map(str::to_string).collect(),
            probe: Some(ToolchainProbe {
                program: executable.into(),
                args: Vec::new(),
                version_arg: "--version".into(),
            }),
        },
        artifacts: vec![ToolchainArtifact {
            platform: "any".into(),
            source: ToolchainArtifactSource::Host {
                executable: executable.into(),
            },
            // Host artifacts are never downloaded. This sentinel is still a
            // fixed value so a UI can display that no remote checksum exists.
            sha256: "0".repeat(64),
            size_bytes: 1,
            archive: None,
        }],
        install: frank_protocol::ToolchainInstallSpec {
            directory: "toolchains".into(),
            executable: Some(executable.into()),
            environment: BTreeMap::new(),
        },
        checks: checks
            .into_iter()
            .map(|(id, args)| frank_protocol::ToolchainCheck {
                id: id.into(),
                program: executable.into(),
                args: args.into_iter().map(str::to_string).collect(),
                environment: BTreeMap::new(),
                timeout_seconds: 600,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_protocol::{ToolchainArtifactSource, ToolchainRequirementStatus};

    fn manifest() -> ToolchainManifest {
        ToolchainManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            id: "demo".into(),
            label: "Demo".into(),
            version: "1.0.0".into(),
            detect: ToolchainDetection {
                files: vec!["demo.toml".into()],
                extensions: Vec::new(),
                probe: None,
            },
            artifacts: vec![ToolchainArtifact {
                platform: "macos-aarch64".into(),
                source: ToolchainArtifactSource::Url {
                    url: "https://example.invalid/demo.tar.gz".into(),
                },
                sha256: "ab".repeat(32),
                size_bytes: 4,
                archive: Some("tar.gz".into()),
            }],
            install: frank_protocol::ToolchainInstallSpec {
                directory: "demo".into(),
                executable: Some("demo".into()),
                environment: BTreeMap::new(),
            },
            checks: vec![frank_protocol::ToolchainCheck {
                id: "test".into(),
                program: "demo".into(),
                args: vec!["test".into()],
                environment: BTreeMap::new(),
                timeout_seconds: 10,
            }],
        }
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_resolves_preview() {
        let raw = toml::to_string(&manifest()).unwrap();
        let parsed = parse_manifest(&raw).unwrap();
        let plan = resolve_install_plan(&parsed, "macos-aarch64", "/tmp/frank").unwrap();
        assert_eq!(plan.version, "1.0.0");
        assert_eq!(plan.size_bytes, 4);
        assert_eq!(plan.checks, vec!["test"]);
        assert!(matches!(
            ToolchainRequirementStatus::NeedsApproval,
            ToolchainRequirementStatus::NeedsApproval
        ));
        assert!(parse_manifest(&format!("{raw}\nunknown = true\n")).is_err());
    }

    #[test]
    fn detection_and_checksum_are_deterministic() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("demo.toml"), "ok").unwrap();
        assert!(detect_project(directory.path(), &manifest().detect).detected);
        let mut artifact = manifest().artifacts.remove(0);
        let bytes = b"demo";
        artifact.size_bytes = bytes.len() as u64;
        let mut digest = Sha256::new();
        digest.update(bytes);
        artifact.sha256 = hex::encode(digest.finalize());
        assert!(verify_artifact(bytes, &artifact).is_ok());
    }

    #[test]
    fn builtins_have_structured_checks() {
        let builtins = builtin_manifests();
        assert_eq!(builtins.len(), 4);
        assert_eq!(builtins[0].version, "3.47.1");
        assert_eq!(
            builtins
                .iter()
                .map(|manifest| manifest.id.as_str())
                .collect::<Vec<_>>(),
            vec!["flutter", "odin", "dotnet", "cpp"]
        );
        assert!(builtins.iter().all(|manifest| {
            manifest.artifacts.iter().all(|artifact| {
                matches!(artifact.source, ToolchainArtifactSource::Host { .. })
                    && artifact.archive.is_none()
            })
        }));
        assert!(
            builtins
                .iter()
                .all(|manifest| validate_manifest(manifest).is_ok())
        );
    }

    #[test]
    fn remote_artifacts_require_an_exact_https_payload_and_platform() {
        let mut manifest = manifest();
        assert!(resolve_install_plan(&manifest, "linux-x86_64", "/tmp/frank").is_err());

        manifest.artifacts.push(ToolchainArtifact {
            platform: "linux-x86_64".into(),
            source: ToolchainArtifactSource::Url {
                url: "https://example.invalid/demo.tar.gz".into(),
            },
            sha256: "cd".repeat(32),
            size_bytes: 7,
            archive: Some("tar.gz".into()),
        });
        let plan = resolve_install_plan(&manifest, "linux-x86_64", "/tmp/frank").unwrap();
        assert_eq!(plan.source, "https://example.invalid/demo.tar.gz");
        assert_eq!(plan.sha256, "cd".repeat(32));
        assert_eq!(plan.size_bytes, 7);
        assert_eq!(plan.artifact, Some(manifest.artifacts[1].clone()));

        let mut invalid = manifest.clone();
        invalid.artifacts[1].source = ToolchainArtifactSource::Url {
            url: "http://example.invalid/demo.tar.gz".into(),
        };
        assert!(matches!(
            validate_manifest(&invalid),
            Err(ToolchainError::Manifest(message)) if message.contains("HTTPS")
        ));

        let mut duplicate_platform = manifest;
        duplicate_platform
            .artifacts
            .push(duplicate_platform.artifacts[1].clone());
        assert!(matches!(
            validate_manifest(&duplicate_platform),
            Err(ToolchainError::Manifest(message)) if message.contains("duplicated")
        ));
    }

    #[test]
    fn v1_resolution_keeps_host_tools_manual_and_url_tools_approval_gated() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("demo.toml"), "demo").unwrap();

        let host = host_manifest(
            "host-demo",
            "Host demo",
            "locked",
            vec!["demo.toml"],
            Vec::new(),
            "demo",
            vec![("check", vec!["test"])],
        );
        let host_view = resolve_requirement_without_host_probe(
            &host,
            project.path(),
            "macos-aarch64",
            project.path().join("data"),
        )
        .unwrap();
        assert_eq!(
            host_view.status,
            ToolchainRequirementStatus::ManualRequirement
        );
        assert!(
            host_view
                .diagnostic
                .as_deref()
                .is_some_and(|value| value.contains("host runner"))
        );

        let url_view = resolve_requirement_without_host_probe(
            &manifest(),
            project.path(),
            "macos-aarch64",
            project.path().join("data"),
        )
        .unwrap();
        assert_eq!(url_view.status, ToolchainRequirementStatus::NeedsApproval);

        let mut undetected = manifest();
        undetected.detect.files = vec!["missing.toml".into()];
        let undetected_view = resolve_requirement_without_host_probe(
            &undetected,
            project.path(),
            "macos-aarch64",
            project.path().join("data"),
        )
        .unwrap();
        assert_eq!(
            undetected_view.status,
            ToolchainRequirementStatus::Incompatible
        );
    }

    #[test]
    fn local_manifest_loading_is_sorted_and_rejects_duplicate_ids() {
        let directory = tempfile::tempdir().unwrap();
        let first = toml::to_string(&manifest()).unwrap();
        let mut second_manifest = manifest();
        second_manifest.id = "second".into();
        std::fs::write(
            directory.path().join("z.toml"),
            toml::to_string(&second_manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(directory.path().join("a.toml"), first).unwrap();
        let manifests = load_local_manifests(directory.path()).unwrap();
        assert_eq!(
            manifests
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["demo", "second"]
        );

        let duplicate = toml::to_string(&manifest()).unwrap();
        std::fs::write(directory.path().join("duplicate.toml"), duplicate).unwrap();
        assert!(matches!(
            load_local_manifests(directory.path()),
            Err(ToolchainError::Manifest(message)) if message.contains("duplicate local toolchain id")
        ));
    }
}
