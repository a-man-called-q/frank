//! Turns a [`PackSource`] on disk into a [`CompiledPack`]: one flat,
//! byte-budget-checked prompt string per level.
//!
//! Pipeline: parse → resolve `inherits` chains → resolve each level's
//! `compose` token list against `[fragments]` / `@rules` / `@examples` →
//! normalize (strip HTML comments, flatten markdown heading markers, collapse
//! blank runs) → enforce `[pack.budget]` → validate the pack-wide activation
//! regexes compile. Runs both ahead-of-time (`xtask build-packs`, for the
//! built-in pack — see `packs/caveman/compiled.rs`) and at runtime (`frank
//! pack add`, for third-party packs), which is why this lives in a plain
//! library function rather than a `build.rs`.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::error::{PackError, Result};
use crate::manifest::{ActivationDef, Example, Fragment, LevelDef, PackManifest, ReductionStat};

pub struct PackSource {
    pub root: PathBuf,
    pub manifest: PackManifest,
}

impl PackSource {
    pub fn load(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join("pack.toml");
        let metadata =
            std::fs::symlink_metadata(dir).map_err(|e| PackError::Io(dir.to_path_buf(), e))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PackError::UnsafePath(dir.display().to_string()));
        }
        if std::fs::symlink_metadata(&manifest_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(PackError::UnsafePath("pack.toml".to_string()));
        }
        let raw = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PackError::Io(manifest_path.clone(), e))?;
        let manifest: PackManifest = toml::from_str(&raw)
            .map_err(|e| PackError::Toml(manifest_path.clone(), Box::new(e)))?;
        Ok(PackSource {
            root: dir.to_path_buf(),
            manifest,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CompiledLevel {
    pub id: String,
    pub title: Option<String>,
    pub aliases: Vec<String>,
    pub activation_prompt: String,
    pub reinforce: String,
}

#[derive(Debug, Clone)]
pub struct CompiledOneshot {
    pub id: String,
    pub prompt: String,
    pub restores_previous: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledActivation {
    pub on: Vec<String>,
    pub off: Vec<String>,
    pub question_guard: Option<String>,
    pub command_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompiledPack {
    pub id: String,
    pub version: String,
    pub default_level: String,
    /// Canonical level id -> compiled level.
    pub levels: BTreeMap<String, CompiledLevel>,
    /// Alias -> canonical level id.
    pub aliases: BTreeMap<String, String>,
    pub oneshots: BTreeMap<String, CompiledOneshot>,
    pub activation: CompiledActivation,
    pub benchmark: BTreeMap<String, ReductionStat>,
}

impl CompiledPack {
    /// Resolve a user-facing level name (canonical id or alias) to its
    /// compiled level.
    pub fn resolve_level(&self, name: &str) -> Option<&CompiledLevel> {
        if let Some(level) = self.levels.get(name) {
            return Some(level);
        }
        let canonical = self.aliases.get(name)?;
        self.levels.get(canonical)
    }

    /// Every value that may legally appear in the active flag file:
    /// canonical level ids, one-shot ids, and the `off` sentinel. One-shots
    /// must be included — a statusline/session-start read that rejected
    /// them would collapse an in-flight one-shot to `off` before its
    /// restore turn runs.
    pub fn valid_flag_values(&self) -> Vec<&str> {
        let mut values = self.levels.keys().map(String::as_str).collect::<Vec<_>>();
        values.extend(self.oneshots.keys().map(String::as_str));
        values.push("off");
        values
    }
}

/// A level with every inheritable field filled in from its `inherits` chain.
/// `aliases` and `lang_hint` are identity-specific and never inherited.
#[derive(Debug, Clone, Default)]
struct Resolved {
    title: Option<String>,
    compose: Vec<String>,
    rules: Option<String>,
    examples: Vec<Example>,
    reinforce: Option<String>,
}

pub fn compile(source: &PackSource) -> Result<CompiledPack> {
    let manifest = &source.manifest;
    if manifest.schema != 1 {
        return Err(PackError::UnsupportedSchema(manifest.schema));
    }
    let mut seen_ids = HashSet::new();
    for l in &manifest.levels {
        if !seen_ids.insert(l.id.as_str()) {
            return Err(PackError::DuplicateLevelId(l.id.clone()));
        }
    }
    let defs: BTreeMap<&str, &LevelDef> =
        manifest.levels.iter().map(|l| (l.id.as_str(), l)).collect();

    let mut resolved_cache: BTreeMap<String, Resolved> = BTreeMap::new();
    for l in &manifest.levels {
        resolve_level(&l.id, &defs, &mut resolved_cache, &mut HashSet::new())?;
    }

    let mut aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut levels: BTreeMap<String, CompiledLevel> = BTreeMap::new();

    for l in &manifest.levels {
        let resolved = resolved_cache.get(&l.id).expect("resolved above").clone();

        let activation_body = render_compose(&source.root, &manifest.fragments, &l.id, &resolved)?;
        let activation_prompt = normalize(&activation_body);
        let reinforce = resolved
            .reinforce
            .as_deref()
            .map(normalize)
            .unwrap_or_default();

        if let Some(budget) = manifest.pack.budget {
            if let Some(limit) = budget.max_activation_bytes {
                if activation_prompt.len() > limit {
                    return Err(PackError::BudgetExceeded {
                        level: l.id.clone(),
                        kind: "activation prompt",
                        limit,
                        actual: activation_prompt.len(),
                    });
                }
            }
            if let Some(limit) = budget.max_reinforce_bytes {
                if reinforce.len() > limit {
                    return Err(PackError::BudgetExceeded {
                        level: l.id.clone(),
                        kind: "reinforce prompt",
                        limit,
                        actual: reinforce.len(),
                    });
                }
            }
        }

        for alias in &l.aliases {
            if let Some(existing) = aliases.insert(alias.clone(), l.id.clone()) {
                return Err(PackError::DuplicateAlias(alias.clone(), existing));
            }
        }

        levels.insert(
            l.id.clone(),
            CompiledLevel {
                id: l.id.clone(),
                title: resolved.title.clone(),
                aliases: l.aliases.clone(),
                activation_prompt,
                reinforce,
            },
        );
    }

    if !levels.contains_key(&manifest.pack.default_level) {
        return Err(PackError::UnknownDefaultLevel(
            manifest.pack.default_level.clone(),
        ));
    }

    let mut oneshots = BTreeMap::new();
    for o in &manifest.oneshots {
        let path = safe_pack_path(&source.root, &o.prompt)?;
        let raw = std::fs::read_to_string(&path).map_err(|e| PackError::Io(path.clone(), e))?;
        oneshots.insert(
            o.id.clone(),
            CompiledOneshot {
                id: o.id.clone(),
                prompt: normalize(&raw),
                restores_previous: o.restores_previous,
            },
        );
    }

    let activation = compile_activation(manifest.activation.as_ref())?;

    let benchmark = manifest
        .benchmark
        .as_ref()
        .map(|b| b.reduction.clone())
        .unwrap_or_default();

    Ok(CompiledPack {
        id: manifest.pack.id.clone(),
        version: manifest.pack.version.clone(),
        default_level: manifest.pack.default_level.clone(),
        levels,
        aliases,
        oneshots,
        activation,
        benchmark,
    })
}

fn resolve_level<'a>(
    id: &str,
    defs: &BTreeMap<&'a str, &'a LevelDef>,
    cache: &mut BTreeMap<String, Resolved>,
    visiting: &mut HashSet<String>,
) -> Result<Resolved> {
    if let Some(r) = cache.get(id) {
        return Ok(r.clone());
    }
    if !visiting.insert(id.to_string()) {
        return Err(PackError::InheritanceCycle(id.to_string()));
    }

    let def = *defs
        .get(id)
        .ok_or_else(|| PackError::UnknownParent(id.to_string(), id.to_string()))?;

    let parent = match &def.inherits {
        Some(parent_id) => {
            if !defs.contains_key(parent_id.as_str()) {
                return Err(PackError::UnknownParent(id.to_string(), parent_id.clone()));
            }
            Some(resolve_level(parent_id, defs, cache, visiting)?)
        }
        None => None,
    };

    let resolved = Resolved {
        title: def
            .title
            .clone()
            .or_else(|| parent.as_ref().and_then(|p| p.title.clone())),
        compose: def
            .compose
            .clone()
            .or_else(|| parent.as_ref().map(|p| p.compose.clone()))
            .unwrap_or_default(),
        rules: def
            .rules
            .clone()
            .or_else(|| parent.as_ref().and_then(|p| p.rules.clone())),
        examples: if def.examples.is_empty() {
            parent
                .as_ref()
                .map(|p| p.examples.clone())
                .unwrap_or_default()
        } else {
            def.examples.clone()
        },
        reinforce: def
            .reinforce
            .clone()
            .or_else(|| parent.as_ref().and_then(|p| p.reinforce.clone())),
    };

    visiting.remove(id);
    cache.insert(id.to_string(), resolved.clone());
    Ok(resolved)
}

fn render_compose(
    root: &Path,
    fragments: &BTreeMap<String, Fragment>,
    level_id: &str,
    resolved: &Resolved,
) -> Result<String> {
    let mut parts = Vec::with_capacity(resolved.compose.len());
    for token in &resolved.compose {
        let piece = match token.as_str() {
            "@rules" => {
                let rules_rel = resolved
                    .rules
                    .as_deref()
                    .ok_or_else(|| PackError::MissingRules(level_id.to_string()))?;
                let path = safe_pack_path(root, rules_rel)?;
                std::fs::read_to_string(&path).map_err(|e| PackError::Io(path.clone(), e))?
            }
            "@examples" => render_examples(&resolved.examples),
            name => match fragments.get(name) {
                Some(Fragment::Inline { inline }) => inline.clone(),
                Some(Fragment::File { file }) => {
                    let path = safe_pack_path(root, file)?;
                    std::fs::read_to_string(&path).map_err(|e| PackError::Io(path.clone(), e))?
                }
                None => {
                    return Err(PackError::UnknownFragment(
                        level_id.to_string(),
                        name.to_string(),
                    ));
                }
            },
        };
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    Ok(parts.join("\n\n"))
}

fn safe_pack_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(PackError::UnsafePath(relative.to_string()));
    }
    // Check every component, not only the final file. A symlinked `levels/`
    // directory would otherwise make `levels/full.md` look like an ordinary
    // file while allowing a pack to read outside its root during preview.
    let mut path = root.to_path_buf();
    for component in candidate.components() {
        let std::path::Component::Normal(name) = component else {
            continue;
        };
        path.push(name);
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(PackError::UnsafePath(relative.to_string()));
            }
        }
    }
    Ok(path)
}

fn render_examples(examples: &[Example]) -> String {
    examples
        .iter()
        .map(|e| format!("Ex: {} | {}", e.q.trim(), e.a.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compile_activation(def: Option<&ActivationDef>) -> Result<CompiledActivation> {
    let Some(def) = def else {
        return Ok(CompiledActivation::default());
    };
    for pattern in def
        .on
        .iter()
        .chain(def.off.iter())
        .chain(def.question_guard.iter())
    {
        Regex::new(pattern).map_err(|e| PackError::InvalidRegex(pattern.clone(), e))?;
    }
    Ok(CompiledActivation {
        on: def.on.clone(),
        off: def.off.clone(),
        question_guard: def.question_guard.clone(),
        command_prefix: def.command_prefix.clone(),
    })
}

/// Strip `<!-- ... -->` comments, flatten markdown heading markers (`## Foo`
/// → `Foo` — the `#` characters cost tokens for no behavioral value in an
/// injected system prompt fragment), and collapse runs of 3+ newlines to 2.
fn normalize(text: &str) -> String {
    let no_comments = strip_html_comments(text);
    let mut out = String::with_capacity(no_comments.len());
    for line in no_comments.lines() {
        out.push_str(&strip_heading_marker(line));
        out.push('\n');
    }
    collapse_blank_runs(&out).trim().to_string()
}

fn strip_html_comments(text: &str) -> String {
    // `(?s)` so `.` matches newlines too — comments can span lines.
    let re = Regex::new(r"(?s)<!--.*?-->").expect("static pattern");
    re.replace_all(text, "").into_owned()
}

fn strip_heading_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        rest.trim_start_matches('#').trim_start().to_string()
    } else {
        line.to_string()
    }
}

fn collapse_blank_runs(text: &str) -> String {
    let re = Regex::new(r"\n{3,}").expect("static pattern");
    re.replace_all(text, "\n\n").into_owned()
}
