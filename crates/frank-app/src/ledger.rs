use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::FrankService;

impl FrankService {
    /// Build and record the ledger report used by both the CLI stats command
    /// and the UserPromptSubmit hook. Keeping session discovery, mode-log
    /// joining, and history writes here prevents the desktop adapter from
    /// drifting away from hook/CLI accounting.
    pub fn build_and_record_stats(
        &self,
        session_override: Option<&Path>,
    ) -> frank_ledger::SessionReport {
        let compiled = self.pack_or_builtin();
        let session_path = session_override
            .map(PathBuf::from)
            .or_else(|| frank_ledger::find_recent_session(&self.paths.config_dir));

        let Some(session_path) = session_path else {
            return frank_ledger::SessionReport {
                session_path: None,
                session_id: None,
                turns: 0,
                model: None,
                attribution: frank_ledger::attribute_by_mode(&[], &[], None, None),
                injection_activate_bytes: 0,
                injection_reinforce_bytes: 0,
            };
        };

        let flag_paths = self.paths.flag_paths();
        let ledger_paths = self.paths.ledger_paths();
        let current_mode = frank_safeio::read_flag(
            &self.paths.active_flag_path(),
            &compiled.valid_flag_values(),
        );
        let flag_mtime_ms = std::fs::metadata(self.paths.active_flag_path())
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64);

        let report = frank_ledger::build_session_report(
            &session_path,
            &flag_paths.mode_log,
            &ledger_paths.ledger,
            &compiled,
            current_mode.as_deref(),
            flag_mtime_ms,
        );

        if report.turns > 0 {
            if let Some(session_id) = &report.session_id {
                frank_ledger::append_history(
                    &ledger_paths.history,
                    &frank_ledger::HistoryRow {
                        ts: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|duration| duration.as_millis() as i64)
                            .unwrap_or(0),
                        session_id: session_id.clone(),
                        model: report.model.clone(),
                        output_tokens: frank_ledger::measured_output_total(&report.attribution),
                        input_tokens: frank_ledger::measured_input_total(&report.attribution),
                        turns: report.turns,
                    },
                );
            }
        }

        report
    }
}
