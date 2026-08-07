mod commands;
mod compress_cmd;
mod error;
mod hook;
mod pack_cmd;
mod stats;
mod targets_cmd;

use clap::{Parser, Subcommand};
use frank_app::{FrankPaths, FrankService};

use error::report;

/// `hook` is dispatched before clap's `Command` tree is even constructed —
/// clap's builder allocation is the dominant startup cost in a binary this
/// small, and hook invocations run on every SessionStart / UserPromptSubmit
/// / statusline call, so they're the path that has to be fast. See
/// AGENTS.md's "contracts that must not drift" section.
fn main() {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() >= 3 && raw[1] == "hook" {
        let name = raw[2].clone();
        let svc = FrankService::new(FrankPaths::from_process());
        // Backstop: hook::dispatch is written to never panic, but a hook
        // exiting non-zero (or crashing) can break the host agent's turn,
        // so this is a hard guarantee, not an optimization.
        let code = guarded_hook(|| hook::dispatch(&svc, &name));
        std::process::exit(code);
    }
    std::process::exit(run());
}

fn guarded_hook<F>(run: F) -> i32
where
    F: FnOnce() -> i32,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or(0)
}

#[derive(Parser)]
#[command(
    name = "frank",
    about = "Frank — an extensible output-style engine for AI coding agents",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Internal: agent lifecycle hook entry points. Normally intercepted
    /// before this parser runs; reaching here means the fast-dispatch check
    /// in `main()` didn't match, which is itself a bug worth surfacing.
    Hook { name: String },
    /// Activate Frank at LEVEL (defaults to the active pack's default level).
    On { level: Option<String> },
    /// Deactivate Frank.
    Off,
    /// Print the active level, or "off".
    Status,
    /// List levels available in the active pack.
    Levels,
    /// Install Frank into a supported agent. Defaults to Claude Code;
    /// pass --only for a declarative target (see `frank targets`).
    Install {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        only: Option<String>,
    },
    /// Remove Frank, leaving anything Frank doesn't own untouched.
    Uninstall {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        only: Option<String>,
    },
    /// Report whether Frank's hooks are correctly installed.
    Doctor,
    /// List known targets and whether each is detected on this machine.
    Targets {
        #[arg(long)]
        detected: bool,
        #[arg(long)]
        json: bool,
    },
    /// Real token usage for the active (or a given) session, plus an
    /// honestly-labelled savings estimate. See AGENTS.md for the
    /// measured-vs-estimated accounting rules this enforces.
    Stats {
        #[arg(long)]
        session: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        share: bool,
        #[arg(long)]
        explain: bool,
    },
    /// Deterministic, offline prose compression — no API key, no `claude`
    /// CLI. See AGENTS.md: expect ~10-20% reduction on prose, not the
    /// archive's ~46% (that number came from its LLM-backed path, which
    /// this deliberately does not reimplement).
    Compress {
        paths: Vec<std::path::PathBuf>,
        /// Report the size reduction without writing anything.
        #[arg(long)]
        check: bool,
        /// Show what would happen (backup + write) without doing it.
        #[arg(long)]
        dry_run: bool,
        /// Restore each path from its backup instead of compressing.
        #[arg(long)]
        restore: bool,
    },
    /// stdio MCP proxy: wraps an upstream MCP server, compressing tool/
    /// prompt description fields in transit. Not a real MCP
    /// implementation — see frank-mcp's crate docs.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Manage local third-party persona packs.
    Pack {
        #[command(subcommand)]
        command: PackCommand,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    Proxy {
        /// Comma-separated field names to compress (default: "description").
        #[arg(long)]
        fields: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        upstream: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PackCommand {
    /// Validate, preview, and install a local directory containing pack.toml.
    Add {
        source: String,
        #[arg(long)]
        sha256: Option<String>,
        /// Skip the activation-prompt confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// List the built-in pack and locally installed packs.
    List,
    /// Select a pack by id or id@version.
    Use { selector: String },
    /// Remove a locally installed pack.
    Remove { selector: String },
    /// Compile a source pack without installing it.
    Build { path: Option<std::path::PathBuf> },
    /// Show pack metadata and compiled levels.
    Show { selector: Option<String> },
}

fn run() -> i32 {
    let cli = Cli::parse();
    let svc = FrankService::new(FrankPaths::from_process());
    match cli.command {
        Command::Hook { name } => hook::dispatch(&svc, &name),
        Command::On { level } => report(commands::on(&svc, level.as_deref())),
        Command::Off => report(commands::off(&svc)),
        Command::Status => report(commands::status(&svc)),
        Command::Levels => report(commands::levels(&svc)),
        Command::Install { dry_run, only } => {
            report(commands::install(&svc, dry_run, only.as_deref()))
        }
        Command::Uninstall { dry_run, only } => {
            report(commands::uninstall(&svc, dry_run, only.as_deref()))
        }
        Command::Doctor => commands::doctor(&svc),
        Command::Targets { detected, json } => targets_cmd::run(&svc, detected, json),
        Command::Stats {
            session,
            json,
            all,
            share,
            explain,
        } => {
            let args = stats::StatsArgs {
                session,
                json,
                all,
                share,
                explain,
            };
            print!("{}", stats::report_text(&svc, &args));
            0
        }
        Command::Compress {
            paths,
            check,
            dry_run,
            restore,
        } => compress_cmd::run(compress_cmd::CompressArgs {
            paths,
            check,
            dry_run,
            restore,
        }),
        Command::Mcp { command } => match command {
            McpCommand::Proxy { fields, upstream } => {
                let Some((cmd, args)) = upstream.split_first() else {
                    eprintln!(
                        "frank mcp proxy: no upstream command given (usage: frank mcp proxy -- <cmd> [args...])"
                    );
                    return 2;
                };
                let fields = fields
                    .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_else(frank_mcp::ProxyConfig::default_fields);
                frank_mcp::run(frank_mcp::ProxyConfig {
                    upstream_cmd: cmd.clone(),
                    upstream_args: args.to_vec(),
                    fields,
                })
            }
        },
        Command::Pack { command } => match command {
            PackCommand::Add {
                source,
                sha256,
                yes,
            } => report(pack_cmd::add(&svc, &source, sha256.as_deref(), yes)),
            PackCommand::List => report(pack_cmd::list(&svc)),
            PackCommand::Use { selector } => report(pack_cmd::use_pack(&svc, &selector)),
            PackCommand::Remove { selector } => report(pack_cmd::remove(&svc, &selector)),
            PackCommand::Build { path } => {
                let path = path.unwrap_or_else(|| {
                    std::path::PathBuf::from(format!("packs/{}", frank_app::BUILTIN_PACK_ID))
                });
                match pack_cmd::build(&path) {
                    Ok(compiled) => {
                        println!("frank: compiled {} v{}", compiled.id, compiled.version);
                        0
                    }
                    Err(e) => {
                        eprintln!("frank pack build: {e}");
                        1
                    }
                }
            }
            PackCommand::Show { selector } => report(pack_cmd::show(&svc, selector.as_deref())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::guarded_hook;

    #[test]
    fn a_caught_hook_panic_is_a_successful_noop() {
        assert_eq!(guarded_hook(|| panic!("simulated hook panic")), 0);
    }

    #[test]
    fn a_successful_hook_result_passes_through() {
        assert_eq!(guarded_hook(|| 7), 7);
    }
}
