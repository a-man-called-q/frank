//! Pure prompt → [`Intent`] classification. No filesystem access — every
//! case here is a table-driven unit test (see `lib.rs`'s test module),
//! which is the coverage the archive's equivalent (deactivation regexes,
//! question guard, slash-command parsing, all interleaved with side
//! effects in one function) never had in isolation.
//!
//! Ported from `archive/src/hooks/caveman-mode-tracker.js`, preserving its
//! hard-won ordering discipline: deactivation is computed before anything
//! else so "turn caveman mode off" can never fall through to activation
//! (#598), and a question ("what is caveman mode?") never activates
//! (#598). One deliberate simplification from the original, documented
//! where it applies below.

use frank_pack::CompiledPack;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Explicit deactivation — an off-trigger phrase, or `/<prefix> off`.
    Deactivate,
    /// Activate at a specific, already-resolved level id (bare
    /// `/<prefix>`, a natural-language trigger, or `/<prefix> <level>`).
    Activate(String),
    /// A one-shot command bound to a pack oneshot id (e.g. `commit`).
    Oneshot(String),
    /// `/<prefix>-stats [tail args]` — an engine-level convention (see
    /// module docs), not part of any pack's data.
    Stats(Vec<String>),
    /// Nothing recognized this turn; flag stays whatever it already was.
    None,
}

fn normalize(prompt: &str) -> String {
    let trimmed = prompt.trim().to_lowercase();
    let mut out = String::with_capacity(trimmed.len());
    let mut last_was_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

fn any_match(patterns: &[String], text: &str) -> bool {
    patterns.iter().any(|p| {
        Regex::new(p)
            .map(|re| re.is_match(text))
            .unwrap_or(false) // an invalid pattern never matches; frank-pack::compile already rejects these at pack-build time
    })
}

/// `/<prefix>-stats` and its marketplace-namespaced twin
/// `/<prefix>:<prefix>-stats`, with everything after as tail args.
fn match_stats(prompt: &str, prefix: &str) -> Option<Vec<String>> {
    let pattern = format!(
        r"^/{p}(?::{p})?-stats(?:\s+(.*))?$",
        p = regex::escape(prefix)
    );
    let re = Regex::new(&pattern).ok()?;
    let caps = re.captures(prompt)?;
    let tail = caps
        .get(1)
        .map(|m| m.as_str().split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    Some(tail)
}

/// Slash-command outcome, if `prompt`'s first token is a recognized
/// `/<prefix>...` form. `None` here means "not a slash command at all" —
/// distinct from `Some(Intent::None)`, "a slash command with an
/// unrecognized argument" (flag stays untouched, per the archive's "no
/// silent overwrite" rule).
fn parse_slash_command(prompt: &str, prefix: &str, pack: &CompiledPack, resolved_default: &str) -> Option<Intent> {
    let mut tokens = prompt.split_whitespace();
    let cmd = tokens.next()?;
    let arg = tokens.next().unwrap_or("");

    let bare = format!("/{prefix}");
    let namespaced = format!("/{prefix}:{prefix}");

    for id in pack.oneshots.keys() {
        if cmd == format!("/{prefix}-{id}") || cmd == format!("{namespaced}-{id}") {
            return Some(Intent::Oneshot(id.clone()));
        }
    }

    if cmd != bare && cmd != namespaced {
        return None;
    }

    if arg.is_empty() {
        return Some(activate_or_none(resolved_default));
    }
    if arg == "off" || arg == "stop" || arg == "disable" {
        return Some(Intent::Deactivate);
    }
    match pack.resolve_level(arg) {
        Some(level) => Some(Intent::Activate(level.id.clone())),
        // Unknown arg: the archive leaves the flag untouched rather than
        // erroring or guessing — a typo in `/caveman ultar` must not
        // silently deactivate or silently pick some other level.
        None => Some(Intent::None),
    }
}

fn activate_or_none(resolved_default: &str) -> Intent {
    if resolved_default == "off" {
        Intent::None
    } else {
        Intent::Activate(resolved_default.to_string())
    }
}

/// Classify one user prompt into an [`Intent`], given the active pack's
/// trigger patterns and the already-resolved default level (see
/// [`crate::resolve_default_level`]).
///
/// Documented divergence from the archive: there, natural-language
/// activation and slash-command parsing are two independent, ungated code
/// blocks, so a prompt that happens to contain *both* an NL trigger phrase
/// and a slash command with a bad argument could see the NL-triggered
/// write survive (because the unknown-arg branch simply performs no write
/// of its own, rather than reverting one performed earlier in the same
/// prompt). That combination isn't exercised by any real prompt or by the
/// archive's own test suite. Here, a prompt recognized as a slash command
/// is handled *exclusively* by slash-command parsing; natural-language
/// triggers are only evaluated for prompts that aren't slash commands at
/// all. `wants_off` still overrides everything, matching the archive,
/// where the deactivation check runs unconditionally last.
pub fn classify(prompt: &str, pack: &CompiledPack, resolved_default: &str) -> Intent {
    let prompt = normalize(prompt);
    let prefix = pack.activation.command_prefix.as_deref().unwrap_or("frank");

    if let Some(tail) = match_stats(&prompt, prefix) {
        return Intent::Stats(tail);
    }

    let wants_off = any_match(&pack.activation.off, &prompt);
    let is_question = pack
        .activation
        .question_guard
        .as_deref()
        .map(|p| Regex::new(p).map(|re| re.is_match(&prompt)).unwrap_or(false))
        .unwrap_or(false);

    let slash_intent = parse_slash_command(&prompt, prefix, pack, resolved_default);

    if wants_off {
        return Intent::Deactivate;
    }

    if let Some(intent) = slash_intent {
        return intent;
    }

    if !is_question && any_match(&pack.activation.on, &prompt) {
        return activate_or_none(resolved_default);
    }

    Intent::None
}
