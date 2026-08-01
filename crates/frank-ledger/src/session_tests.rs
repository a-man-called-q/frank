#[cfg(test)]
mod tests {
    use crate::session::*;
    use proptest::prelude::*;
    use std::io::Write;
    use tempfile::tempdir;

    /// Cross-checked against Python's `datetime` for several known dates —
    /// this hand-rolled parser exists to avoid a full date/time dependency
    /// for one always-well-formed field, so it needs its own ground truth.
    #[test]
    fn iso8601_matches_known_epoch_values() {
        let cases: &[(&str, i64)] = &[
            ("1970-01-01T00:00:00.000Z", 0),
            ("2026-08-01T03:13:39.489Z", 1785554019489),
            ("2000-03-01T00:00:00.000Z", 951868800000), // leap-year boundary
            ("1999-12-31T23:59:59.999Z", 946684799999),
            ("2024-02-29T12:00:00.000Z", 1709208000000), // leap day
        ];
        for (input, expected) in cases {
            let raw = format!(
                r#"{{"type":"assistant","timestamp":"{input}","message":{{"model":"m","usage":{{"output_tokens":1}}}}}}"#
            );
            let tmp = tempdir().unwrap();
            let path = tmp.path().join("s.jsonl");
            std::fs::write(&path, raw).unwrap();
            let scan = parse_session(&path);
            assert_eq!(scan.turns[0].ts, Some(*expected), "input: {input}");
        }
    }

    #[test]
    fn invalid_iso8601_timestamps_are_left_unattributed() {
        for timestamp in [
            "2026-02-30T00:00:00.000Z",
            "2026-13-01T00:00:00.000Z",
            "2026-01-01T24:00:00.000Z",
            "2026-01-01T00:60:00.000Z",
            "2026-01-01T00:00:00.0000Z",
            "not-a-timestamp",
        ] {
            let raw = format!(
                r#"{{"type":"assistant","timestamp":"{timestamp}","message":{{"usage":{{"output_tokens":1}}}}}}"#
            );
            let tmp = tempdir().unwrap();
            let path = tmp.path().join("invalid.jsonl");
            std::fs::write(&path, raw).unwrap();
            let scan = parse_session(&path);
            assert_eq!(scan.turns[0].ts, None, "input: {timestamp}");
        }
    }

    fn write_lines(dir: &std::path::Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        path
    }

    #[test]
    fn parses_assistant_turns_and_ignores_other_entry_types() {
        let tmp = tempdir().unwrap();
        let path = write_lines(
            tmp.path(),
            "s.jsonl",
            &[
                r#"{"type":"user","message":{"content":"hi"}}"#,
                r#"{"type":"assistant","timestamp":"2026-01-01T00:00:00.000Z","message":{"model":"claude-sonnet-4-20250514","usage":{"output_tokens":100,"input_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":20}}}"#,
                r#"{"type":"attachment","data":"x"}"#,
                "not even json",
                "",
                r#"{"type":"assistant","message":{}}"#,
                r#"{"type":"assistant","message":{"model":"claude-sonnet-4-20250514"}}"#,
            ],
        );
        let scan = parse_session(&path);
        assert_eq!(scan.turns.len(), 1);
        assert_eq!(scan.turns[0].output_tokens, 100);
        assert_eq!(scan.turns[0].input_tokens, 50);
        assert_eq!(scan.turns[0].cache_creation_input_tokens, 10);
        assert_eq!(scan.turns[0].cache_read_input_tokens, 20);
        assert_eq!(scan.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn missing_file_returns_empty_scan_not_an_error() {
        let scan = parse_session(std::path::Path::new("/nonexistent/path.jsonl"));
        assert_eq!(scan.turns.len(), 0);
        assert_eq!(scan.model, None);
    }

    #[test]
    fn oversized_or_symlinked_session_is_rejected_without_following_it() {
        let tmp = tempdir().unwrap();
        let oversized = tmp.path().join("oversized.jsonl");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len((frank_safeio::MAX_SESSION_BYTES + 1) as u64)
            .unwrap();
        assert!(parse_session(&oversized).turns.is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let secret = tmp.path().join("secret.jsonl");
            std::fs::write(
                &secret,
                r#"{"type":"assistant","message":{"usage":{"output_tokens":99}}}"#,
            )
            .unwrap();
            let link = tmp.path().join("link.jsonl");
            symlink(&secret, &link).unwrap();
            assert!(parse_session(&link).turns.is_empty());
        }
    }

    #[test]
    fn sidechain_turns_are_flagged() {
        let tmp = tempdir().unwrap();
        let path = write_lines(
            tmp.path(),
            "s.jsonl",
            &[
                r#"{"type":"assistant","isSidechain":true,"message":{"usage":{"output_tokens":5}}}"#,
                r#"{"type":"assistant","isSidechain":false,"message":{"usage":{"output_tokens":10}}}"#,
            ],
        );
        let scan = parse_session(&path);
        assert!(scan.turns[0].is_sidechain);
        assert!(!scan.turns[1].is_sidechain);
        assert_eq!(scan.turn_count(), 1); // sidechain excluded from the turn count
    }

    proptest! {
        #[test]
        fn arbitrary_partial_jsonl_is_a_total_scan(input in prop::collection::vec(any::<u8>(), 0..4096)) {
            let tmp = tempdir().unwrap();
            let path = tmp.path().join("partial.jsonl");
            std::fs::write(&path, &input).unwrap();
            let scan = parse_session(&path);
            prop_assert!(scan.turns.len() <= input.split(|byte| *byte == b'\n').count());
        }
    }

    #[test]
    fn find_recent_session_picks_the_newest_jsonl_across_nested_project_dirs() {
        let tmp = tempdir().unwrap();
        let projects = tmp.path().join("projects");
        std::fs::create_dir_all(projects.join("proj-a")).unwrap();
        std::fs::create_dir_all(projects.join("proj-b/nested")).unwrap();

        let old = projects.join("proj-a/old.jsonl");
        std::fs::write(&old, "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let newer = projects.join("proj-b/nested/newer.jsonl");
        std::fs::write(&newer, "{}").unwrap();

        let found = find_recent_session(tmp.path()).unwrap();
        assert_eq!(found, newer);
    }

    #[test]
    fn find_recent_session_returns_none_when_projects_dir_missing() {
        let tmp = tempdir().unwrap();
        assert_eq!(find_recent_session(tmp.path()), None);
    }

    #[cfg(unix)]
    #[test]
    fn find_recent_session_skips_symlinked_project_entries() {
        use std::os::unix::fs::symlink;
        let tmp = tempdir().unwrap();
        let projects = tmp.path().join("projects");
        std::fs::create_dir_all(&projects).unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.jsonl");
        std::fs::write(&secret, "{}").unwrap();
        symlink(&outside, projects.join("linked")).unwrap();
        assert_eq!(find_recent_session(tmp.path()), None);
    }

    /// The real Claude Code session log for *this* project — a genuine,
    /// non-synthetic fixture. Skips cleanly if the file isn't present
    /// (e.g. CI, or a different machine) rather than failing.
    #[test]
    fn parses_a_real_claude_code_session_file_if_present() {
        let Some(home) = frank_safeio::home_dir() else {
            return;
        };
        let dir = home.join(".claude/projects/-Volumes-external-Works-personal-caveman");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let Some(path) = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"))
        else {
            return;
        };

        let scan = parse_session(&path);
        assert!(
            !scan.turns.is_empty(),
            "expected at least one assistant turn in a real session file"
        );
        assert!(scan.model.is_some());
        // Every real turn has both output and (usually) cache-read tokens;
        // this is the assertion that would have caught the archive's gap —
        // input_tokens/cache_creation_input_tokens must not all be zero
        // across a real multi-turn session.
        let any_input = scan
            .turns
            .iter()
            .any(|t| t.input_tokens > 0 || t.cache_creation_input_tokens > 0);
        assert!(
            any_input,
            "expected at least one turn with nonzero input/cache-creation tokens"
        );
    }
}
