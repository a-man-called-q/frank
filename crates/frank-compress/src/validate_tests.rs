#[cfg(test)]
mod tests {
    use crate::validate::*;

    #[test]
    fn identical_text_is_valid_with_no_findings() {
        let text = "# Title\n\nSome prose with a [link](https://x.com) and `code`.\n";
        let r = validate(text, text);
        assert!(r.is_valid());
        assert!(r.findings.is_empty());
    }

    #[test]
    fn code_block_mismatch_is_an_error() {
        let orig = "before\n```\nlet x = 1;\n```\nafter";
        let comp = "before\n```\nlet x = 2;\n```\nafter";
        let r = validate(orig, comp);
        assert!(!r.is_valid());
        assert!(r.errors().any(|f| f.message.contains("Code blocks")));
    }

    #[test]
    fn code_block_order_matters() {
        let orig = "```\na\n```\ntext\n```\nb\n```\n";
        let comp = "```\nb\n```\ntext\n```\na\n```\n";
        let r = validate(orig, comp);
        assert!(!r.is_valid(), "swapping block order must be an error even though the set of blocks is the same");
    }

    #[test]
    fn nested_fences_extracted_as_one_block() {
        let text = "````\nouter\n```\ninner\n```\nstill outer\n````\n";
        let blocks = extract_code_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("inner"));
    }

    #[test]
    fn unclosed_fence_is_silently_dropped_not_flagged() {
        let orig = "```\nunclosed forever";
        let comp = "```\nunclosed forever, but reworded";
        let r = validate(orig, comp);
        // Neither side has any *closed* block, so there's nothing to compare —
        // matches the archive's "unclosed fences indicate malformed markdown,
        // including them would cause false-positive failures" rationale.
        assert!(r.errors().next().is_none());
    }

    #[test]
    fn url_loss_is_an_error() {
        let orig = "See https://example.com/docs for details.";
        let comp = "See docs for details.";
        let r = validate(orig, comp);
        assert!(!r.is_valid());
        assert!(r.errors().any(|f| f.message.contains("URL")));
    }

    #[test]
    fn heading_count_mismatch_is_error_but_text_change_is_only_warning() {
        let orig = "# One\n\n## Two\n";
        let comp_missing = "# One\n";
        let r1 = validate(orig, comp_missing);
        assert!(!r1.is_valid());
        assert!(r1.errors().any(|f| f.message.contains("Heading count")));

        let comp_reworded = "# One Reworded\n\n## Two\n";
        let r2 = validate(orig, comp_reworded);
        assert!(r2.is_valid(), "heading text change alone must not be an error");
        assert!(r2.warnings().any(|f| f.message.contains("Heading text")));
    }

    #[test]
    fn inline_code_multiset_catches_a_dropped_duplicate() {
        // Set equality alone would miss this: {"x"} vs {"x"} looks unchanged
        // even though one of two occurrences vanished.
        let orig = "Use `x` here and `x` again there.";
        let comp = "Use `x` here.";
        let r = validate(orig, comp);
        assert!(!r.is_valid());
        assert!(r.errors().any(|f| f.message.contains("Inline code lost")));
    }

    #[test]
    fn inline_code_addition_is_only_a_warning() {
        let orig = "Plain text.";
        let comp = "Plain text with `new_code` added.";
        let r = validate(orig, comp);
        assert!(r.is_valid());
        assert!(r.warnings().any(|f| f.message.contains("Inline code added")));
    }

    #[test]
    fn path_change_is_only_a_warning() {
        let orig = "See /etc/config.yml for settings.";
        let comp = "See settings file for details.";
        let r = validate(orig, comp);
        assert!(r.is_valid());
        assert!(r.warnings().any(|f| f.message.contains("Path mismatch")));
    }

    #[test]
    fn bullet_count_within_15_percent_is_fine() {
        let orig = "- a\n- b\n- c\n- d\n- e\n- f\n- g\n- h\n- i\n- j\n";
        let comp = "- a\n- b\n- c\n- d\n- e\n- f\n- g\n- h\n- i\n";
        let r = validate(orig, comp);
        assert!(r.warnings().next().is_none() || !r.warnings().any(|f| f.message.contains("Bullet")));
    }

    #[test]
    fn bullet_count_beyond_15_percent_warns() {
        let orig = "- a\n- b\n- c\n- d\n- e\n- f\n- g\n- h\n- i\n- j\n";
        let comp = "- a\n- b\n";
        let r = validate(orig, comp);
        assert!(r.warnings().any(|f| f.message.contains("Bullet count changed too much")));
    }
}
