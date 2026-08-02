//! Claude Code `settings.json` merge: JSONC read, hook-field validation,
//! and an ownership model for idempotent install/uninstall.
//!
//! Ported from `archive/bin/lib/settings.js`. Two deliberate departures:
//!
//! - **JSONC parsing uses a real tokenizer** ([`jsonc_parser`]), not a
//!   hand-rolled comment/trailing-comma stripper. The archive's second pass
//!   (`stripTrailingCommas`) was a string-aware regex that could still
//!   corrupt a value like `"echo ,}"` (issue #595) — a real parser doesn't
//!   have that failure mode by construction.
//! - **Ownership can't be a basename set.** The archive's
//!   `MANAGED_HOOK_BASENAMES` worked because each hook was a distinct
//!   script file (`caveman-activate.js`, `caveman-mode-tracker.js`, ...).
//!   Frank is one binary — every hook command has the same basename. So
//!   ownership here is an exact substring marker inside the *subcommand*
//!   (`"hook session-start"`, not `"frank"`), which is specific enough to
//!   avoid false positives on a user's own unrelated hooks while still
//!   surviving a rotating absolute binary path across reinstalls.
//!
//! `validateHookFields` is ported verbatim (structure and rationale both):
//! Claude Code's settings schema is all-or-nothing, so one malformed hook
//! entry — from *any* tool, not just Frank — silently discards the whole
//! file. Every write goes through this first.

use std::path::Path;

use serde_json::{Value, json};

/// Read `settings.json`, tolerating JSON5-ish comments and trailing commas.
/// Mirrors the archive's `readSettings` contract: missing file or blank
/// content is an empty object (nothing to merge with yet); a file that
/// exists but fails to parse is `None` — callers must refuse to touch it
/// rather than silently overwriting a config they couldn't understand.
pub fn read_settings(path: &Path) -> Option<Value> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(json!({})),
        Err(_) => return None,
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > frank_safeio::MAX_CONFIG_BYTES as u64
    {
        return None;
    }
    match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
        Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Some(json!({}))
        }
        Err(_) => None,
        Ok(raw) => {
            if raw.trim().is_empty() {
                return Some(json!({}));
            }
            match jsonc_parser::parse_to_serde_value(&raw, &Default::default()) {
                Ok(Some(v)) => Some(v),
                Ok(None) => Some(json!({})),
                Err(_) => None,
            }
        }
    }
}

/// Write `settings.json` atomically, pretty-printed, plain JSON.
///
/// Known limitation, same as the archive: this drops any comments that
/// were in the file. Callers that care (the installer) back the file up
/// once, on first write, before this ever runs — see `plan.rs`.
pub fn write_settings(path: &Path, value: &Value) -> frank_safeio::Result<()> {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    frank_safeio::write_text_atomic(path, &format!("{text}\n"), frank_safeio::MAX_CONFIG_BYTES)
}

/// Drop anything in `settings.hooks` that doesn't match Claude Code's
/// required shape, so one malformed entry can't take down the whole file
/// on the next Claude Code launch. Mutates in place; safe to call on
/// already-valid settings (no-op).
pub fn validate_hook_fields(settings: &mut Value) {
    let Some(obj) = settings.as_object_mut() else {
        return;
    };
    let Some(hooks) = obj.get_mut("hooks") else {
        return;
    };
    let Some(hooks_obj) = hooks.as_object_mut() else {
        obj.remove("hooks");
        return;
    };

    let events: Vec<String> = hooks_obj.keys().cloned().collect();
    for event in events {
        let Some(arr) = hooks_obj.get(&event).and_then(Value::as_array) else {
            hooks_obj.remove(&event);
            continue;
        };
        let filtered: Vec<Value> = arr
            .iter()
            .filter_map(|entry| {
                let entry_obj = entry.as_object()?;
                let raw_hooks = entry_obj.get("hooks")?.as_array()?;
                let kept: Vec<Value> = raw_hooks
                    .iter()
                    .filter(|h| {
                        let Some(h) = h.as_object() else { return false };
                        match h.get("type").and_then(Value::as_str) {
                            Some("command") => h
                                .get("command")
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty()),
                            Some("agent") => h
                                .get("prompt")
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty()),
                            _ => false,
                        }
                    })
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    return None;
                }
                let mut kept_entry = entry.clone();
                kept_entry["hooks"] = Value::Array(kept);
                Some(kept_entry)
            })
            .collect();

        if filtered.is_empty() {
            hooks_obj.remove(&event);
        } else {
            hooks_obj.insert(event, Value::Array(filtered));
        }
    }

    if hooks_obj.is_empty() {
        obj.remove("hooks");
    }
}

#[derive(Debug, Clone)]
pub struct HookSpec {
    pub event: String,
    pub command: String,
    pub timeout: Option<u64>,
    pub status_message: Option<String>,
    /// Exact substring that identifies this hook as Frank's on future
    /// install/uninstall/prune passes. Must be specific enough that a
    /// user's own unrelated hook wouldn't plausibly contain it.
    pub owned_marker: String,
}

fn hooks_array<'a>(settings: &'a mut Value, event: &str) -> &'a mut Vec<Value> {
    let obj = settings
        .as_object_mut()
        .expect("settings.json root must be an object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .expect("hooks field must be an object");
    hooks_obj
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("hooks[event] must be an array")
}

fn has_marker(settings: &Value, event: &str, marker: &str) -> bool {
    settings["hooks"][event]
        .as_array()
        .map(|arr| {
            arr.iter().any(|entry| {
                entry["hooks"]
                    .as_array()
                    .map(|hs| {
                        hs.iter()
                            .any(|h| h["command"].as_str().is_some_and(|c| c.contains(marker)))
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Idempotent: does nothing and returns `false` if a hook with this marker
/// already exists for this event.
pub fn add_command_hook(settings: &mut Value, spec: &HookSpec) -> bool {
    if !settings.is_object() {
        *settings = json!({});
    }
    if has_marker(settings, &spec.event, &spec.owned_marker) {
        return false;
    }
    let mut hook = json!({ "type": "command", "command": spec.command });
    if let Some(t) = spec.timeout {
        hook["timeout"] = json!(t);
    }
    if let Some(m) = &spec.status_message {
        hook["statusMessage"] = json!(m);
    }
    hooks_array(settings, &spec.event).push(json!({ "hooks": [hook] }));
    true
}

/// Remove every hook entry whose command contains one of `markers`.
/// Returns the number of individual hook objects removed. Anything that
/// merely *mentions* a marker's text as a substring of an unrelated
/// command is, by construction, also "ours" — this is the same tradeoff
/// the archive's basename matching made, just against a different key.
pub fn remove_owned_hooks(settings: &mut Value, markers: &[&str]) -> usize {
    let mut removed = 0usize;
    let Some(hooks_obj) = settings
        .as_object_mut()
        .and_then(|o| o.get_mut("hooks"))
        .and_then(Value::as_object_mut)
    else {
        return 0;
    };

    let events: Vec<String> = hooks_obj.keys().cloned().collect();
    for event in events {
        let Some(arr) = hooks_obj.get(&event).and_then(Value::as_array) else {
            continue;
        };
        let kept: Vec<Value> = arr
            .iter()
            .filter_map(|entry| {
                let Some(hs) = entry["hooks"].as_array() else {
                    // An uninstall must not delete a user's malformed or
                    // future-shaped hook entry merely because Frank cannot
                    // inspect its ownership marker.
                    return Some(entry.clone());
                };
                let survivors: Vec<Value> = hs
                    .iter()
                    .filter(|h| {
                        let is_owned = h["command"]
                            .as_str()
                            .is_some_and(|c| markers.iter().any(|m| c.contains(m)));
                        if is_owned {
                            removed += 1;
                        }
                        !is_owned
                    })
                    .cloned()
                    .collect();
                if survivors.is_empty() {
                    None
                } else {
                    let mut e = entry.clone();
                    e["hooks"] = Value::Array(survivors);
                    Some(e)
                }
            })
            .collect();

        if kept.is_empty() {
            hooks_obj.remove(&event);
        } else {
            hooks_obj.insert(event, Value::Array(kept));
        }
    }

    if hooks_obj.is_empty() {
        settings.as_object_mut().unwrap().remove("hooks");
    }
    removed
}
