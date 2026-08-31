//! Sensitive-path denylist — never compress a file that almost certainly
//! holds secrets or PII, even if `classify` would otherwise call it
//! natural language. Ported verbatim from
//! `archive/skills/caveman-compress/scripts/compress.py`'s
//! `is_sensitive_path`. This is the one safety behavior from that file
//! kept as a hard, unconditional refuse — see `AGENTS.md` on why the rest
//! of `compress.py` (the LLM orchestration) was dropped rather than
//! ported.

use std::path::Path;

use regex::Regex;

const SENSITIVE_PATH_COMPONENTS: &[&str] = &[".ssh", ".aws", ".gnupg", ".kube", ".docker"];
const SENSITIVE_NAME_TOKENS: &[&str] = &[
    "secret",
    "credential",
    "password",
    "passwd",
    "apikey",
    "accesskey",
    "token",
    "privatekey",
];

fn sensitive_basename_regex() -> Regex {
    Regex::new(
        r"(?i)^(\.env(\..+)?|\.netrc|credentials(\..+)?|secrets?(\..+)?|passwords?(\..+)?|id_(rsa|dsa|ecdsa|ed25519)(\.pub)?|authorized_keys|known_hosts|.*\.(pem|key|p12|pfx|crt|cer|jks|keystore|asc|gpg))$",
    )
    .expect("static pattern")
}

pub fn is_sensitive_path(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if sensitive_basename_regex().is_match(name) {
        return true;
    }

    let has_sensitive_component = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|part| SENSITIVE_PATH_COMPONENTS.contains(&part.to_lowercase().as_str()));
    if has_sensitive_component {
        return true;
    }

    // Normalize separators so "api-key" and "api_key" both match "apikey".
    let normalize = Regex::new(r"[_\-\s.]").unwrap();
    let normalized = normalize.replace_all(&name.to_lowercase(), "").into_owned();
    SENSITIVE_NAME_TOKENS
        .iter()
        .any(|tok| normalized.contains(tok))
}
