#![no_main]

use frank_target::markdown_block::{append, remove, strip_all, Block};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let block = Block { begin: "<!-- frank:begin -->".into(), end: "<!-- frank:end -->".into() };
        let _ = strip_all(text, &block);
        let _ = remove(text, &block);
        let _ = append(text, &block, "fuzz body");
    }
});

