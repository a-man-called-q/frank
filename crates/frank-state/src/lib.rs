//! Mode state machine and config precedence.
//!
//! Two halves, deliberately kept separate: [`intent::classify`] is pure
//! (prompt in, [`Intent`] out — no filesystem access, fully unit-testable)
//! and [`engine::apply`] is the impure executor that turns an `Intent` into
//! flag-file mutations, the one-shot restore dance, and the mode-transition
//! log. See each module's docs for what was ported verbatim from
//! `archive/src/hooks/caveman-mode-tracker.js` / `caveman-config.js` versus
//! deliberately simplified, and why.

mod config;
mod engine;
mod intent;

pub use config::{resolve_default_level, resolve_default_level_with_user_dir};
pub use engine::{AppliedState, FlagPaths, apply};
pub use intent::{Intent, classify};

use frank_pack::CompiledLevel;

/// The per-turn reinforcement line for an active level — what
/// `hook user-prompt-submit` emits as `additionalContext` on every turn
/// caveman (or any pack) is active, on top of the once-per-session
/// activation prompt. See `AGENTS.md` on why this is the *expensive* half
/// of the injection: it lands at the end of every prompt and is therefore
/// never cached, unlike the activation block.
pub fn reinforce_text(level: &CompiledLevel) -> &str {
    &level.reinforce
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_pack::{PackSource, compile};
    use proptest::prelude::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    /// A small pack fixture, independent of the shipped caveman pack, so
    /// these tests don't break every time someone edits `packs/caveman/`.
    /// Trigger patterns are copied from `packs/caveman/pack.toml` because
    /// the *ordering behavior* under test is what those exact patterns
    /// produce — see the module docs on why `classify` is pure and
    /// separately tested from `engine::apply`.
    pub(crate) fn fixture_pack(dir: &Path) -> frank_pack::CompiledPack {
        fs::write(
            dir.join("pack.toml"),
            r#"
schema = 1

[pack]
id = "fixture"
version = "0.0.0"
default_level = "full"

[fragments]
core = { file = "core.md" }

[[level]]
id = "full"
aliases = ["classic"]
compose = ["core"]
reinforce = "ACTIVE (full)."

[[level]]
id = "ultra"
compose = ["core"]
reinforce = "ACTIVE (ultra)."

[[oneshot]]
id = "commit"
prompt = "commit.md"
restores_previous = true

[[oneshot]]
id = "review"
prompt = "review.md"
restores_previous = true

[activation]
on = [
  '\b(activate|enable|start|turn on|use|switch to|want|give me)\b[^.]{0,40}\bcaveman\b',
  '\btalk like\b[^.]{0,40}\bcaveman\b',
  '\bcaveman\s+mode\s+(on|please|now)\b',
  '^caveman(\s+mode)?\s*[.!]*$',
  '\b(less tokens|fewer tokens|be brief|be terse|shorter answers)\b',
]
off = [
  '\b(stop|disable|deactivate|quit|exit|kill)\s+(the\s+)?caveman\b',
  '\bcaveman(\s+mode)?\s+(off|stop|disabled?)\b',
  '\bturn\s+off\s+(the\s+)?caveman\b',
  '^(please\s+)?(go\s+|back\s+to\s+|switch\s+(back\s+)?to\s+|return\s+to\s+)?normal\s+mode\b',
  '\bcaveman\b.*\bnormal\s+mode\b|\bnormal\s+mode\b.*\bcaveman\b',
]
question_guard = "^(what|whats|how|why|when|where|who|does|do|did|is|are|can|could|would|should|tell me|explain)\\b"
command_prefix = "caveman"
"#,
        )
        .unwrap();
        fs::write(dir.join("core.md"), "Respond terse.").unwrap();
        fs::write(dir.join("commit.md"), "Write a commit message.").unwrap();
        fs::write(dir.join("review.md"), "Review the diff.").unwrap();
        compile(&PackSource::load(dir).unwrap()).unwrap()
    }

    // ---------- classify: table-driven cases from the archive's fixed bugs ----------

    #[test]
    fn classify_cases() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let default = "full";

        let cases: &[(&str, Intent)] = &[
            // #598: deactivation must win, including odd word orders.
            ("turn caveman mode off", Intent::Deactivate),
            ("stop caveman", Intent::Deactivate),
            ("please disable the caveman", Intent::Deactivate),
            ("caveman mode off", Intent::Deactivate),
            ("normal mode", Intent::Deactivate),
            ("back to normal mode", Intent::Deactivate),
            ("switch back to normal mode please", Intent::Deactivate),
            // vim's "normal mode" must NOT deactivate — no caveman context,
            // not prompt-initial.
            ("how do I exit vim normal mode", Intent::None),
            ("how do i get back to normal mode in vim", Intent::None),
            // but "normal mode" + "caveman" anywhere still counts.
            (
                "caveman is fun, back to normal mode now",
                Intent::Deactivate,
            ),
            // questions must never activate.
            ("what is caveman mode?", Intent::None),
            ("how does caveman work", Intent::None),
            ("does caveman lite drop articles?", Intent::None),
            // NL activation.
            (
                "talk like a caveman please",
                Intent::Activate("full".into()),
            ),
            ("activate caveman mode", Intent::Activate("full".into())),
            ("caveman mode on", Intent::Activate("full".into())),
            ("caveman", Intent::Activate("full".into())),
            ("caveman!", Intent::Activate("full".into())),
            ("be brief", Intent::Activate("full".into())),
            ("give me fewer tokens", Intent::Activate("full".into())),
            // scoped brevity request: still activates, since the `regex`
            // crate has no lookaround to exclude "in the summary" — a
            // documented simplification (see intent.rs).
            ("be brief in the summary", Intent::Activate("full".into())),
            // ordinary prose shouldn't trip anything.
            ("fix the auth bug in login.rs", Intent::None),
            // bare slash command activates at default.
            ("/caveman", Intent::Activate("full".into())),
            ("/caveman:caveman", Intent::Activate("full".into())),
            // slash with explicit level / alias.
            ("/caveman ultra", Intent::Activate("ultra".into())),
            ("/caveman classic", Intent::Activate("full".into())),
            // slash off forms.
            ("/caveman off", Intent::Deactivate),
            ("/caveman stop", Intent::Deactivate),
            ("/caveman disable", Intent::Deactivate),
            // unknown slash arg: flag stays untouched, not an error.
            ("/caveman bogus", Intent::None),
            ("/caveman ultar", Intent::None),
            // oneshots, bare and namespaced.
            ("/caveman-commit", Intent::Oneshot("commit".into())),
            ("/caveman:caveman-commit", Intent::Oneshot("commit".into())),
            ("/caveman-review", Intent::Oneshot("review".into())),
            // stats interception.
            ("/caveman-stats", Intent::Stats(vec![])),
            (
                "/caveman-stats --share",
                Intent::Stats(vec!["--share".into()]),
            ),
            (
                "/caveman:caveman-stats --since 7d",
                Intent::Stats(vec!["--since".into(), "7d".into()]),
            ),
            // multiline prompts collapse to one line so triggers still match (#598).
            ("talk\nlike a\ncaveman", Intent::Activate("full".into())),
        ];

        for (prompt, expected) in cases {
            let got = classify(prompt, &pack, default);
            assert_eq!(&got, expected, "prompt: {prompt:?}");
        }
    }

    #[test]
    fn deactivate_wins_over_a_slash_command_in_the_same_prompt() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        // Contrived, but the archive's own control flow makes wants_off an
        // unconditional final override — see intent.rs's doc comment.
        let got = classify("/caveman ultra, wait, stop caveman", &pack, "full");
        assert_eq!(got, Intent::Deactivate);
    }

    #[test]
    fn resolved_default_of_off_suppresses_activation() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        assert_eq!(classify("caveman mode on", &pack, "off"), Intent::None);
        assert_eq!(classify("/caveman", &pack, "off"), Intent::None);
    }

    // ---------- engine::apply: the stateful half ----------

    fn paths(dir: &Path) -> FlagPaths {
        FlagPaths::under(dir)
    }

    #[test]
    fn apply_activate_writes_flag() {
        let tmp = tempdir().unwrap();
        let pack_dir = tmp.path().join("pack");
        fs::create_dir_all(&pack_dir).unwrap();
        let pack = fixture_pack(&pack_dir);
        let p = paths(tmp.path());

        let state = apply(&Intent::Activate("ultra".into()), &pack, &p);
        assert_eq!(state, AppliedState::Level("ultra".into()));
        assert_eq!(fs::read_to_string(&p.active).unwrap(), "ultra");
    }

    #[test]
    fn apply_deactivate_removes_flag() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());
        apply(&Intent::Activate("full".into()), &pack, &p);
        assert!(p.active.exists());

        let state = apply(&Intent::Deactivate, &pack, &p);
        assert_eq!(state, AppliedState::Off);
        assert!(!p.active.exists());
    }

    #[test]
    fn apply_oneshot_saves_prev_and_restores_next_turn() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());

        apply(&Intent::Activate("ultra".into()), &pack, &p);
        let state = apply(&Intent::Oneshot("commit".into()), &pack, &p);
        assert_eq!(state, AppliedState::Oneshot("commit".into()));
        assert_eq!(fs::read_to_string(&p.prev).unwrap(), "ultra");

        // Next ordinary prompt (Intent::None) triggers the restore.
        let state = apply(&Intent::None, &pack, &p);
        assert_eq!(state, AppliedState::Level("ultra".into()));
        assert!(!p.prev.exists());
    }

    #[test]
    fn chained_oneshots_do_not_clobber_the_saved_prev() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());

        apply(&Intent::Activate("ultra".into()), &pack, &p);
        apply(&Intent::Oneshot("commit".into()), &pack, &p);
        // A second independent mode right after must not overwrite prev
        // with "commit" — it must still hold "ultra" (#599).
        apply(&Intent::Oneshot("review".into()), &pack, &p);
        assert_eq!(fs::read_to_string(&p.prev).unwrap(), "ultra");

        let state = apply(&Intent::None, &pack, &p);
        assert_eq!(state, AppliedState::Level("ultra".into()));
    }

    #[test]
    fn oneshot_with_no_prior_prose_mode_deactivates_after_restore() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());

        // No prior activation — caveman was off.
        apply(&Intent::Oneshot("commit".into()), &pack, &p);
        let state = apply(&Intent::None, &pack, &p);
        assert_eq!(state, AppliedState::Off);
    }

    #[test]
    fn stats_intent_never_touches_the_flag() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());

        apply(&Intent::Activate("ultra".into()), &pack, &p);
        let state = apply(&Intent::Stats(vec!["--share".into()]), &pack, &p);
        assert_eq!(state, AppliedState::StatsRequested(vec!["--share".into()]));
        assert_eq!(fs::read_to_string(&p.active).unwrap(), "ultra");
    }

    #[test]
    fn mode_log_records_transitions() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let p = paths(tmp.path());

        apply(&Intent::Activate("full".into()), &pack, &p);
        apply(&Intent::Activate("ultra".into()), &pack, &p);
        // Re-activating the same level should not append a redundant entry.
        apply(&Intent::Activate("ultra".into()), &pack, &p);

        let lines = frank_safeio::read_lines(&p.mode_log);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("\"mode\":\"full\""));
        assert!(lines[1].contains("\"mode\":\"ultra\""));
        assert!(lines[1].contains("\"prev\":\"full\""));
    }

    // ---------- config precedence ----------

    #[test]
    fn env_var_takes_precedence_over_everything() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        // SAFETY: single-threaded test, unique env var name not read elsewhere.
        unsafe { std::env::set_var("FRANK_TEST_DEFAULT_LEVEL_1", "ultra") };
        let resolved = resolve_default_level(&pack, tmp.path(), "FRANK_TEST_DEFAULT_LEVEL_1");
        unsafe { std::env::remove_var("FRANK_TEST_DEFAULT_LEVEL_1") };
        assert_eq!(resolved, "ultra");
    }

    #[test]
    fn falls_back_to_pack_default_when_nothing_configured() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let resolved =
            resolve_default_level(&pack, tmp.path(), "FRANK_TEST_DEFAULT_LEVEL_UNSET_XYZ");
        assert_eq!(resolved, pack.default_level);
    }

    #[test]
    fn repo_local_config_walks_up_from_a_nested_directory() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let nested = tmp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            tmp.path().join(".frank.toml"),
            "default_level = \"ultra\"\n",
        )
        .unwrap();

        let resolved = resolve_default_level(&pack, &nested, "FRANK_TEST_DEFAULT_LEVEL_UNSET_ABC");
        assert_eq!(resolved, "ultra");
    }

    #[test]
    fn symlinked_repo_config_is_refused() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());
        let secret = tmp.path().join("secret.toml");
        fs::write(&secret, "default_level = \"ultra\"\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, tmp.path().join(".frank.toml")).unwrap();

        let resolved =
            resolve_default_level(&pack, tmp.path(), "FRANK_TEST_DEFAULT_LEVEL_UNSET_DEF");
        assert_eq!(resolved, pack.default_level);
    }

    #[test]
    fn reinforce_text_returns_the_compiled_level_reinforcement() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack(tmp.path());

        let level = pack.resolve_level("ultra").unwrap();
        assert_eq!(reinforce_text(level), "ACTIVE (ultra).");
    }

    proptest! {
        #[test]
        fn arbitrary_prompt_text_is_total(prompt in any::<String>()) {
            let tmp = tempdir().unwrap();
            let pack = fixture_pack(tmp.path());
            let intent = classify(&prompt, &pack, &pack.default_level);
            prop_assert!(matches!(
                intent,
                Intent::None | Intent::Activate(_) | Intent::Deactivate | Intent::Oneshot(_) | Intent::Stats(_)
            ));
        }

        #[test]
        fn deactivation_precedence_survives_arbitrary_suffix(suffix in any::<String>()) {
            let tmp = tempdir().unwrap();
            let pack = fixture_pack(tmp.path());
            let prompt = format!("please stop caveman {suffix}");
            prop_assert_eq!(classify(&prompt, &pack, &pack.default_level), Intent::Deactivate);
        }
    }
}
