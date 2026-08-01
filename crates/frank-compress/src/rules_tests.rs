#[cfg(test)]
mod tests {
    use crate::rules::*;
    use crate::validate::validate;

    #[test]
    fn drops_articles() {
        let out = compress("The user is the owner of an account").compressed;
        assert!(!out.to_lowercase().contains(" the "), "{out}");
        assert!(!out.to_lowercase().starts_with("the "), "{out}");
        assert!(out.to_lowercase().contains("owner"));
    }

    #[test]
    fn drops_filler_and_pleasantries() {
        let out = compress("Sure, this just basically returns the value").compressed.to_lowercase();
        assert!(!out.contains("sure"));
        assert!(!out.contains("just"));
        assert!(!out.contains("basically"));
    }

    #[test]
    fn drops_hedging_and_i_will_leader() {
        let out = compress("I will perhaps connect to the database").compressed;
        assert!(!out.to_lowercase().contains("perhaps"));
        assert!(!out.to_lowercase().starts_with("i will"));
        assert!(out.to_lowercase().contains("database"));
    }

    #[test]
    fn preserves_fenced_code_blocks_verbatim() {
        let input = "Run the example: ```\nthe just sure return 1;\n``` and also more text";
        let out = compress(input).compressed;
        assert!(out.contains("```\nthe just sure return 1;\n```"), "{out}");
    }

    #[test]
    fn preserves_inline_code_verbatim() {
        let input = "Use `the just basically API` for fetching";
        let out = compress(input).compressed;
        assert!(out.contains("`the just basically API`"), "{out}");
    }

    #[test]
    fn preserves_urls_verbatim() {
        let input = "See the docs at https://example.com/the/just/api";
        let out = compress(input).compressed;
        assert!(out.contains("https://example.com/the/just/api"), "{out}");
    }

    #[test]
    fn preserves_filesystem_paths_verbatim() {
        let input = "Read just the file at /tmp/the/just/file.txt";
        let out = compress(input).compressed;
        assert!(out.contains("/tmp/the/just/file.txt"), "{out}");
    }

    #[test]
    fn preserves_const_case_and_dotted_identifiers() {
        let input = "Set the API_KEY_VALUE on the just config.api.endpoint()";
        let out = compress(input).compressed;
        assert!(out.contains("API_KEY_VALUE"), "{out}");
        assert!(out.contains("config.api.endpoint()"), "{out}");
    }

    #[test]
    fn compresses_real_mcp_style_description_with_meaningful_reduction() {
        let input = "Get the current weather for a given location. \
            Returns the temperature in Fahrenheit. \
            Please make sure to provide the location as a city name.";
        let result = compress(input);
        assert!(result.after < result.before, "{} -> {}", result.before, result.after);
        let reduction = (result.before - result.after) as f64 / result.before as f64;
        assert!(reduction > 0.15, "wanted >15% savings, got {reduction}");
        let lower = result.compressed.to_lowercase();
        assert!(lower.contains("weather"));
        assert!(lower.contains("fahrenheit"));
        assert!(lower.contains("city name"));
    }

    #[test]
    fn handles_empty_input_gracefully() {
        let r = compress("");
        assert_eq!(r.compressed, "");
        assert_eq!(r.before, 0);
        assert_eq!(r.after, 0);
    }

    /// #444: the path pattern matches "STARTER/BUSINESS" and the
    /// function-call pattern matches the whole "type(STARTER/BUSINESS)" on
    /// the *original* text — merging their spans protects the entire
    /// thing in one step, with no restore-pass concept needed at all.
    #[test]
    fn preserves_enum_values_inside_parens_nested_case() {
        let cases = [
            ("plan type (STARTER/BUSINESS)", "STARTER/BUSINESS"),
            ("user role (ADMIN/MEMBER/GUEST)", "ADMIN/MEMBER/GUEST"),
            ("user plan (Free/Pro/Business)", "Free/Pro/Business"),
        ];
        for (input, needle) in cases {
            let out = compress(input).compressed;
            assert!(out.contains(needle), "lost {needle:?} in {out:?}");
        }
    }

    #[test]
    fn articles_strip_before_any_letter_regardless_of_case() {
        // The original regex carries `/i`, and in JS that flag applies to
        // the whole pattern including the lookahead — `(?=[a-z])` under
        // `/gi` matches any letter, not just lowercase. Verified directly
        // against the archive: `compress("The API returned an error")` →
        // `"API returned error"`. See rules.rs's `strip_articles` doc.
        assert_eq!(compress("The API returned an error").compressed, "API returned error");
    }

    #[test]
    fn articles_survive_before_a_non_letter() {
        assert_eq!(compress("a 5-minute task remains").compressed, "A 5-minute task remains");
        assert_eq!(compress("an $100 fee applies").compressed, "An $100 fee applies");
    }

    #[test]
    fn word_around_a_protected_span_keeps_exactly_one_separating_space() {
        let out = compress("Hello there, `code` world.").compressed;
        assert!(!out.contains("`code`world"), "{out}");
        assert!(!out.contains("there,`code`"), "{out}");
        assert!(out.contains("`code`"));
    }

    #[test]
    fn compress_output_always_passes_its_own_validator() {
        let inputs = [
            "The user is the owner of an account, please check the API_KEY_VALUE.",
            "See https://example.com/docs and `inline code` and /etc/config.yml.",
            "# Heading\n\nSome prose with - a bullet\n- another bullet\n",
            "I will perhaps just simply connect to the database, actually.",
        ];
        for input in inputs {
            let out = compress(input).compressed;
            let result = validate(input, &out);
            assert!(result.is_valid(), "input {input:?} -> {out:?} failed: {:?}", result.findings);
        }
    }

    // ---------- property test: protected spans are always byte-identical ----------

    proptest::proptest! {
        #[test]
        fn protected_spans_are_never_altered(
            prefix in "[a-zA-Z ,.]{0,40}",
            code in "[a-zA-Z0-9_]{1,20}",
            suffix in "[a-zA-Z ,.]{0,40}",
        ) {
            let input = format!("{prefix} `{code}` {suffix}");
            let out = compress(&input).compressed;
            proptest::prop_assert!(out.contains(&format!("`{code}`")), "lost inline code span in {out:?}");
        }

        #[test]
        fn compress_never_panics_on_arbitrary_utf8_ish_input(s in ".{0,200}") {
            let _ = compress(&s);
        }
    }
}
