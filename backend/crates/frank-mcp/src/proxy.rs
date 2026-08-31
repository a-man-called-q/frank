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
            eprintln!(
                "frank-mcp: failed to spawn upstream '{}': {e}",
                config.upstream_cmd
            );
            return 1;
        }
    };

    let child_stdin = child.stdin.take().expect("piped stdin");
    let child_stdout = child.stdout.take().expect("piped stdout");

    // client -> upstream: raw, unbuffered passthrough.
    let stdin_forwarder = thread::spawn(move || {
        let _ = forward_client(io::stdin(), child_stdin);
    });

    // upstream -> client: line-buffered, rewritten.
    let reader = BufReader::new(child_stdout);
    let mut stdout = io::stdout();
    let _ = process_upstream(reader, &mut stdout, &config.fields);

    let _ = stdin_forwarder.join();
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}

/// Copy client input to the upstream without line buffering or rewriting.
/// Keeping this generic makes the byte-for-byte contract testable without
/// launching a process or touching the real stdin/stdout handles.
fn forward_client<R: Read, W: Write>(mut reader: R, mut writer: W) -> io::Result<()> {
    io::copy(&mut reader, &mut writer).map(|_| ())
}

/// Rewrite one upstream line at a time. Blank lines are protocol noise and
/// are omitted; malformed JSON is deliberately passed through unchanged.
fn process_upstream<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    fields: &[String],
) -> io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let out_line = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(mut value) => {
                transform_response(&mut value, fields);
                value.to_string()
            }
            Err(_) => line,
        };
        writeln!(writer, "{out_line}")?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ProxyConfig, forward_client, process_upstream, run};
    use std::io::{self, BufReader, Cursor, Write};

    fn fields() -> Vec<String> {
        vec!["description".to_string()]
    }

    #[test]
    fn default_fields_are_the_protocol_description_field() {
        assert_eq!(ProxyConfig::default_fields(), fields());
    }

    #[derive(Default)]
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "simulated failure",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn client_forwarding_is_byte_for_byte() {
        let mut output = Vec::new();
        forward_client(Cursor::new(b"first\nsecond\0"), &mut output).unwrap();
        assert_eq!(output, b"first\nsecond\0");
    }

    #[test]
    fn upstream_processing_rewrites_json_and_passes_non_json() {
        let input = concat!(
            "\n",
            "{\"result\":{\"tools\":[{\"description\":\"Please just fetch this URL.\"}]}}\n",
            "not-json\n"
        );
        let mut output = Vec::new();
        process_upstream(BufReader::new(Cursor::new(input)), &mut output, &fields()).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.lines().any(|line| line == "not-json"));
        assert!(text.lines().any(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            !value["result"]["tools"][0]["description"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase()
                .contains("please")
        }));
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn upstream_processing_reports_writer_errors() {
        let input = "{\"result\":{\"tools\":[]}}\n";
        let error = process_upstream(BufReader::new(Cursor::new(input)), FailingWriter, &fields())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn missing_upstream_returns_one() {
        let code = run(ProxyConfig {
            upstream_cmd: "frank-command-that-does-not-exist".to_string(),
            upstream_args: Vec::new(),
            fields: fields(),
        });
        assert_eq!(code, 1);
    }

    #[cfg(unix)]
    #[test]
    fn upstream_exit_code_is_propagated() {
        let code = run(ProxyConfig {
            upstream_cmd: "sh".to_string(),
            upstream_args: vec!["-c".to_string(), "exit 7".to_string()],
            fields: fields(),
        });
        assert_eq!(code, 7);
    }
}
