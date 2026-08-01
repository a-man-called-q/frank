#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(raw) = std::str::from_utf8(bytes) {
        let _: Result<frank_pack::PackManifest, _> = toml::from_str(raw);
    }
});
