#[cfg(test)]
mod tests {
    use crate::settings::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn write(path: &std::path::Path, content: &str) {
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn read_settings_missing_file_is_empty_object() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        assert_eq!(read_settings(&path), Some(json!({})));
    }

    #[test]
    fn read_settings_blank_file_is_empty_object() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write(&path, "   \n  ");
        assert_eq!(read_settings(&path), Some(json!({})));
    }

    #[test]
    fn read_settings_strips_line_comments() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write(&path, "{\"a\":1}// trailing comment\n");
        assert_eq!(read_settings(&path), Some(json!({"a": 1})));
    }

    /// The archive's regex-based trailing-comma stripper corrupted string
    /// values containing `,}`/`,]` (issue #595). A real tokenizer doesn't
    /// have that failure mode — this is the regression test for it.
    #[test]
    fn read_settings_preserves_commas_and_braces_inside_string_values() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write(
            &path,
            "{\"cmd\": \"echo ,}\", // comment\n\"glob\": \"cp file{,]x\", }",
        );
        let v = read_settings(&path).unwrap();
        assert_eq!(v["cmd"], "echo ,}");
        assert_eq!(v["glob"], "cp file{,]x");
    }

    #[test]
    fn read_settings_still_handles_real_trailing_commas() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write(&path, "{\"a\": [1, 2, 3,], \"b\": {\"c\": 1,},}");
        let v = read_settings(&path).unwrap();
        assert_eq!(v["a"], json!([1, 2, 3]));
        assert_eq!(v["b"]["c"], 1);
    }

    #[test]
    fn read_settings_refuses_to_touch_genuinely_malformed_json() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write(&path, "{not json at all!!!");
        assert_eq!(read_settings(&path), None);
    }

    #[test]
    fn read_settings_refuses_symlinks_directories_and_oversized_documents() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(read_settings(&path), None);
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "x".repeat(frank_safeio::MAX_CONFIG_BYTES + 1)).unwrap();
        assert_eq!(read_settings(&path), None);
        #[cfg(unix)]
        {
            std::fs::remove_file(&path).unwrap();
            let target = tmp.path().join("target.json");
            write(&target, "{}\n");
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert_eq!(read_settings(&path), None);
        }
    }

    #[test]
    fn read_settings_returns_none_for_an_unreadable_parent_error() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent-file");
        write(&parent, "not a directory");
        assert_eq!(read_settings(&parent.join("settings.json")), None);
    }

    #[test]
    fn read_settings_accepts_a_valid_document_at_the_exact_size_cap() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let mut raw = "{}".to_string();
        raw.push_str(&" ".repeat(frank_safeio::MAX_CONFIG_BYTES - raw.len()));
        write(&path, &raw);
        assert_eq!(read_settings(&path), Some(json!({})));
    }

    #[test]
    fn write_settings_round_trips() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        write_settings(&path, &json!({"hooks": {}})).unwrap();
        let back = read_settings(&path).unwrap();
        assert_eq!(back, json!({"hooks": {}}));
        assert!(std::fs::read_to_string(&path).unwrap().ends_with('\n'));
    }

    #[test]
    fn validate_hook_fields_drops_command_hook_without_command() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command" } ] } ] }
        });
        validate_hook_fields(&mut v);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn validate_hook_fields_drops_empty_command_string() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command", "command": "" } ] } ] }
        });
        validate_hook_fields(&mut v);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn validate_hook_fields_keeps_valid_agent_hook() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "agent", "prompt": "do the thing" } ] } ] }
        });
        validate_hook_fields(&mut v);
        assert_eq!(
            v["hooks"]["SessionStart"][0]["hooks"][0]["prompt"],
            "do the thing"
        );
    }

    #[test]
    fn validate_hook_fields_drops_unknown_type() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "mystery", "command": "x" } ] } ] }
        });
        validate_hook_fields(&mut v);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn validate_hook_fields_preserves_a_mixed_valid_and_invalid_event() {
        let mut v = json!({
            "hooks": {
                "SessionStart": [ { "hooks": [ { "type": "command", "command": "real" } ] } ],
                "Bogus": "not an array",
            }
        });
        validate_hook_fields(&mut v);
        assert_eq!(v["hooks"]["SessionStart"][0]["hooks"][0]["command"], "real");
        assert!(v["hooks"].get("Bogus").is_none());
    }

    #[test]
    fn validate_hook_fields_is_a_noop_on_already_valid_settings() {
        let mut v = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command", "command": "x", "timeout": 5 } ] } ] },
            "statusLine": { "type": "command", "command": "true" }
        });
        let before = v.clone();
        validate_hook_fields(&mut v);
        assert_eq!(v, before);
    }

    #[test]
    fn validate_hook_fields_handles_non_objects_and_empty_agent_prompts() {
        let mut scalar = json!("not a settings object");
        validate_hook_fields(&mut scalar);
        assert_eq!(scalar, json!("not a settings object"));

        let mut malformed = json!({
            "hooks": ["not an object"]
        });
        validate_hook_fields(&mut malformed);
        assert_eq!(malformed, json!({}));

        let mut empty_agent = json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "agent", "prompt": "" } ] } ] }
        });
        validate_hook_fields(&mut empty_agent);
        assert_eq!(empty_agent, json!({}));
    }

    fn spec(event: &str, command: &str, marker: &str) -> HookSpec {
        HookSpec {
            event: event.to_string(),
            command: command.to_string(),
            timeout: Some(5),
            status_message: None,
            owned_marker: marker.to_string(),
        }
    }

    #[test]
    fn add_command_hook_is_idempotent() {
        let mut v = json!({});
        let s = spec(
            "SessionStart",
            "frank hook session-start",
            "hook session-start",
        );
        assert!(add_command_hook(&mut v, &s));
        assert!(!add_command_hook(&mut v, &s));
        assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn add_command_hook_repairs_scalar_root_and_preserves_optional_fields() {
        let mut v = json!("bad root");
        let mut s = spec(
            "SessionStart",
            "frank hook session-start",
            "hook session-start",
        );
        s.status_message = Some("Loading".into());
        assert!(add_command_hook(&mut v, &s));
        assert_eq!(v["hooks"]["SessionStart"][0]["hooks"][0]["timeout"], 5);
        assert_eq!(
            v["hooks"]["SessionStart"][0]["hooks"][0]["statusMessage"],
            "Loading"
        );

        let mut no_optional = json!({"hooks": {"SessionStart": []}});
        let mut s = spec(
            "SessionStart",
            "frank hook session-start",
            "hook session-start",
        );
        s.timeout = None;
        s.status_message = None;
        assert!(add_command_hook(&mut no_optional, &s));
        assert!(
            no_optional["hooks"]["SessionStart"][0]["hooks"][0]
                .get("timeout")
                .is_none()
        );
    }

    #[test]
    fn remove_owned_hooks_leaves_user_hooks_that_merely_mention_the_marker() {
        // A user's own hook command that happens to contain the word
        // "frank" (their own tool, a colleague's name, whatever) must
        // survive — only an exact marker-substring match on *our* specific
        // subcommand text counts as owned.
        let mut v = json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "frank hook session-start" } ] },
                    { "hooks": [ { "type": "command", "command": "echo 'frank says hi'" } ] },
                ]
            }
        });
        let removed = remove_owned_hooks(&mut v, &["hook session-start"]);
        assert_eq!(removed, 1);
        let remaining = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0]["hooks"][0]["command"], "echo 'frank says hi'");
    }

    #[test]
    fn remove_owned_hooks_cleans_up_empty_event_and_hooks_root() {
        let mut v = json!({
            "hooks": {
                "SessionStart": [ { "hooks": [ { "type": "command", "command": "frank hook session-start" } ] } ]
            }
        });
        let removed = remove_owned_hooks(&mut v, &["hook session-start"]);
        assert_eq!(removed, 1);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn remove_owned_hooks_ignores_malformed_entries_and_empty_markers() {
        let mut v = json!({
            "hooks": {
                "SessionStart": [
                    "malformed",
                    { "hooks": "not an array" },
                    { "hooks": [ { "type": "command", "command": "echo user" } ] }
                ],
                "Other": "not an array"
            }
        });
        assert_eq!(remove_owned_hooks(&mut v, &["never-match"]), 0);
        assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 3);
    }
}
