//! Reads `.frank-mode-log.jsonl`, written by `frank_state::engine`'s
//! `record_mode_change`. Ported from `archive/src/hooks/caveman-stats.js`'s
//! `readModeLog` — rows with a non-whitelisted `mode`/`prev` are rejected
//! outright rather than coerced, since a corrupted or tampered log row
//! must never silently attribute tokens to the wrong mode.

use std::path::Path;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ModeLogRow {
    pub ts: i64,
    pub mode: Option<String>,
    pub prev: Option<String>,
}

fn normalize(v: Option<&Value>, valid: &[&str]) -> Option<Option<String>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) if valid.contains(&s.as_str()) => Some(Some(s.clone())),
        _ => None,
    }
}

/// Returns rows sorted by `ts`. `valid` is the whitelist of legal mode
/// strings for the pack in play (level ids + oneshot ids + `"off"`).
pub fn read_mode_log(path: &Path, valid: &[&str]) -> Vec<ModeLogRow> {
    let mut rows: Vec<ModeLogRow> = frank_safeio::read_lines(path)
        .iter()
        .filter_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            let ts = v.get("ts")?.as_i64()?;
            let mode = normalize(v.get("mode"), valid)?;
            let prev = normalize(v.get("prev"), valid)?;
            Some(ModeLogRow { ts, mode, prev })
        })
        .collect();
    rows.sort_by_key(|r| r.ts);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn mode_log_filters_malformed_values_and_sorts_clock_skew() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("mode.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"ts\":20,\"mode\":\"full\",\"prev\":null}\n",
                "{\"ts\":10,\"mode\":\"off\",\"prev\":\"full\"}\n",
                "{\"ts\":30,\"mode\":\"unknown\",\"prev\":null}\n",
                "partial\n",
                "{\"ts\":15,\"mode\":null,\"prev\":\"off\"}\n"
            ),
        )
        .unwrap();
        let rows = read_mode_log(&path, &["full", "off"]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].ts, 10);
        assert_eq!(rows[1].ts, 15);
        assert_eq!(rows[1].mode, None);
        assert_eq!(rows[2].mode.as_deref(), Some("full"));
    }
}
