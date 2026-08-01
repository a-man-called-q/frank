//! The stdio proxy process. Ported from
//! historical Caveman MCP proxy: spawns an upstream
//! MCP server, passes client→upstream through raw and unbuffered, and
//! line-buffers upstream→client so each JSON-RPC message can be parsed,
//! rewritten, and re-serialized — falling back to passthrough on a line
//! that doesn't parse as JSON, rather than dropping it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread;

use crate::transform::transform_response;

pub struct ProxyConfig {
    pub upstream_cmd: String,
    pub upstream_args: Vec<String>,
    pub fields: Vec<String>,
}

impl ProxyConfig {
    pub fn default_fields() -> Vec<String> {
        vec!["description".to_string()]
    }
}

/// Runs the proxy to completion, returning the exit code to propagate
/// (mirroring the archive's `128 + signal` / `code || 0` handling, minus
/// signal-number detail Rust's `std::process` doesn't expose portably).
pub fn run(config: ProxyConfig) -> i32 {
    let mut cmd = Command::new(&config.upstream_cmd);
    cmd.args(&config.upstream_args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());
    // Windows .cmd/.ps1 upstream shims (e.g. `npx some-mcp-server`) need a
    // shell to resolve, which `std::process::Command` doesn't provide the
    // way Node's `shell: true` does — real cross-platform spawn handling
    // lands with the rest of M6's Windows work; POSIX is the target here.

    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("frank-mcp: failed to spawn upstream '{}': {e}", config.upstream_cmd);
            return 1;
        }
    };

    let mut child_stdin = child.stdin.take().expect("piped stdin");
    let child_stdout = child.stdout.take().expect("piped stdout");

    // client -> upstream: raw, unbuffered passthrough.
    let stdin_forwarder = thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut stdin = io::stdin();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if child_stdin.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // upstream -> client: line-buffered, rewritten.
    let reader = BufReader::new(child_stdout);
    let mut stdout = io::stdout();
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let out_line = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(mut v) => {
                transform_response(&mut v, &config.fields);
                v.to_string()
            }
            Err(_) => line,
        };
        if writeln!(stdout, "{out_line}").is_err() || stdout.flush().is_err() {
            break;
        }
    }

    let _ = stdin_forwarder.join();
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}
