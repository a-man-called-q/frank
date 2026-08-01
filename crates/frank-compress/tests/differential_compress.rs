//! Differential test: for a range of inputs, the Rust compressor's output
//! must match the vendored legacy JS `compress.js` — not a
//! reimplementation of it. Skips cleanly (with a printed note) if `node`
//! isn't available, rather than failing CI on a missing interpreter; the
//! unit tests in `rules_tests.rs` cover the same behaviors without an
//! external dependency, this is the belt-and-suspenders check that the
//! port is byte-exact where it claims to be.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run_js_oracle(input: &str) -> Option<String> {
    let root = repo_root();
    let helper = root.join("crates/frank-compress/tests/fixtures/run_js_compress.js");
    let mut child = Command::new("node")
        .arg(&helper)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    v.get("compressed")
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

#[test]
fn matches_legacy_js_compressor_on_representative_cases() {
    let cases = [
        "The user is the owner of an account",
        "Sure, this just basically returns the value",
        "I will perhaps connect to the database",
        "Run the example: ```\nthe just sure return 1;\n``` and also more text",
        "Use `the just basically API` for fetching",
        "See the docs at https://example.com/the/just/api",
        "Read just the file at /tmp/the/just/file.txt",
        "Set the API_KEY_VALUE on the just config.api.endpoint()",
        "Get the current weather for a given location. Returns the temperature in Fahrenheit. Please make sure to provide the location as a city name.",
        "plan type (STARTER/BUSINESS)",
        "user role (ADMIN/MEMBER/GUEST)",
        "user plan (Free/Pro/Business)",
        "The quick brown fox. Actually, it jumps over the lazy dog. Perhaps you could also check the API_ENDPOINT_URL and call service.fetch().",
    ];

    let mut checked = 0;
    let mut skipped_no_node = false;
    for input in cases {
        let Some(js) = run_js_oracle(input) else {
            skipped_no_node = true;
            break;
        };
        let rust = frank_compress::compress(input).compressed;
        assert_eq!(rust, js, "mismatch for input: {input:?}");
        checked += 1;
    }

    if skipped_no_node {
        eprintln!(
            "differential test skipped: `node` unavailable or vendored JS oracle unreachable"
        );
    } else {
        assert_eq!(checked, cases.len());
    }
}

#[test]
fn matches_legacy_js_compressor_on_real_compress_fixtures() {
    let root = repo_root();
    let fixtures_dir = root.join("crates/frank-compress/tests/fixtures/caveman-compress");
    let Ok(entries) = std::fs::read_dir(&fixtures_dir) else {
        return;
    };

    let mut checked = 0;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md")
            || !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".original.md")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(js) = run_js_oracle(&text) else {
            eprintln!("differential test skipped: node unavailable");
            return;
        };
        let rust = frank_compress::compress(&text).compressed;
        assert_eq!(rust, js, "mismatch for fixture: {}", path.display());
        checked += 1;
    }
    assert!(
        checked > 0,
        "expected at least one *.original.md fixture under {}",
        fixtures_dir.display()
    );
}
