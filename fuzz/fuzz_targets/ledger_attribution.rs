#![no_main]

use frank_ledger::attribution::attribute_by_mode;
use frank_ledger::mode_log::ModeLogRow;
use frank_ledger::session::SessionTurn;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let turns = bytes
        .chunks(24)
        .take(256)
        .map(|chunk| SessionTurn {
            ts: (chunk.len() >= 8).then(|| i64::from_le_bytes(chunk[..8].try_into().unwrap())),
            output_tokens: chunk.get(8).copied().unwrap_or_default() as u64,
            input_tokens: chunk.get(9).copied().unwrap_or_default() as u64,
            cache_creation_input_tokens: chunk.get(10).copied().unwrap_or_default() as u64,
            cache_read_input_tokens: chunk.get(11).copied().unwrap_or_default() as u64,
            is_sidechain: chunk.get(12).copied().unwrap_or_default() & 1 == 1,
        })
        .collect::<Vec<_>>();
    let mode_log = bytes
        .chunks(16)
        .take(64)
        .map(|chunk| ModeLogRow {
            ts: chunk
                .get(..8)
                .and_then(|value| value.try_into().ok())
                .map(i64::from_le_bytes)
                .unwrap_or_default(),
            mode: Some("full".to_string()),
            prev: None,
        })
        .collect::<Vec<_>>();
    let _ = attribute_by_mode(&turns, &mode_log, Some("full"), None);
});

