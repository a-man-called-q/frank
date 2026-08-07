//! Repo-maintenance tasks that aren't part of the shipped `frank` binary.
//!
//! `build-packs` compiles every pack under `packs/*/pack.toml` and writes a
//! `compiled.rs` next to it containing `&'static str` constants, which
//! `frank-cli` `include!`s. This runs ahead of time for the built-in pack
//! (committed, CI fails on drift) and gives the pack compiler a second real
//! caller besides `frank pack add`, so it's exercised as a library from day
//! one rather than only through a future runtime path.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod coverage;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile every pack under packs/*/pack.toml into packs/<id>/compiled.rs.
    BuildPacks,
    /// Parse and validate every targets/*.toml manifest.
    LintTargets,
    /// Write dist/SHA256SUMS for every published CLI or desktop artifact.
    Checksums,
    /// Ensure CLI, GUI, and package metadata use one workspace version.
    VersionCheck,
    /// Validate the allowed direct dependency edges between Frank crates.
    ArchitectureCheck,
    /// Validate a cargo-llvm-cov JSON report against the repository policy.
    CoverageCheck {
        /// JSON report produced by cargo-llvm-cov.
        #[arg(long)]
        report: PathBuf,
        /// TOML policy containing per-package floors and uncovered ceilings.
        #[arg(long)]
        policy: PathBuf,
    },
    /// Package a release binary into dist/. Without --target this uses the
    /// host triple; CI passes an explicit target after cross-compiling it.
    Dist {
        #[arg(long)]
        target: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repo_root()?;
    match cli.command {
        Command::BuildPacks => build_packs(&root),
        Command::LintTargets => lint_targets(&root).map(|_| ()),
        Command::Checksums => checksums(&root),
        Command::VersionCheck => version_check(&root),
        Command::ArchitectureCheck => architecture_check(&root),
        Command::CoverageCheck { report, policy } => coverage::check(&report, &policy),
        Command::Dist { target } => dist(&root, target.as_deref()),
    }
}

/// Before the frank-gui -> iced migration, this compared the workspace
/// version against `apps/frank-gui/package.json` and `.../tauri.conf.json`
/// by hand -- two extra version files a GUI needed to stay in sync with
/// manually. `crates/frank-gui`'s `[package] version.workspace = true`
/// makes that whole class of drift structurally impossible for any
/// workspace member, so the check is repurposed: assert every member
/// actually uses the inherited version rather than hard-pinning its own.
fn version_check(root: &Path) -> Result<()> {
    let cargo_path = root.join("Cargo.toml");
    let cargo: toml::Value = toml::from_str(
        &std::fs::read_to_string(&cargo_path)
            .with_context(|| format!("reading {}", cargo_path.display()))?,
    )?;
    let expected = cargo
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .context("[workspace.package].version is missing")?;

    let members_dir = root.join("crates");
    let mut checked = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&members_dir)
        .with_context(|| format!("reading {}", members_dir.display()))?
        .collect::<std::io::Result<_>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let manifest_path = entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest: toml::Value = toml::from_str(
            &std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("reading {}", manifest_path.display()))?,
        )
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
        // `version.workspace = true` parses as `package.version` being a
        // table `{ workspace = true }`, not a bare boolean.
        let inherits_workspace = manifest
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true);
        if !inherits_workspace {
            anyhow::bail!(
                "{} does not use `version.workspace = true` -- every workspace \
                 member must inherit the workspace version rather than pin its own",
                manifest_path.display()
            );
        }
        checked.push(entry.file_name().to_string_lossy().into_owned());
    }

    println!(
        "xtask version-check: {} crate(s) inherit workspace version {expected}",
        checked.len()
    );
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .context("CARGO_MANIFEST_DIR not set — run via `cargo run -p xtask`")?;
    Ok(PathBuf::from(manifest_dir)
        .parent()
        .context("xtask has no parent directory")?
        .to_path_buf())
}

fn architecture_check(root: &Path) -> Result<()> {
    let packages = [
        ("frank-app", "crates/frank-app/Cargo.toml"),
        ("frank-cli", "crates/frank-cli/Cargo.toml"),
        ("frank-compress", "crates/frank-compress/Cargo.toml"),
        ("frank-ledger", "crates/frank-ledger/Cargo.toml"),
        ("frank-mcp", "crates/frank-mcp/Cargo.toml"),
        ("frank-pack", "crates/frank-pack/Cargo.toml"),
        ("frank-safeio", "crates/frank-safeio/Cargo.toml"),
        ("frank-state", "crates/frank-state/Cargo.toml"),
        ("frank-target", "crates/frank-target/Cargo.toml"),
        ("frank-release-cli", "crates/frank-release-cli/Cargo.toml"),
        ("frank-gui-core", "crates/frank-gui-core/Cargo.toml"),
        ("frank-gui", "crates/frank-gui/Cargo.toml"),
        ("xtask", "xtask/Cargo.toml"),
    ];

    let mut errors = Vec::new();
    for (package, relative) in packages {
        let path = root.join(relative);
        let manifest: toml::Value = toml::from_str(
            &std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| format!("parsing {}", path.display()))?;
        let actual = frank_dependencies(&manifest);
        let expected = expected_architecture_dependencies(package)
            .with_context(|| format!("no architecture policy for package {package}"))?;
        let expected = expected.iter().copied().collect::<BTreeSet<_>>();
        if actual != expected {
            errors.push(format!(
                "{package}: expected {expected:?}, found {actual:?}"
            ));
        }
    }

    if errors.is_empty() {
        println!("xtask architecture-check: dependency graph is within policy");
        Ok(())
    } else {
        for error in errors {
            eprintln!("FAIL {error}");
        }
        anyhow::bail!("dependency graph violates architecture policy");
    }
}

fn expected_architecture_dependencies(package: &str) -> Option<&'static [&'static str]> {
    Some(match package {
        "frank-app" => &[
            "frank-ledger",
            "frank-pack",
            "frank-safeio",
            "frank-state",
            "frank-target",
        ],
        "frank-cli" => &[
            "frank-app",
            "frank-compress",
            "frank-ledger",
            "frank-mcp",
            "frank-pack",
            "frank-safeio",
            "frank-state",
            "frank-target",
        ],
        "frank-compress" => &["frank-safeio"],
        "frank-ledger" => &["frank-pack", "frank-safeio", "frank-state"],
        "frank-mcp" => &["frank-compress"],
        "frank-pack" => &["frank-safeio"],
        "frank-safeio" => &[],
        "frank-state" => &["frank-pack", "frank-safeio"],
        "frank-target" => &["frank-pack", "frank-safeio"],
        "frank-release-cli" => &[],
        "frank-gui-core" => &["frank-app"],
        "frank-gui" => &["frank-app", "frank-gui-core", "frank-safeio"],
        "xtask" => &["frank-pack", "frank-target"],
        _ => return None,
    })
}

fn frank_dependencies(manifest: &toml::Value) -> BTreeSet<&str> {
    let mut dependencies = BTreeSet::new();
    let Some(table) = manifest.as_table() else {
        return dependencies;
    };
    for (section, value) in table {
        let is_dependency_section = matches!(
            section.as_str(),
            "dependencies" | "build-dependencies" | "dev-dependencies"
        ) || (section.starts_with("target.")
            && section.ends_with(".dependencies"));
        if !is_dependency_section {
            continue;
        }
        let Some(entries) = value.as_table() else {
            continue;
        };
        for name in entries.keys().filter(|name| name.starts_with("frank-")) {
            dependencies.insert(name.as_str());
        }
    }
    dependencies
}

fn build_packs(root: &Path) -> Result<()> {
    let packs_dir = root.join("packs");
    let mut built = 0usize;
    let mut changed = 0usize;
    for entry in
        std::fs::read_dir(&packs_dir).with_context(|| format!("reading {}", packs_dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let pack_dir = entry.path();
        if !pack_dir.join("pack.toml").is_file() {
            continue;
        }
        if build_one_pack(&pack_dir)? {
            changed = changed.saturating_add(1);
        }
        built += 1;
    }
    if built == 0 {
        anyhow::bail!("no packs found under {}", packs_dir.display());
    }
    println!("xtask build-packs: compiled {built} pack(s), {changed} generated output(s) changed");
    Ok(())
}

fn build_one_pack(pack_dir: &Path) -> Result<bool> {
    let source = frank_pack::PackSource::load(pack_dir)
        .with_context(|| format!("loading pack at {}", pack_dir.display()))?;
    let compiled = frank_pack::compile(&source)
        .with_context(|| format!("compiling pack at {}", pack_dir.display()))?;

    let rust_src = emit_rust(&compiled);
    let out_path = pack_dir.join("compiled.rs");
    let changed = std::fs::read_to_string(&out_path).ok().as_deref() != Some(rust_src.as_str());
    std::fs::write(&out_path, &rust_src)
        .with_context(|| format!("writing {}", out_path.display()))?;

    for (id, level) in &compiled.levels {
        println!(
            "  {}/{id}: activation {}B, reinforce {}B",
            compiled.id,
            level.activation_prompt.len(),
            level.reinforce.len()
        );
    }
    if changed {
        println!("  -> {} (changed)", out_path.display());
    }
    Ok(changed)
}

/// Emit `compiled.rs`: plain Rust source with `&'static str` constants, so
/// the activation prompt is a `.rodata` slice at runtime — zero parse cost,
/// zero allocation. Compare to the archive's SessionStart hook, which does a
/// `readFileSync`, a frontmatter regex, and a 78-line filtering reduce on
/// every session start (`archive/src/hooks/caveman-activate.js:91-116`).
fn emit_rust(pack: &frank_pack::CompiledPack) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "// @generated by `cargo run -p xtask -- build-packs`. Do not edit by hand —"
    )
    .unwrap();
    writeln!(
        out,
        "// edit the pack.toml / fragment files next to this and regenerate."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub const PACK_ID: &str = {:?};", pack.id).unwrap();
    writeln!(out, "pub const PACK_VERSION: &str = {:?};", pack.version).unwrap();
    writeln!(
        out,
        "pub const DEFAULT_LEVEL: &str = {:?};",
        pack.default_level
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub struct Level {{").unwrap();
    writeln!(out, "    pub id: &'static str,").unwrap();
    writeln!(out, "    pub title: Option<&'static str>,").unwrap();
    writeln!(out, "    pub aliases: &'static [&'static str],").unwrap();
    writeln!(out, "    pub activation_prompt: &'static str,").unwrap();
    writeln!(out, "    pub reinforce: &'static str,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub static LEVELS: &[Level] = &[").unwrap();
    for level in pack.levels.values() {
        writeln!(out, "    Level {{").unwrap();
        writeln!(out, "        id: {:?},", level.id).unwrap();
        writeln!(out, "        title: {},", opt_str(level.title.as_deref())).unwrap();
        writeln!(out, "        aliases: &[{}],", str_slice(&level.aliases)).unwrap();
        writeln!(
            out,
            "        activation_prompt: {:?},",
            level.activation_prompt
        )
        .unwrap();
        writeln!(out, "        reinforce: {:?},", level.reinforce).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub struct AliasEntry {{").unwrap();
    writeln!(out, "    pub alias: &'static str,").unwrap();
    writeln!(out, "    pub canonical: &'static str,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub static ALIASES: &[AliasEntry] = &[").unwrap();
    for (alias, canonical) in &pack.aliases {
        writeln!(
            out,
            "    AliasEntry {{ alias: {alias:?}, canonical: {canonical:?} }},"
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub struct Oneshot {{").unwrap();
    writeln!(out, "    pub id: &'static str,").unwrap();
    writeln!(out, "    pub prompt: &'static str,").unwrap();
    writeln!(out, "    pub restores_previous: bool,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub static ONESHOTS: &[Oneshot] = &[").unwrap();
    for o in pack.oneshots.values() {
        writeln!(out, "    Oneshot {{").unwrap();
        writeln!(out, "        id: {:?},", o.id).unwrap();
        writeln!(out, "        prompt: {:?},", o.prompt).unwrap();
        writeln!(out, "        restores_previous: {},", o.restores_previous).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub struct Activation {{").unwrap();
    writeln!(out, "    pub on: &'static [&'static str],").unwrap();
    writeln!(out, "    pub off: &'static [&'static str],").unwrap();
    writeln!(out, "    pub question_guard: Option<&'static str>,").unwrap();
    writeln!(out, "    pub command_prefix: Option<&'static str>,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub static ACTIVATION: Activation = Activation {{").unwrap();
    writeln!(out, "    on: &[{}],", str_slice(&pack.activation.on)).unwrap();
    writeln!(out, "    off: &[{}],", str_slice(&pack.activation.off)).unwrap();
    writeln!(
        out,
        "    question_guard: {},",
        opt_str(pack.activation.question_guard.as_deref())
    )
    .unwrap();
    writeln!(
        out,
        "    command_prefix: {},",
        opt_str(pack.activation.command_prefix.as_deref())
    )
    .unwrap();
    writeln!(out, "}};").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "pub struct Reduction {{").unwrap();
    writeln!(out, "    pub level: &'static str,").unwrap();
    writeln!(out, "    pub mean: f64,").unwrap();
    writeln!(out, "    pub p25: Option<f64>,").unwrap();
    writeln!(out, "    pub p75: Option<f64>,").unwrap();
    writeln!(out, "    pub n: Option<u32>,").unwrap();
    writeln!(out, "    pub model: Option<&'static str>,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub static BENCHMARK: &[Reduction] = &[").unwrap();
    for (level, r) in &pack.benchmark {
        writeln!(out, "    Reduction {{").unwrap();
        writeln!(out, "        level: {level:?},").unwrap();
        writeln!(out, "        mean: {:?},", r.mean).unwrap();
        writeln!(out, "        p25: {},", opt_f64(r.p25)).unwrap();
        writeln!(out, "        p75: {},", opt_f64(r.p75)).unwrap();
        writeln!(out, "        n: {},", opt_u32(r.n)).unwrap();
        writeln!(out, "        model: {},", opt_str(r.model.as_deref())).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();

    out
}

fn opt_str(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("Some({s:?})"),
        None => "None".to_string(),
    }
}

fn opt_f64(v: Option<f64>) -> String {
    match v {
        Some(n) => format!("Some({n:?})"),
        None => "None".to_string(),
    }
}

fn opt_u32(v: Option<u32>) -> String {
    match v {
        Some(n) => format!("Some({n})"),
        None => "None".to_string(),
    }
}

fn str_slice(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A manifest path field must expand under `$HOME` or be project-relative
/// (`./...`) — never an absolute path elsewhere on disk. This is enforced
/// here, at lint time, rather than at install time: a manifest that tried
/// to write outside those two roots should fail CI, not merely warn a
/// user at install.
fn check_path_scope(field: &str, raw: &str, errors: &mut Vec<String>) {
    let (prefix, remainder) = if let Some(rest) = raw.strip_prefix("$HOME") {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            ("$HOME", rest)
        } else {
            ("", raw)
        }
    } else if let Some(rest) = raw.strip_prefix("./") {
        ("./", rest)
    } else {
        ("", raw)
    };
    if !prefix.is_empty()
        && !remainder
            .split(['/', '\\'])
            .any(|component| component == "..")
    {
        return;
    }
    errors.push(format!(
        "{field} = \"{raw}\" must start with \"$HOME\" or \"./\""
    ));
}

fn lint_targets(root: &Path) -> Result<usize> {
    let targets_dir = root.join("targets");
    if !targets_dir.is_dir() {
        println!("xtask lint-targets: no targets/ directory yet");
        return Ok(0);
    }
    let mut checked = 0usize;
    let mut had_error = false;
    for entry in std::fs::read_dir(&targets_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;

        let manifest: frank_target::TargetManifest = match toml::from_str(&raw) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("FAIL {}: {e}", path.display());
                had_error = true;
                continue;
            }
        };

        let mut errors = Vec::new();
        for clause in &manifest.detect {
            if let Some(d) = &clause.dir {
                check_path_scope("detect.dir", d, &mut errors);
            }
            if let Some(f) = &clause.file {
                check_path_scope("detect.file", f, &mut errors);
            }
        }
        match &manifest.install {
            frank_target::InstallSpec::MarkdownBlock { markdown } => {
                check_path_scope("install.markdown.path", &markdown.path, &mut errors);
            }
            frank_target::InstallSpec::SettingsMerge { settings } => {
                check_path_scope("install.settings.path", &settings.path, &mut errors);
            }
            frank_target::InstallSpec::Files { file } => {
                for f in file {
                    check_path_scope("install.file.path", &f.path, &mut errors);
                }
            }
            frank_target::InstallSpec::Spawn { .. } => {}
        }
        if manifest.detect.is_empty() && !matches!(manifest.target.kind.as_str(), "native") {
            errors.push("no [[detect]] clauses — target can never be auto-detected".to_string());
        }

        if errors.is_empty() {
            println!(
                "OK   {} ({}, {}{})",
                manifest.target.id,
                manifest.target.kind,
                if manifest.target.verified {
                    "verified"
                } else {
                    "unverified"
                },
                if manifest.target.soft { ", soft" } else { "" }
            );
            checked += 1;
        } else {
            had_error = true;
            eprintln!("FAIL {} ({}):", manifest.target.id, path.display());
            for e in errors {
                eprintln!("  - {e}");
            }
        }
    }
    println!("xtask lint-targets: {checked} manifest(s) OK");
    if had_error {
        anyhow::bail!("one or more target manifests failed linting");
    }
    Ok(checked)
}

/// `rustc -vV`'s `host:` line — there's no `std` constant for "the triple
/// this binary happens to be running on" outside a build script, and a
/// build-script dependency isn't worth it for one xtask command.
fn host_triple() -> Result<String> {
    let out = ProcessCommand::new("rustc")
        .arg("-vV")
        .output()
        .context("running `rustc -vV`")?;
    if !out.status.success() {
        anyhow::bail!("`rustc -vV` exited with {:?}", out.status.code());
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(str::to_string)
        .context("no `host:` line in `rustc -vV` output")
}

/// Packages a release binary for the host or an explicitly supplied target.
/// Cross-compilation itself remains a CI concern; this function only packages
/// a binary that was already built by Cargo.
fn dist(root: &Path, requested_target: Option<&str>) -> Result<()> {
    let triple = requested_target
        .map(str::to_string)
        .unwrap_or(host_triple()?);
    let binary_name = binary_name(&triple);
    let release_dir = match requested_target {
        Some(target) => root.join("target").join(target).join("release"),
        None => root.join("target/release"),
    };
    let release_bin = release_dir.join(binary_name);
    if !release_bin.exists() {
        anyhow::bail!(
            "release binary not found at {} — run `cargo build --release -p frank-cli` first",
            release_bin.display()
        );
    }

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir)?;
    let archive_name = archive_name(&triple);
    let archive_path = dist_dir.join(&archive_name);

    let status = if triple.contains("windows") {
        package_zip(&release_bin, &archive_path)?
    } else {
        package_tar_gz(&release_dir, binary_name, &archive_path)?
    };
    if !status.success() {
        anyhow::bail!("packaging command exited with {:?}", status.code());
    }
    if !archive_path.is_file() {
        anyhow::bail!(
            "packaging command succeeded but did not create {}",
            archive_path.display()
        );
    }

    println!("xtask dist: wrote {}", archive_path.display());
    Ok(())
}

fn binary_name(triple: &str) -> &'static str {
    if triple.contains("windows") {
        "frank.exe"
    } else {
        "frank"
    }
}

fn archive_name(triple: &str) -> String {
    if triple.contains("windows") {
        format!("frank-{triple}.zip")
    } else {
        format!("frank-{triple}.tar.gz")
    }
}

fn package_tar_gz(
    release_dir: &Path,
    binary_name: &str,
    archive_path: &Path,
) -> Result<std::process::ExitStatus> {
    let mut command = ProcessCommand::new("tar");

    // A release archive should not change just because it was packaged again.
    // BSD tar (the default on macOS) stores the current time in gzip headers;
    // this option disables that timestamp. COPYFILE_DISABLE prevents macOS
    // resource-fork metadata from becoming an invisible archive input.
    #[cfg(target_os = "macos")]
    command.args(["--options", "gzip:!timestamp"]);

    // GNU tar accepts these flags and normalizes the member metadata. GZIP=-n
    // covers the compressor header on GNU tar versions that do not inherit
    // the normalized mtime from the archive format.
    #[cfg(target_os = "linux")]
    command.args([
        "--sort=name",
        "--mtime=@0",
        "--owner=0",
        "--group=0",
        "--numeric-owner",
    ]);

    let status = command
        .env("COPYFILE_DISABLE", "1")
        .env("GZIP", "-n")
        .args(["-czf"])
        .arg(archive_path)
        .arg("-C")
        .arg(release_dir)
        .arg(binary_name)
        .status()
        .context("running tar")?;
    Ok(status)
}

fn package_zip(binary_path: &Path, archive_path: &Path) -> Result<std::process::ExitStatus> {
    let script = format!(
        "Compress-Archive -LiteralPath {} -DestinationPath {} -Force",
        powershell_quote(binary_path),
        powershell_quote(archive_path),
    );
    ProcessCommand::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .context("running PowerShell Compress-Archive")
}

fn powershell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn checksums(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};

    let dist_dir = root.join("dist");
    if !dist_dir.is_dir() {
        anyhow::bail!(
            "{} does not exist — run `xtask dist` first",
            dist_dir.display()
        );
    }

    let mut lines = Vec::new();
    for entry in std::fs::read_dir(&dist_dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_artifact = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            [".tar.gz", ".zip", ".dmg", ".msi", ".deb", ".rpm"]
                .iter()
                .any(|suffix| n.ends_with(suffix))
        });
        if !is_artifact {
            continue;
        }
        if !path.is_file() {
            anyhow::bail!("archive path is not a regular file: {}", path.display());
        }
        let bytes = std::fs::read(&path)?;
        let digest = Sha256::digest(&bytes);
        let mut hex = String::with_capacity(64);
        for byte in digest {
            write!(&mut hex, "{byte:02x}").expect("writing into a String cannot fail");
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        lines.push(format!("{hex}  {name}"));
    }
    lines.sort();

    if lines.is_empty() {
        anyhow::bail!("no published artifacts found in {}", dist_dir.display());
    }

    let manifest_path = dist_dir.join("SHA256SUMS");
    std::fs::write(&manifest_path, lines.join("\n") + "\n")?;
    println!(
        "xtask checksums: wrote {} ({} entries)",
        manifest_path.display(),
        lines.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::fs;
    use std::io::Write as _;
    use std::path::Path;
    use std::process::Command as ProcessCommand;

    use super::{
        architecture_check, archive_name, binary_name, build_one_pack, build_packs,
        check_path_scope, checksums, dist, frank_dependencies, lint_targets, opt_f64, opt_str,
        opt_u32, package_zip, powershell_quote, str_slice, version_check,
    };

    fn minimal_pack(root: &Path, id: &str) {
        let pack = root.join("packs").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("pack.toml"),
            r#"
schema = 1
[pack]
id = "demo"
version = "1.0.0"
default_level = "full"
[fragments]
core = { file = "core.md" }
[[level]]
id = "full"
compose = ["core"]
reinforce = "demo active"
[activation]
on = []
off = []
"#,
        )
        .unwrap();
        fs::write(pack.join("core.md"), "Keep output concise.").unwrap();
    }

    #[test]
    fn unix_artifacts_use_tar_gzip_and_unix_binary_name() {
        assert_eq!(binary_name("aarch64-apple-darwin"), "frank");
        assert_eq!(
            archive_name("aarch64-apple-darwin"),
            "frank-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn generated_literal_helpers_preserve_optional_values_and_slices() {
        assert_eq!(opt_str(Some("demo")), "Some(\"demo\")");
        assert_eq!(opt_str(None), "None");
        assert_eq!(opt_f64(Some(0.65)), "Some(0.65)");
        assert_eq!(opt_f64(None), "None");
        assert_eq!(opt_u32(Some(3)), "Some(3)");
        assert_eq!(opt_u32(None), "None");
        assert_eq!(str_slice(&["a".into(), "b\"c".into()]), "\"a\", \"b\\\"c\"");
    }

    #[test]
    fn host_triple_is_reported_by_rustc() {
        let host = super::host_triple().unwrap();
        let raw = ProcessCommand::new("rustc").args(["-vV"]).output().unwrap();
        let raw_stdout = String::from_utf8_lossy(&raw.stdout);
        let expected = raw_stdout
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .unwrap();
        assert_eq!(host, expected);
    }

    #[test]
    fn architecture_policy_accepts_the_workspace_graph() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        architecture_check(root).unwrap();
    }

    #[test]
    fn architecture_policy_rejects_a_forbidden_dependency_edge() {
        let root = tempfile::tempdir().unwrap();
        let packages = [
            (
                "frank-app",
                "crates/frank-app/Cargo.toml",
                &[
                    "frank-ledger",
                    "frank-pack",
                    "frank-safeio",
                    "frank-state",
                    "frank-target",
                ][..],
            ),
            (
                "frank-cli",
                "crates/frank-cli/Cargo.toml",
                &[
                    "frank-app",
                    "frank-compress",
                    "frank-ledger",
                    "frank-mcp",
                    "frank-pack",
                    "frank-safeio",
                    "frank-state",
                    "frank-target",
                ][..],
            ),
            (
                "frank-compress",
                "crates/frank-compress/Cargo.toml",
                &["frank-safeio"][..],
            ),
            (
                "frank-ledger",
                "crates/frank-ledger/Cargo.toml",
                &["frank-pack", "frank-safeio", "frank-state"][..],
            ),
            (
                "frank-mcp",
                "crates/frank-mcp/Cargo.toml",
                &["frank-compress"][..],
            ),
            (
                "frank-pack",
                "crates/frank-pack/Cargo.toml",
                &["frank-safeio"][..],
            ),
            ("frank-safeio", "crates/frank-safeio/Cargo.toml", &[][..]),
            (
                "frank-state",
                "crates/frank-state/Cargo.toml",
                &["frank-pack", "frank-safeio"][..],
            ),
            (
                "frank-target",
                "crates/frank-target/Cargo.toml",
                &["frank-pack", "frank-safeio"][..],
            ),
            (
                "frank-gui-core",
                "crates/frank-gui-core/Cargo.toml",
                &["frank-app"][..],
            ),
            (
                "frank-gui",
                "crates/frank-gui/Cargo.toml",
                &["frank-app", "frank-gui-core", "frank-safeio"][..],
            ),
            (
                "xtask",
                "xtask/Cargo.toml",
                &["frank-pack", "frank-target"][..],
            ),
        ];
        for (_, relative, dependencies) in packages {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut manifest = String::from("[dependencies]\n");
            for dependency in dependencies {
                writeln!(manifest, "{dependency} = \"0.1.0\"").unwrap();
            }
            fs::write(path, manifest).unwrap();
        }
        let forbidden = root.path().join("crates/frank-pack/Cargo.toml");
        fs::OpenOptions::new()
            .append(true)
            .open(forbidden)
            .unwrap()
            .write_all(b"frank-app = \"0.1.0\"\n")
            .unwrap();

        assert!(architecture_check(root.path()).is_err());
    }

    #[test]
    fn architecture_parser_ignores_non_target_dependency_sections() {
        let manifest: toml::Value =
            toml::from_str("[\"metadata.dependencies\"]\nfrank-app = \"0.1.0\"\n").unwrap();
        let dependencies = frank_dependencies(&manifest);
        assert!(dependencies.is_empty());
        assert!(!dependencies.contains("frank-app"));
    }

    #[test]
    fn windows_artifacts_use_zip_and_exe_binary_name() {
        assert_eq!(binary_name("x86_64-pc-windows-msvc"), "frank.exe");
        assert_eq!(
            archive_name("x86_64-pc-windows-msvc"),
            "frank-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn target_path_lint_rejects_prefix_spoofing_and_parent_escape() {
        for raw in ["$HOMEfoo/file", "$HOME/../outside", "./../../outside"] {
            let mut errors = Vec::new();
            check_path_scope("path", raw, &mut errors);
            assert!(
                !errors.is_empty(),
                "unsafe path unexpectedly accepted: {raw}"
            );
        }
        for raw in ["$HOME/.config/frank", "./AGENTS.md"] {
            let mut errors = Vec::new();
            check_path_scope("path", raw, &mut errors);
            assert!(errors.is_empty(), "safe path unexpectedly rejected: {raw}");
        }
    }

    #[test]
    fn build_packs_compiles_only_pack_directories() {
        let root = tempfile::tempdir().unwrap();
        minimal_pack(root.path(), "demo");
        build_packs(root.path()).unwrap();
        let generated = root.path().join("packs/demo/compiled.rs");
        assert!(generated.is_file());
        assert!(
            fs::read_to_string(generated)
                .unwrap()
                .contains("pub const PACK_ID: &str = \"demo\";")
        );
        assert!(!build_one_pack(&root.path().join("packs/demo")).unwrap());
    }

    #[test]
    fn lint_targets_accepts_valid_manifest_and_rejects_unsafe_path() {
        let root = tempfile::tempdir().unwrap();
        let targets = root.path().join("targets");
        fs::create_dir_all(&targets).unwrap();
        fs::write(
            targets.join("valid.toml"),
            r#"
schema = 1
[target]
id = "demo"
label = "Demo"
kind = "generic"
verified = false
soft = true
[[detect]]
dir = "$HOME/.demo"
[install]
strategy = "markdown-block"
[install.markdown]
path = "./AGENTS.md"
begin = "<!-- frank:begin demo -->"
end = "<!-- frank:end demo -->"
body = "demo"
create_if_missing = true
"#,
        )
        .unwrap();
        assert_eq!(lint_targets(root.path()).unwrap(), 1);
        fs::copy(
            targets.join("valid.toml"),
            targets.join("valid-second.toml"),
        )
        .unwrap();
        assert_eq!(lint_targets(root.path()).unwrap(), 2);

        fs::write(
            targets.join("unsafe.toml"),
            fs::read_to_string(targets.join("valid.toml"))
                .unwrap()
                .replace("./AGENTS.md", "$HOME/../outside"),
        )
        .unwrap();
        assert!(lint_targets(root.path()).is_err());
    }

    #[test]
    fn version_check_and_checksum_generation_are_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        let crates_dir = root.path().join("crates");
        fs::create_dir_all(crates_dir.join("frank-gui")).unwrap();
        fs::write(
            crates_dir.join("frank-gui/Cargo.toml"),
            "[package]\nname = \"frank-gui\"\nversion.workspace = true\n",
        )
        .unwrap();
        version_check(root.path()).unwrap();

        // A crate that hard-pins its own version instead of inheriting the
        // workspace one must fail closed, not silently pass.
        fs::write(
            crates_dir.join("frank-gui/Cargo.toml"),
            "[package]\nname = \"frank-gui\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        assert!(version_check(root.path()).is_err());

        let dist_dir = root.path().join("dist");
        fs::create_dir_all(&dist_dir).unwrap();
        fs::write(dist_dir.join("frank-demo.tar.gz"), b"artifact").unwrap();
        checksums(root.path()).unwrap();
        let checksums = fs::read_to_string(dist_dir.join("SHA256SUMS")).unwrap();
        assert!(checksums.contains("frank-demo.tar.gz"));
    }

    #[test]
    fn dist_reports_missing_release_binary_and_quotes_powershell_paths() {
        let root = tempfile::tempdir().unwrap();
        let error = dist(root.path(), Some("x86_64-pc-windows-msvc")).unwrap_err();
        assert!(error.to_string().contains("release binary not found"));
        assert_eq!(powershell_quote(Path::new("a'b")), "'a''b'");
    }

    #[cfg(unix)]
    #[test]
    fn package_zip_invokes_powershell_archiver() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let marker = root.path().join("powershell-ran");
        let powershell = bin.join("powershell");
        fs::write(
            &powershell,
            "#!/bin/sh\ntouch \"$FRANK_TEST_ZIP_MARKER\"\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&powershell, fs::Permissions::from_mode(0o755)).unwrap();

        let previous_path = std::env::var_os("PATH");
        let previous_marker = std::env::var_os("FRANK_TEST_ZIP_MARKER");
        let path = match &previous_path {
            Some(value) => format!("{}:{}", bin.display(), value.to_string_lossy()),
            None => bin.display().to_string(),
        };
        unsafe {
            std::env::set_var("PATH", path);
            std::env::set_var("FRANK_TEST_ZIP_MARKER", &marker);
        }
        let status =
            package_zip(&root.path().join("frank"), &root.path().join("frank.zip")).unwrap();
        match previous_path {
            Some(value) => unsafe { std::env::set_var("PATH", value) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        match previous_marker {
            Some(value) => unsafe { std::env::set_var("FRANK_TEST_ZIP_MARKER", value) },
            None => unsafe { std::env::remove_var("FRANK_TEST_ZIP_MARKER") },
        }

        assert!(status.success());
        assert!(marker.is_file());
    }

    #[test]
    fn dist_packages_an_existing_unix_release_binary() {
        let root = tempfile::tempdir().unwrap();
        let release = root.path().join("target/aarch64-apple-darwin/release");
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("frank"), b"fake binary").unwrap();
        dist(root.path(), Some("aarch64-apple-darwin")).unwrap();
        assert!(
            root.path()
                .join("dist/frank-aarch64-apple-darwin.tar.gz")
                .is_file()
        );
    }

    #[test]
    fn lint_requires_detection_for_non_native_targets() {
        let root = tempfile::tempdir().unwrap();
        let targets = root.path().join("targets");
        fs::create_dir_all(&targets).unwrap();
        fs::write(
            targets.join("missing-detect.toml"),
            r#"
schema = 1
[target]
id = "demo"
label = "Demo"
kind = "generic"
verified = false
soft = true
[install]
strategy = "spawn"
[install.spawn]
command = "echo"
args = ["demo"]
"#,
        )
        .unwrap();
        assert!(lint_targets(root.path()).is_err());
    }
}
