#![no_main]

// These are `frank-app`'s desktop-adapter payload types. The desktop client is
// a separate Flutter package and does not change what needs fuzzing here.
// `PackOperation` is fuzzed alongside the other two
// now: it's the one payload type through which the GUI supplies a
// filesystem path (`PackOperation::Add { source: PathBuf, .. }`), and it was
// missing from the original three-type coverage.
use frank_app::{PackOperation, TargetOperation, UserSettingsPatch};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else { return };
    let _ = serde_json::from_value::<UserSettingsPatch>(value.clone());
    let _ = serde_json::from_value::<TargetOperation>(value.clone());
    let _ = serde_json::from_value::<PackOperation>(value);
});
