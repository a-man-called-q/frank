#[cfg(test)]
mod tests {
    use crate::attribution::*;
    use crate::mode_log::ModeLogRow;
    use crate::session::SessionTurn;
    use proptest::prelude::*;

    fn turn(ts: Option<i64>, output: u64) -> SessionTurn {
        SessionTurn {
            ts,
            output_tokens: output,
            input_tokens: output * 2,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            is_sidechain: false,
        }
    }

    #[test]
    fn whole_session_when_no_log_and_no_flag_mtime_evidence() {
        let turns = vec![turn(Some(100), 50), turn(Some(200), 30)];
        let attr = attribute_by_mode(&turns, &[], Some("full"), None);
        assert_eq!(attr.basis, AttributionBasis::WholeSession);
        assert_eq!(attr.by_mode["full"].output_tokens, 80);
    }

    #[test]
    fn whole_session_when_off() {
        let turns = vec![turn(Some(100), 50)];
        let attr = attribute_by_mode(&turns, &[], None, None);
        assert_eq!(attr.by_mode["none"].output_tokens, 50);
    }

    #[test]
    fn flag_mtime_basis_excludes_tokens_before_the_write() {
        let turns = vec![turn(Some(100), 50), turn(Some(300), 30)];
        // Flag written at ts=200, after the first turn but before the second.
        let attr = attribute_by_mode(&turns, &[], Some("full"), Some(200));
        assert_eq!(attr.basis, AttributionBasis::FlagMtime);
        assert_eq!(attr.by_mode.get("full").map(|b| b.output_tokens), Some(30));
        assert_eq!(
            attr.unknown.output_tokens, 50,
            "pre-write span must be excluded, not guessed"
        );
    }

    #[test]
    fn flag_mtime_before_first_turn_falls_back_to_whole_session() {
        let turns = vec![turn(Some(300), 50)];
        let attr = attribute_by_mode(&turns, &[], Some("full"), Some(100));
        assert_eq!(attr.basis, AttributionBasis::WholeSession);
        assert_eq!(attr.by_mode["full"].output_tokens, 50);
    }

    #[test]
    fn log_basis_attributes_each_span_to_the_mode_active_at_that_time() {
        let turns = vec![
            turn(Some(50), 10),  // before first log row -> prefix mode
            turn(Some(150), 20), // full
            turn(Some(250), 30), // ultra
        ];
        let log = vec![
            ModeLogRow {
                ts: 100,
                mode: Some("full".into()),
                prev: None,
            },
            ModeLogRow {
                ts: 200,
                mode: Some("ultra".into()),
                prev: Some("full".into()),
            },
        ];
        let attr = attribute_by_mode(&turns, &log, Some("ultra"), None);
        assert_eq!(attr.basis, AttributionBasis::Log);
        assert_eq!(
            attr.by_mode.get("none").map(|b| b.output_tokens),
            Some(10),
            "prefix mode is the first row's prev (null = off)"
        );
        assert_eq!(attr.by_mode["full"].output_tokens, 20);
        assert_eq!(attr.by_mode["ultra"].output_tokens, 30);
    }

    #[test]
    fn log_basis_prefix_mode_can_be_a_real_mode_not_just_off() {
        let turns = vec![turn(Some(50), 99)];
        let log = vec![ModeLogRow {
            ts: 100,
            mode: Some("ultra".into()),
            prev: Some("lite".into()),
        }];
        let attr = attribute_by_mode(&turns, &log, Some("ultra"), None);
        assert_eq!(attr.by_mode["lite"].output_tokens, 99);
    }

    #[test]
    fn messages_without_a_timestamp_are_unknown_not_guessed() {
        let turns = vec![turn(None, 40), turn(Some(50), 10)];
        let log = vec![ModeLogRow {
            ts: 30,
            mode: Some("full".into()),
            prev: None,
        }];
        let attr = attribute_by_mode(&turns, &log, Some("full"), None);
        assert_eq!(attr.unknown.output_tokens, 40);
        assert_eq!(attr.by_mode["full"].output_tokens, 10);
    }

    #[test]
    fn sidechain_turns_are_isolated_from_by_mode_and_unknown() {
        let mut turns = vec![turn(Some(100), 50)];
        let mut side = turn(Some(150), 999);
        side.is_sidechain = true;
        turns.push(side);
        let attr = attribute_by_mode(&turns, &[], Some("full"), None);
        assert_eq!(attr.by_mode["full"].output_tokens, 50);
        assert_eq!(attr.sidechain.output_tokens, 999);
        assert!(!attr.by_mode.contains_key("none") || attr.by_mode["none"].output_tokens == 0);
    }

    #[test]
    fn buckets_accumulate_all_four_token_fields_not_just_output() {
        let turns = vec![SessionTurn {
            ts: Some(1),
            output_tokens: 10,
            input_tokens: 100,
            cache_creation_input_tokens: 5,
            cache_read_input_tokens: 200,
            is_sidechain: false,
        }];
        let attr = attribute_by_mode(&turns, &[], Some("full"), None);
        let b = &attr.by_mode["full"];
        assert_eq!(b.output_tokens, 10);
        assert_eq!(b.input_tokens, 100);
        assert_eq!(b.cache_creation_input_tokens, 5);
        assert_eq!(b.cache_read_input_tokens, 200);
    }

    #[test]
    fn huge_counters_saturate_instead_of_panicking_or_wrapping() {
        let turns = vec![
            SessionTurn {
                ts: Some(1),
                output_tokens: u64::MAX,
                input_tokens: u64::MAX,
                cache_creation_input_tokens: u64::MAX,
                cache_read_input_tokens: u64::MAX,
                is_sidechain: false,
            },
            SessionTurn {
                ts: Some(2),
                output_tokens: 1,
                input_tokens: 1,
                cache_creation_input_tokens: 1,
                cache_read_input_tokens: 1,
                is_sidechain: false,
            },
        ];
        let bucket = &attribute_by_mode(&turns, &[], Some("full"), None).by_mode["full"];
        assert_eq!(bucket.output_tokens, u64::MAX);
        assert_eq!(bucket.input_tokens, u64::MAX);
        assert_eq!(bucket.cache_creation_input_tokens, u64::MAX);
        assert_eq!(bucket.cache_read_input_tokens, u64::MAX);
    }

    #[test]
    fn direct_attribution_sorts_out_of_order_transition_rows() {
        let turns = vec![turn(Some(150), 10), turn(Some(250), 20)];
        let log = vec![
            ModeLogRow {
                ts: 200,
                mode: Some("ultra".into()),
                prev: Some("full".into()),
            },
            ModeLogRow {
                ts: 100,
                mode: Some("full".into()),
                prev: None,
            },
        ];
        let attr = attribute_by_mode(&turns, &log, Some("ultra"), None);
        assert_eq!(attr.by_mode["full"].output_tokens, 10);
        assert_eq!(attr.by_mode["ultra"].output_tokens, 20);
    }

    proptest! {
        #[test]
        fn attribution_never_loses_a_main_turn_due_to_order_or_clock_skew(
            values in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let turns = values.iter().map(|value| SessionTurn {
                ts: Some((*value as i64) - 32),
                output_tokens: 1,
                input_tokens: 2,
                cache_creation_input_tokens: 3,
                cache_read_input_tokens: 4,
                is_sidechain: false,
            }).collect::<Vec<_>>();
            let attr = attribute_by_mode(&turns, &[], Some("full"), None);
            let output: u64 = attr.by_mode.values().map(|b| b.output_tokens).sum();
            prop_assert_eq!(output, turns.len() as u64);
        }
    }
}
