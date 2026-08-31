#[cfg(test)]
mod tests {
    use crate::attribution::{Attribution, AttributionBasis, TokenBucket};
    use crate::injection_ledger;
    use crate::pricing::*;
    use crate::stats::*;
    use frank_pack::ReductionStat;

    #[test]
    fn price_for_model_matches_most_specific_prefix_first() {
        assert_eq!(
            price_for_model(Some("claude-opus-4-1-20250805")),
            Some(75.0)
        );
        assert_eq!(price_for_model(Some("claude-opus-4-5")), Some(25.0));
        assert_eq!(
            price_for_model(Some("claude-sonnet-4-20250514")),
            Some(15.0)
        );
        assert_eq!(price_for_model(Some("claude-haiku-4-5")), Some(5.0));
        assert_eq!(price_for_model(Some("gpt-4o")), None);
        assert_eq!(price_for_model(None), None);
    }

    #[test]
    fn format_usd_uses_more_precision_for_smaller_amounts() {
        assert_eq!(format_usd(12.3456), "$12.35");
        assert_eq!(format_usd(0.5), "$0.500");
        assert_eq!(format_usd(0.001234), "$0.0012");
    }

    #[test]
    fn savings_estimate_math_matches_the_archive_formula() {
        // archive: Math.round(tokens/(1-ratio)) - tokens
        let stat = ReductionStat {
            mean: 0.65,
            p25: Some(0.48),
            p75: Some(0.79),
            n: Some(10),
            model: Some("claude-sonnet-4-20250514".into()),
        };
        let est = savings_estimate(350, &stat, Some("claude-sonnet-4-20250514"));
        // 350/(1-0.65) = 1000 -> saved 650
        assert_eq!(est.mean_tokens, 650);
        assert!(est.model_matches);
    }

    #[test]
    fn savings_estimate_flags_model_mismatch() {
        let stat = ReductionStat {
            mean: 0.65,
            p25: None,
            p75: None,
            n: Some(10),
            model: Some("claude-sonnet-4-20250514".into()),
        };
        let est = savings_estimate(100, &stat, Some("claude-opus-4-5"));
        assert!(!est.model_matches);
    }

    #[test]
    fn savings_estimate_zero_tokens_is_zero_saved() {
        let stat = ReductionStat {
            mean: 0.65,
            p25: None,
            p75: None,
            n: None,
            model: None,
        };
        let est = savings_estimate(0, &stat, None);
        assert_eq!(est.mean_tokens, 0);
    }

    #[test]
    fn savings_estimate_rejects_invalid_reduction_ratios() {
        let stat = ReductionStat {
            mean: 1.0,
            p25: Some(-0.1),
            p75: Some(1.2),
            n: None,
            model: None,
        };
        let est = savings_estimate(100, &stat, None);
        assert_eq!(est.low_tokens, 0);
        assert_eq!(est.mean_tokens, 0);
        assert_eq!(est.high_tokens, 0);
    }

    #[test]
    fn savings_estimate_accepts_model_prefixes_in_either_direction() {
        let stat = ReductionStat {
            mean: 0.5,
            p25: None,
            p75: None,
            n: None,
            model: Some("claude-sonnet-4".into()),
        };
        assert!(savings_estimate(10, &stat, Some("claude-sonnet-4-20250514")).model_matches);
        assert!(
            savings_estimate(
                10,
                &ReductionStat {
                    model: Some("claude-sonnet-4-20250514".into()),
                    ..stat
                },
                Some("claude-sonnet-4")
            )
            .model_matches
        );
    }

    #[test]
    fn aggregate_history_keeps_only_the_latest_row_per_session() {
        let rows = vec![
            HistoryRow {
                ts: 100,
                session_id: "s1".into(),
                model: None,
                output_tokens: 50,
                input_tokens: 10,
                turns: 5,
            },
            HistoryRow {
                ts: 200,
                session_id: "s1".into(),
                model: None,
                output_tokens: 90,
                input_tokens: 20,
                turns: 8,
            },
            HistoryRow {
                ts: 150,
                session_id: "s2".into(),
                model: None,
                output_tokens: 30,
                input_tokens: 5,
                turns: 3,
            },
        ];
        let agg = aggregate_history(&rows);
        assert_eq!(agg.len(), 2);
        let s1 = agg.iter().find(|r| r.session_id == "s1").unwrap();
        assert_eq!(
            s1.output_tokens, 90,
            "must keep the row with the later ts, not the first or last in file order"
        );
    }

    #[test]
    fn aggregate_history_keeps_the_first_row_when_timestamps_tie() {
        let rows = vec![
            HistoryRow {
                ts: 100,
                session_id: "same".into(),
                model: Some("first".into()),
                output_tokens: 1,
                input_tokens: 2,
                turns: 1,
            },
            HistoryRow {
                ts: 100,
                session_id: "same".into(),
                model: Some("second".into()),
                output_tokens: 9,
                input_tokens: 8,
                turns: 2,
            },
        ];

        let agg = aggregate_history(&rows);
        assert_eq!(agg.len(), 1);
        assert_eq!(agg[0].model.as_deref(), Some("first"));
        assert_eq!(agg[0].output_tokens, 1);
    }

    #[test]
    fn history_round_trips_through_append_and_read() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".frank-history.jsonl");
        append_history(
            &path,
            &HistoryRow {
                ts: 1,
                session_id: "a".into(),
                model: Some("m".into()),
                output_tokens: 5,
                input_tokens: 2,
                turns: 4,
            },
        );
        append_history(
            &path,
            &HistoryRow {
                ts: 2,
                session_id: "b".into(),
                model: None,
                output_tokens: 9,
                input_tokens: 3,
                turns: 7,
            },
        );
        let rows = read_history(&path);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].session_id, "a");
        assert_eq!(rows[1].output_tokens, 9);
        assert_eq!(rows[0].turns, 4);
    }

    #[test]
    fn lifetime_verdict_requires_both_session_and_turn_thresholds() {
        let low_turn_rows = (0..MIN_SESSIONS_FOR_LIFETIME_VERDICT)
            .map(|i| HistoryRow {
                ts: i as i64,
                session_id: format!("low-{i}"),
                model: None,
                output_tokens: 1,
                input_tokens: 1,
                turns: 1,
            })
            .collect::<Vec<_>>();
        assert!(!lifetime_verdict_has_enough_data(&low_turn_rows));

        let enough_rows = (0..MIN_SESSIONS_FOR_LIFETIME_VERDICT)
            .map(|i| HistoryRow {
                ts: i as i64,
                session_id: format!("enough-{i}"),
                model: None,
                output_tokens: 1,
                input_tokens: 1,
                turns: MIN_TURNS_FOR_LIFETIME_VERDICT / MIN_SESSIONS_FOR_LIFETIME_VERDICT,
            })
            .collect::<Vec<_>>();
        assert!(lifetime_verdict_has_enough_data(&enough_rows));
    }

    #[test]
    fn legacy_history_without_turns_is_conservative() {
        let legacy: HistoryRow = serde_json::from_str(
            r#"{"ts":1,"session_id":"legacy","model":null,"output_tokens":1,"input_tokens":1}"#,
        )
        .unwrap();
        assert_eq!(legacy.turns, 0);
    }

    fn minimal_pack(dir: &std::path::Path) -> frank_pack::CompiledPack {
        std::fs::write(
            dir.join("pack.toml"),
            r#"
schema = 1
[pack]
id = "t"
version = "0.0.0"
default_level = "full"
[fragments]
core = { file = "core.md" }
[[level]]
id = "full"
compose = ["core"]
reinforce = "r"
[benchmark.reduction]
full = { mean = 0.65, n = 10 }
"#,
        )
        .unwrap();
        std::fs::write(dir.join("core.md"), "x").unwrap();
        frank_pack::compile(&frank_pack::PackSource::load(dir).unwrap()).unwrap()
    }

    #[test]
    fn render_text_labels_measured_vs_estimated_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());

        let session_path = tmp.path().join("s.jsonl");
        std::fs::write(
            &session_path,
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:00.000Z","message":{"model":"claude-sonnet-4-20250514","usage":{"output_tokens":350,"input_tokens":1000,"cache_creation_input_tokens":200}}}"#,
        )
        .unwrap();

        let report = build_session_report(
            &session_path,
            &tmp.path().join("mode-log.jsonl"),
            &tmp.path().join("ledger.jsonl"),
            &pack,
            Some("full"),
            None,
        );
        let text = render_text(&report, &pack);
        assert!(text.contains("measured"));
        assert!(text.contains("est. saved"));
        assert!(text.contains("650")); // 350/(1-0.65) - 350
        assert!(text.contains("Input tokens (measured):    1200"));
        assert!(text.contains("Reading time (est., ~200 wpm): ~1.3 min"));

        let json = render_json(&report, &pack);
        assert_eq!(json["by_mode"]["full"]["measured"]["output_tokens"], 350);
        assert_eq!(json["by_mode"]["full"]["estimate"]["mean_tokens"], 650);
    }

    /// Regression: when flag-mtime basis excludes every turn as
    /// unattributed (the flag was written after all measured turns), the
    /// top-line "Output tokens (measured)" total must still reflect real
    /// usage — not read "0" just because nothing was attributable to a
    /// mode. Caught via a manual end-to-end smoke test, not a unit test,
    /// which is exactly the coverage gap this regression closes.
    #[test]
    fn render_text_top_line_total_includes_unattributed_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());

        let session_path = tmp.path().join("s.jsonl");
        std::fs::write(
            &session_path,
            "{\"type\":\"assistant\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"model\":\"m\",\"usage\":{\"output_tokens\":480,\"input_tokens\":2050,\"cache_creation_input_tokens\":75}}}\n",
        )
        .unwrap();

        // Flag mtime far after the turn's timestamp -> flag-mtime basis,
        // everything before the write is unattributed.
        let report = build_session_report(
            &session_path,
            &tmp.path().join("mode-log.jsonl"),
            &tmp.path().join("ledger.jsonl"),
            &pack,
            Some("full"),
            Some(9_999_999_999_999),
        );
        assert_eq!(
            report
                .attribution
                .by_mode
                .get("full")
                .map(|b| b.output_tokens)
                .unwrap_or(0),
            0
        );
        assert_eq!(report.attribution.unknown.output_tokens, 480);

        let text = render_text(&report, &pack);
        assert!(text.contains("Output tokens (measured):   480"), "{text}");
        assert!(text.contains("Input tokens (measured):    2125"), "{text}");
        assert!(text.contains("unattributed: 480 tok"), "{text}");
        assert!(!text.contains("Output tokens (measured):   0"), "{text}");
    }

    #[test]
    fn render_text_labels_off_mode_sidechains_and_injection_cost() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());
        let session_path = tmp.path().join("s.jsonl");
        std::fs::write(
            &session_path,
            concat!(
                "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-sonnet-4\",\"usage\":{\"output_tokens\":100}}}\n",
                "{\"type\":\"assistant\",\"isSidechain\":true,\"message\":{\"model\":\"claude-sonnet-4\",\"usage\":{\"output_tokens\":7}}}\n"
            ),
        )
        .unwrap();
        let ledger_path = tmp.path().join("ledger.jsonl");
        injection_ledger::append(
            &ledger_path,
            &injection_ledger::InjectionEntry {
                ts: 1,
                kind: "activate".into(),
                session: Some("s".into()),
                level: Some("full".into()),
                inject_bytes: 400,
            },
        );
        injection_ledger::append(
            &ledger_path,
            &injection_ledger::InjectionEntry {
                ts: 2,
                kind: "reinforce".into(),
                session: Some("s".into()),
                level: Some("full".into()),
                inject_bytes: 100,
            },
        );

        let report = build_session_report(
            &session_path,
            &tmp.path().join("mode-log.jsonl"),
            &ledger_path,
            &pack,
            None,
            None,
        );
        let text = render_text(&report, &pack);
        assert!(text.contains("frank off"), "{text}");
        assert!(text.contains("subagent (sidechain): 7 tok"), "{text}");
        assert!(!text.contains("unattributed:"), "{text}");
        assert!(text.contains("~$0.0019"), "{text}");
    }

    #[test]
    fn render_text_shortens_long_session_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());
        let report = SessionReport {
            session_path: Some(std::path::PathBuf::from("x".repeat(60))),
            session_id: Some("s".into()),
            turns: 1,
            model: None,
            attribution: Attribution {
                by_mode: std::collections::BTreeMap::new(),
                unknown: TokenBucket::default(),
                sidechain: TokenBucket::default(),
                basis: AttributionBasis::WholeSession,
            },
            injection_activate_bytes: 0,
            injection_reinforce_bytes: 0,
        };
        let text = render_text(&report, &pack);
        let full_path_line = "Session:  ".to_string() + &"x".repeat(60);
        assert!(text.contains("Session:  ..."), "{text}");
        assert!(!text.contains(&full_path_line), "{text}");
        assert!(!text.contains("subagent (sidechain)"), "{text}");
    }

    #[test]
    fn render_text_does_not_shorten_a_path_at_exactly_45_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());
        let path = "x".repeat(45);
        let report = SessionReport {
            session_path: Some(std::path::PathBuf::from(&path)),
            session_id: Some("s".into()),
            turns: 1,
            model: None,
            attribution: Attribution {
                by_mode: std::collections::BTreeMap::new(),
                unknown: TokenBucket::default(),
                sidechain: TokenBucket::default(),
                basis: AttributionBasis::WholeSession,
            },
            injection_activate_bytes: 0,
            injection_reinforce_bytes: 0,
        };
        let text = render_text(&report, &pack);
        assert!(text.contains(&format!("Session:  {path}")), "{text}");
        assert!(!text.contains("Session:  ..."), "{text}");
    }

    #[test]
    fn render_text_handles_zero_turns_gracefully() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = minimal_pack(tmp.path());
        let report = build_session_report(
            &tmp.path().join("nonexistent.jsonl"),
            &tmp.path().join("mode-log.jsonl"),
            &tmp.path().join("ledger.jsonl"),
            &pack,
            Some("full"),
            None,
        );
        assert_eq!(report.turns, 0);
        let text = render_text(&report, &pack);
        assert!(text.contains("No conversation yet"));
    }

    #[test]
    fn render_text_reports_unmeasured_level_without_guessing() {
        let tmp = tempfile::tempdir().unwrap();
        // "ultra" has no [benchmark.reduction] entry in this fixture pack.
        std::fs::write(
            tmp.path().join("pack.toml"),
            r#"
schema = 1
[pack]
id = "t"
version = "0.0.0"
default_level = "ultra"
[fragments]
core = { file = "core.md" }
[[level]]
id = "ultra"
compose = ["core"]
reinforce = "r"
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("core.md"), "x").unwrap();
        let pack = frank_pack::compile(&frank_pack::PackSource::load(tmp.path()).unwrap()).unwrap();

        let session_path = tmp.path().join("s.jsonl");
        std::fs::write(
            &session_path,
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:00.000Z","message":{"model":"m","usage":{"output_tokens":100}}}"#,
        )
        .unwrap();
        let report = build_session_report(
            &session_path,
            &tmp.path().join("mode-log.jsonl"),
            &tmp.path().join("ledger.jsonl"),
            &pack,
            Some("ultra"),
            None,
        );
        let text = render_text(&report, &pack);
        assert!(text.contains("no benchmark estimate"));
        assert!(!text.contains("est. saved"));
    }
}
