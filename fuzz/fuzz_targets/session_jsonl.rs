#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let dir = std::env::temp_dir().join(format!("frank-fuzz-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("session.jsonl");
    if std::fs::write(&path, bytes).is_ok() {
        let _ = frank_ledger::session::parse_session(&path);
    }
    let _ = std::fs::remove_file(path);
});
