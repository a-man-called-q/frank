#![no_main]

use frank_app::{TargetOperation, UserSettingsPatch};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else { return };
    let _ = serde_json::from_value::<UserSettingsPatch>(value.clone());
    let _ = serde_json::from_value::<TargetOperation>(value);
});

