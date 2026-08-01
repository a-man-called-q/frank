#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let result = frank_compress::compress(text);
        let _ = frank_compress::validate(text, &result.compressed);
    }
});
