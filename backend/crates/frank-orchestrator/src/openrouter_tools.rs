//! OpenRouter tool execution facade.
//!
//! Domain handlers live under `tools/`; this module owns only the
//! transport-independent policy gate and the daemon entry point.

use frank_protocol::*;
use serde_json::{Value, json};

use crate::{Orchestrator, organization_tool_denial, tools};

impl Orchestrator {
    pub async fn execute_openrouter_tool(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        workspace_root: &str,
        name: &str,
        input: &Value,
    ) -> Value {
        match self
            .execute_openrouter_tool_inner(task_id, agent_id, workspace_root, name, input)
            .await
        {
            Ok(result) => json!({"ok": true, "result": result}),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    async fn execute_openrouter_tool_inner(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        workspace_root: &str,
        name: &str,
        input: &Value,
    ) -> std::result::Result<Value, String> {
        let route = tools::parse_route(name)?;
        if !tools::owner_is_orchestrator(&route) {
            return Err("tool execution owner is not the orchestrator".into());
        }
        let snapshot = self
            .store
            .snapshot()
            .await
            .map_err(|error| error.to_string())?;
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| "task is outside the session scope".to_string())?
            .clone();
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id && !agent.archived)
            .ok_or_else(|| "agent is outside the session scope".to_string())?
            .clone();
        let name = route.id.as_str();
        if let Some(reason) = tool_policy_denial(&agent.policy, name) {
            return Err(reason);
        }
        if let Some(reason) = organization_tool_denial(&snapshot, agent_id, name) {
            return Err(reason);
        }
        let root = task
            .worktree
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .unwrap_or(workspace_root);
        let root = std::fs::canonicalize(root)
            .map_err(|error| format!("task worktree is unavailable: {error}"))?;
        let context = tools::ToolExecutionContext {
            orchestrator: self,
            task_id,
            agent_id,
            workspace_root: &root,
            task: &task,
            agent: &agent,
            snapshot: &snapshot,
        };
        tools::dispatch(&route, &context, input)
            .await?
            .ok_or_else(|| format!("unknown OpenRouter tool: {name}"))
    }

    pub(crate) async fn command_from_agent(
        &self,
        agent_id: AgentId,
        command: Command,
    ) -> std::result::Result<Value, String> {
        let response = Box::pin(self.execute(
            CommandEnvelope {
                protocol_version: PROTOCOL_VERSION,
                command_id: CommandId::new(),
                expected_revision: None,
                command,
            },
            ActorRef {
                kind: ActorKind::Agent,
                id: Some(agent_id.to_string()),
                display_name: None,
            },
            DeviceRole::Operator,
        ))
        .await;
        if let Some(error) = response.error {
            return Err(error.message);
        }
        serde_json::to_value(response).map_err(|error| error.to_string())
    }
}

pub(crate) fn tool_is_write(name: &str) -> bool {
    frank_tool_catalog::descriptor(name)
        .is_some_and(|descriptor| descriptor.effect == frank_tool_catalog::ToolEffect::Write)
}

/// Connector calls arrive through both the OpenRouter stream and the
/// task-scoped MCP HTTP route. Keep policy enforcement in the shared daemon
/// execution path so the second transport cannot bypass the role's deny
/// boundary.
pub(crate) fn tool_policy_denial(policy: &AgentPolicy, name: &str) -> Option<String> {
    if name == "terminal_execute" && policy.shell == ShellPolicy::Deny {
        return Some("denied: agent shell policy is deny".into());
    }
    if name == "shell_exec" && policy.network == NetworkPolicy::Deny {
        return Some("denied: agent network policy is deny".into());
    }
    if frank_tool_catalog::descriptor(name).is_some_and(|descriptor| descriptor.requires_network)
        && policy.network == NetworkPolicy::Deny
    {
        return Some("denied: agent network policy is deny".into());
    }
    None
}

pub(crate) fn tool_requires_approval(
    policy: &AgentPolicy,
    name: &str,
    input: &Value,
) -> Option<String> {
    if frank_tool_catalog::descriptor(name).is_some_and(|descriptor| descriptor.requires_approval)
        && name != "shell_exec"
    {
        return Some("connector mutation requires explicit approval".into());
    }
    if name == "shell_exec" {
        if policy.shell == ShellPolicy::Deny {
            return Some("denied: agent shell policy is deny".into());
        }
        if policy.network == NetworkPolicy::Deny {
            return Some("denied: agent network policy is deny".into());
        }
        if policy.shell == ShellPolicy::Ask
            || (policy.network == NetworkPolicy::Ask
                && input
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(command_may_use_network))
        {
            return Some("shell command requires explicit approval".into());
        }
    }
    if name == "workspace_apply_patch" && policy.filesystem == FilesystemPolicy::ReadOnly {
        return Some("denied: agent filesystem policy is read-only".into());
    }
    if tool_is_write(name) && policy.approval == ApprovalPolicy::Ask {
        return Some("tool mutation requires explicit approval".into());
    }
    None
}

fn command_may_use_network(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    [
        "curl ",
        "wget ",
        "http://",
        "https://",
        "git fetch",
        "git pull",
        "npm install",
        "pnpm install",
        "yarn add",
        "pip install",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{
        database::execute_sqlite_database,
        required_string,
        terminal::{execute_shell, read_pipe_capped},
        workspace::confined_path,
    };
    use frank_protocol::{
        AgentPolicy, ApprovalPolicy, FilesystemPolicy, NetworkPolicy, ShellPolicy,
    };
    use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};
    use tokio::io::AsyncWriteExt;

    #[test]
    fn workspace_confinement_rejects_parent_absolute_and_symlink_escape() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("worktree");
        let outside = directory.path().join("outside");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(outside.clone(), "secret\n").unwrap();

        assert!(confined_path(&root, "src/main.rs", false).is_ok());
        assert!(confined_path(&root, "../outside", false).is_err());
        assert!(confined_path(&root, outside.to_str().unwrap(), false).is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
            assert!(confined_path(&root, "escape", false).is_err());
        }
    }

    #[test]
    fn shell_network_and_approval_policy_is_fail_closed() {
        assert!(command_may_use_network("curl https://example.test"));
        assert!(command_may_use_network("git fetch origin"));
        assert!(!command_may_use_network(
            "python -c 'import urllib.request'"
        ));
        assert!(!command_may_use_network("nc example.test 443"));
        assert!(!command_may_use_network("cargo test"));

        let policy = AgentPolicy {
            filesystem: FilesystemPolicy::WorkspaceWrite,
            shell: ShellPolicy::Ask,
            network: NetworkPolicy::Deny,
            approval: ApprovalPolicy::Never,
        };
        assert!(
            tool_requires_approval(
                &policy,
                "shell_exec",
                &json!({"command": "curl https://example.test"})
            )
            .unwrap()
            .starts_with("denied:")
        );
        for command in [
            "curl https://example.test",
            "python -c 'import urllib.request'",
            "nc example.test 443",
            "cargo test",
        ] {
            assert_eq!(
                tool_requires_approval(&policy, "shell_exec", &json!({"command": command})),
                Some("denied: agent network policy is deny".into())
            );
        }
        assert_eq!(
            tool_policy_denial(&policy, "shell_exec"),
            Some("denied: agent network policy is deny".into())
        );
        assert!(tool_policy_denial(&policy, "browser_browse").is_some());
        assert!(tool_policy_denial(&policy, "terminal_execute").is_none());
        let shell_denied = AgentPolicy {
            shell: ShellPolicy::Deny,
            network: NetworkPolicy::Allow,
            ..policy
        };
        assert!(tool_policy_denial(&shell_denied, "terminal_execute").is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_execution_uses_only_the_safe_environment() {
        let directory = tempfile::tempdir().unwrap();
        let result = execute_shell(
            "printf '%s|%s|%s' \"$PATH\" \"$LC_ALL\" \"${HOME-unset}\"",
            directory.path(),
        )
        .await
        .unwrap();

        assert_eq!(result["stdout"], "/usr/bin:/bin:/usr/local/bin|C|unset");
    }

    #[tokio::test]
    async fn shell_output_is_capped_without_blocking_the_reader() {
        let (mut writer, reader) = tokio::io::duplex(8 * 1024);
        let payload = vec![b'x'; MAX_COMMAND_BODY_BYTES + 17];
        let writer_task = tokio::spawn(async move {
            writer.write_all(&payload).await.unwrap();
            writer.shutdown().await.unwrap();
        });
        let (output, truncated) = read_pipe_capped(reader).await.unwrap();
        writer_task.await.unwrap();
        assert_eq!(output.len(), MAX_COMMAND_BODY_BYTES);
        assert!(truncated);
    }

    #[tokio::test]
    async fn sqlite_connector_stays_inside_project_root_and_classifies_reads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("work.sqlite3");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO items (name) VALUES ('first')")
            .execute(&pool)
            .await
            .unwrap();
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot
            .server
            .allowed_project_roots
            .push(directory.path().to_string_lossy().into_owned());
        let profile = ConnectorProfileView {
            id: ConnectorProfileId::new(),
            name: "Temporary SQLite".into(),
            kind: ConnectorKind::Sqlite,
            config: json!({"path": path.to_string_lossy()}),
            health: ConnectorHealth::Healthy,
            configured: true,
            diagnostic: None,
            checked_at: None,
            archived: false,
        };
        let read = execute_sqlite_database(
            &profile,
            &snapshot,
            "database_read",
            "SELECT id, name FROM items",
        )
        .await
        .unwrap();
        assert_eq!(read["rows"][0]["name"], "first");
        assert!(!read["truncated"].as_bool().unwrap());

        for id in 2..=257 {
            sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
                .bind(id)
                .bind(format!("item-{id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;

        let bounded_read = execute_sqlite_database(
            &profile,
            &snapshot,
            "database_read",
            "SELECT id, name FROM items ORDER BY id",
        )
        .await
        .unwrap();
        assert_eq!(bounded_read["rows"].as_array().unwrap().len(), 256);
        assert!(bounded_read["truncated"].as_bool().unwrap());

        assert!(
            execute_sqlite_database(
                &profile,
                &snapshot,
                "database_read",
                "UPDATE items SET name = 'bad'",
            )
            .await
            .is_err()
        );
        execute_sqlite_database(
            &profile,
            &snapshot,
            "database_write",
            "UPDATE items SET name = 'second'",
        )
        .await
        .unwrap();
    }

    #[test]
    fn required_tool_strings_reject_control_and_empty_values() {
        assert!(required_string(&json!({"path": "src/main.rs"}), "path").is_ok());
        assert!(required_string(&json!({"path": ""}), "path").is_err());
        assert!(required_string(&json!({"path": "bad\npath"}), "path").is_err());
    }
}
