#![no_main]

use frank_target::read_settings;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let path = std::env::temp_dir().join(format!("frank-jsonc-{}", std::process::id()));
    if std::fs::write(&path, bytes).is_ok() {
        let _ = read_settings(&path);
    }
    let _ = std::fs::remove_file(path);
});

