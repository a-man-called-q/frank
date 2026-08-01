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
        assert_eq!(v["hooks"]["SessionStart"][0]["hooks"][0]["prompt"], "do the thing");
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
        let s = spec("SessionStart", "frank hook session-start", "hook session-start");
        assert!(add_command_hook(&mut v, &s));
        assert!(!add_command_hook(&mut v, &s));
        assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
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
    fn prune_orphaned_removes_hooks_whose_target_is_unreachable() {
        let mut v = json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "/old/path/frank hook session-start" } ] }
                ]
            }
        });
        let pruned = prune_orphaned(&mut v, &["hook session-start"], |_cmd| false);
        assert_eq!(pruned, 1);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn prune_orphaned_keeps_reachable_hooks() {
        let mut v = json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "/real/frank hook session-start" } ] }
                ]
            }
        });
        let pruned = prune_orphaned(&mut v, &["hook session-start"], |_cmd| true);
        assert_eq!(pruned, 0);
        assert!(v["hooks"]["SessionStart"].as_array().unwrap().len() == 1);
    }
}
