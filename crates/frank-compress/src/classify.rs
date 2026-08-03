//! File-type classification: is this file natural language (compressible),
//! code, config, or unknown? Ported verbatim from
//! `archive/skills/caveman-compress/scripts/detect.py` — this table is
//! correct and cheap as-is; the only change is a typed `FileClass` enum in
//! place of Python's string literals.

use std::path::Path;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    NaturalLanguage,
    Code,
    Config,
    Unknown,
}

const COMPRESSIBLE_EXTENSIONS: &[&str] = &["md", "txt", "markdown", "rst", "typ", "typst", "tex"];

const SKIP_EXTENSIONS_CODE: &[&str] = &[
    "py",
    "js",
    "ts",
    "tsx",
    "jsx",
    "css",
    "scss",
    "html",
    "xml",
    "sql",
    "sh",
    "bash",
    "zsh",
    "go",
    "rs",
    "java",
    "c",
    "cpp",
    "h",
    "hpp",
    "rb",
    "php",
    "swift",
    "kt",
    "lua",
    "dockerfile",
    "makefile",
    "csv",
];

const SKIP_EXTENSIONS_CONFIG: &[&str] =
    &["json", "yaml", "yml", "toml", "env", "lock", "ini", "cfg"];

/// `Dockerfile` has no suffix so the `.dockerfile` rule above never
/// matches it, and `CMakeLists.txt` would otherwise ride the compressible
/// `.txt` rule — checked by basename before any extension rule.
const KNOWN_CODE_FILENAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "gnumakefile",
    "jenkinsfile",
    "vagrantfile",
    "rakefile",
    "gemfile",
    "justfile",
    "procfile",
    "brewfile",
    "cmakelists.txt",
];

fn code_patterns() -> Vec<Regex> {
    [
        r"^\s*(import |from .+ import |require\(|const |let |var )",
        r"^\s*(def |class |function |async function |export )",
        r"^\s*(if\s*\(|for\s*\(|while\s*\(|switch\s*\(|try\s*\{)",
        r"^\s*[\}\]\);]+\s*$",
        r"^\s*@\w+",
        r#"^\s*"[^"]+"\s*:\s*"#,
        r#"^\s*\w+\s*=\s*[{\[("']"#,
    ]
    .iter()
    .map(|p| Regex::new(p).expect("static pattern"))
    .collect()
}

fn is_code_line(line: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|p| p.is_match(line))
}

fn is_json_content(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

fn is_yaml_content(lines: &[&str]) -> bool {
    let kv = Regex::new(r"^\w[\w\s]*:\s").expect("static pattern");
    // Three independent heuristics, ported line-for-line from detect.py's
    // `elif` chain — each just increments the same counter, which reads as
    // "identical blocks" to clippy but is the faithful structure.
    #[allow(clippy::if_same_then_else)]
    let is_indicator = |stripped: &str| -> bool {
        if stripped.starts_with("---") {
            true
        } else if kv.is_match(stripped) {
            true
        } else {
            stripped.starts_with("- ") && stripped.contains(':')
        }
    };
    let sample = &lines[..lines.len().min(30)];
    let indicators = sample.iter().filter(|l| is_indicator(l.trim())).count() as i32;
    let non_empty = sample.iter().filter(|l| !l.trim().is_empty()).count();
    if non_empty == 0 {
        return false;
    }
    (indicators as f64 / non_empty as f64) > 0.6
}

pub fn detect_file_type(path: &Path) -> FileClass {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    let name_lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    if KNOWN_CODE_FILENAMES.contains(&name_lower.as_str()) {
        return FileClass::Code;
    }

    if let Some(ext) = &ext {
        if COMPRESSIBLE_EXTENSIONS.contains(&ext.as_str()) {
            return FileClass::NaturalLanguage;
        }
        if SKIP_EXTENSIONS_CODE.contains(&ext.as_str()) {
            return FileClass::Code;
        }
        if SKIP_EXTENSIONS_CONFIG.contains(&ext.as_str()) {
            return FileClass::Config;
        }
        return FileClass::Unknown;
    }

    // Extensionless: sniff content.
    let Ok(text) = std::fs::read_to_string(path) else {
        return FileClass::Unknown;
    };
    let lines: Vec<&str> = text.lines().take(50).collect();

    if text.starts_with("#!") {
        return FileClass::Code;
    }
    let sample_len = text.len().min(10_000);
    if is_json_content(&text[..sample_len]) {
        return FileClass::Config;
    }
    if is_yaml_content(&lines) {
        return FileClass::Config;
    }

    let patterns = code_patterns();
    let code_lines = lines
        .iter()
        .filter(|l| !l.trim().is_empty() && is_code_line(l, &patterns))
        .count();
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count();
    if non_empty != 0 && (code_lines as f64 / non_empty as f64) > 0.4 {
        return FileClass::Code;
    }

    FileClass::NaturalLanguage
}

pub fn should_compress(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".original.md"))
    {
        return false;
    }
    detect_file_type(path) == FileClass::NaturalLanguage
}
