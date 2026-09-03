//! Server-owned Git/worktree delivery workflow.
//!
//! Provider sessions are never given a Git write capability.  This module is
//! called only by the daemon after task acceptance and required checks have
//! passed.  All commands run with an explicit cwd and reject the configured
//! default branch as a mutation target.

use std::path::{Path, PathBuf};

use frank_protocol::{MAX_COMMAND_BODY_BYTES, PrPolicy, ProjectView, PushPolicy, TaskId};
use thiserror::Error;
use tokio::process::Command;

use crate::helpers::bounded_text;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git command failed: {command}: {message}")]
    Command { command: String, message: String },
    #[error("project path is not a Git repository")]
    NotRepository,
    #[error("configured path is outside the allowed project roots")]
    OutsideAllowedRoot,
    #[error("refusing to mutate the default branch")]
    DefaultBranchMutation,
    #[error("configured check command is invalid")]
    InvalidCheck,
    #[error("draft PR delivery is unavailable: {0}")]
    DeliveryUnavailable(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

#[derive(Debug, Clone)]
pub struct GitWorkflow {
    pub project: ProjectView,
    pub allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePlan {
    pub branch: String,
    pub path: PathBuf,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub command: String,
    pub success: bool,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryResult {
    pub branch: String,
    pub draft_pr_url: Option<String>,
}

impl GitWorkflow {
    pub fn new(project: ProjectView, allowed_roots: Vec<PathBuf>) -> Result<Self> {
        let path = Path::new(&project.path);
        if !allowed_roots.is_empty()
            && !allowed_roots
                .iter()
                .any(|root| safe_path_within(path, root))
        {
            return Err(GitError::OutsideAllowedRoot);
        }
        if !project.worktree_root.is_empty()
            && !allowed_roots.is_empty()
            && !allowed_roots
                .iter()
                .any(|root| safe_path_within(Path::new(&project.worktree_root), root))
        {
            return Err(GitError::OutsideAllowedRoot);
        }
        Ok(Self {
            project,
            allowed_roots,
        })
    }

    pub async fn validate_repository(&self) -> Result<()> {
        let output = self.git(["rev-parse", "--show-toplevel"]).await?;
        if output.trim().is_empty() {
            Err(GitError::NotRepository)
        } else {
            let repository =
                std::fs::canonicalize(output.trim()).map_err(|_| GitError::NotRepository)?;
            let configured =
                std::fs::canonicalize(&self.project.path).map_err(|_| GitError::NotRepository)?;
            if repository != configured {
                return Err(GitError::NotRepository);
            }
            Ok(())
        }
    }

    /// Return the checked-out branch without ever treating it as a mutation
    /// target. Registration uses this to seed a project with the repository's
    /// actual default branch instead of assuming `main`.
    pub async fn current_branch(&self) -> Result<String> {
        let branch = self.git(["symbolic-ref", "--short", "HEAD"]).await?;
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(GitError::NotRepository);
        }
        Ok(branch.to_string())
    }

    pub fn mission_plan(&self, mission_id: &str, slug: &str) -> WorktreePlan {
        let suffix = mission_id
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .take(8)
            .collect::<String>();
        let slug = sanitize_slug(slug);
        let branch = format!("frank/mission-{slug}-{suffix}");
        let root = self.worktree_root();
        WorktreePlan {
            path: root.join(branch.replace('/', "-")),
            branch,
            base: self.project.base_branch.clone(),
        }
    }

    pub fn task_plan(&self, task_id: TaskId) -> WorktreePlan {
        self.task_plan_from_base(task_id, &self.project.base_branch)
    }

    pub fn task_plan_from_base(&self, task_id: TaskId, base: &str) -> WorktreePlan {
        let branch = format!("frank/task-{task_id}");
        WorktreePlan {
            path: self.worktree_root().join(branch.replace('/', "-")),
            branch,
            base: base.to_string(),
        }
    }

    pub fn branch_plan(&self, branch: &str, base: &str) -> WorktreePlan {
        WorktreePlan {
            path: self.worktree_root().join(branch.replace('/', "-")),
            branch: branch.to_string(),
            base: base.to_string(),
        }
    }

    pub async fn create_worktree(&self, plan: &WorktreePlan) -> Result<()> {
        if plan.branch == self.project.base_branch
            || plan.branch == "main"
            || plan.branch == "master"
        {
            return Err(GitError::DefaultBranchMutation);
        }
        if !safe_worktree_path(&plan.path, &self.allowed_roots) {
            return Err(GitError::OutsideAllowedRoot);
        }
        if plan.branch.is_empty()
            || plan.branch.starts_with('-')
            || plan.branch.chars().any(|character| character.is_control())
        {
            return Err(GitError::Command {
                command: "git worktree add".into(),
                message: "worktree branch name is invalid".into(),
            });
        }
        self.git(["check-ref-format", "--branch", plan.branch.as_str()])
            .await
            .map_err(|_| GitError::Command {
                command: "git worktree add".into(),
                message: "worktree branch name is invalid".into(),
            })?;
        if plan.base.is_empty()
            || plan.base.starts_with('-')
            || plan.base.chars().any(|character| character.is_control())
        {
            return Err(GitError::Command {
                command: "git worktree add".into(),
                message: "worktree base ref is invalid".into(),
            });
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&plan.path)
            && metadata.file_type().is_symlink()
        {
            return Err(GitError::Command {
                command: "git worktree add".into(),
                message: "worktree path may not be a symlink".into(),
            });
        }
        if plan.path.exists() {
            if !plan.path.is_dir() {
                return Err(GitError::Command {
                    command: "git worktree add".into(),
                    message: "worktree path is not a directory".into(),
                });
            }
            let current = self
                .git_at(&plan.path, ["symbolic-ref", "--short", "HEAD"])
                .await
                .map_err(|_| GitError::Command {
                    command: "git worktree add".into(),
                    message: "worktree path already exists and is not the requested branch".into(),
                })?;
            if current.trim() == plan.branch {
                return Ok(());
            }
            return Err(GitError::Command {
                command: "git worktree add".into(),
                message: "worktree path already belongs to another branch".into(),
            });
        }
        if let Some(parent) = plan.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| GitError::Command {
                command: "create worktree root".into(),
                message: error.to_string(),
            })?;
        }
        let path = plan.path.to_string_lossy().to_string();
        // Retrying a command after a daemon crash may leave the branch but no
        // worktree. Reuse that branch instead of attempting `-b` again.
        let existing_branch = self
            .git(["branch", "--list", plan.branch.as_str()])
            .await
            .is_ok_and(|output| !output.trim().is_empty());
        if existing_branch {
            self.git(["worktree", "add", path.as_str(), plan.branch.as_str()])
                .await
                .map(|_| ())
        } else {
            self.git([
                "worktree",
                "add",
                "-b",
                plan.branch.as_str(),
                path.as_str(),
                plan.base.as_str(),
            ])
            .await
            .map(|_| ())
        }
    }

    pub async fn clone_project(
        url: &str,
        destination: &Path,
        allowed_roots: &[PathBuf],
    ) -> Result<()> {
        if url.trim().is_empty()
            || url.len() > 4_096
            || url.chars().any(|character| character.is_control())
            || url.trim_start().starts_with('-')
            || (!allowed_roots.is_empty()
                && !allowed_roots
                    .iter()
                    .any(|root| safe_path_within(destination, root)))
        {
            return Err(GitError::OutsideAllowedRoot);
        }
        if std::fs::symlink_metadata(destination).is_ok() {
            return Err(GitError::Command {
                command: "git clone".into(),
                message: "clone destination already exists".into(),
            });
        }
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| GitError::Command {
            command: "create clone destination parent".into(),
            message: error.to_string(),
        })?;
        let destination = destination.to_string_lossy().into_owned();
        let output = Command::new("git")
            .args(["clone", "--", url, destination.as_str()])
            .current_dir(parent)
            .output()
            .await
            .map_err(|error| GitError::Command {
                command: "git clone".into(),
                message: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(GitError::Command {
                command: "git clone".into(),
                message: bounded_text(&output.stderr),
            });
        }
        Ok(())
    }

    pub async fn capture_diff(&self, worktree: &Path) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .current_dir(worktree)
            .args(["diff", "--binary"])
            .output()
            .await
            .map_err(|error| GitError::Command {
                command: "git diff --binary".into(),
                message: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(GitError::Command {
                command: "git diff --binary".into(),
                message: bounded_text(&output.stderr),
            });
        }
        Ok(output.stdout)
    }

    /// Commit worker changes from a task worktree. Providers never receive a
    /// Git write capability; the daemon creates this commit immediately before
    /// the reviewed squash merge. Returns `false` for an artifact-only task.
    pub async fn commit_worktree(&self, worktree: &Path, message: &str) -> Result<bool> {
        if !safe_worktree_path(worktree, &self.allowed_roots) || !worktree.is_dir() {
            return Err(GitError::OutsideAllowedRoot);
        }
        let branch = self
            .git_at(worktree, ["symbolic-ref", "--short", "HEAD"])
            .await
            .map_err(|_| GitError::DefaultBranchMutation)?;
        let branch = branch.trim();
        if branch == self.project.base_branch || branch == "main" || branch == "master" {
            return Err(GitError::DefaultBranchMutation);
        }
        if message.trim().is_empty() || message.len() > 4_096 {
            return Err(GitError::Command {
                command: "git commit".into(),
                message: "commit message is invalid".into(),
            });
        }
        let status = self
            .git_at(worktree, ["status", "--porcelain", "--untracked-files=all"])
            .await?;
        if status.trim().is_empty() {
            return Ok(false);
        }
        self.git_at(worktree, ["add", "--all"]).await?;
        self.git_at(worktree, ["commit", "-m", message]).await?;
        Ok(true)
    }

    pub async fn run_checks(&self, worktree: &Path) -> Result<Vec<CheckResult>> {
        let mut results = Vec::with_capacity(self.project.check_commands.len());
        for command in &self.project.check_commands {
            if command.contains('\n') || command.len() > 4096 {
                return Err(GitError::InvalidCheck);
            }
            let output = if cfg!(windows) {
                Command::new("cmd")
                    .arg("/C")
                    .arg(command)
                    .current_dir(worktree)
                    .output()
                    .await
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .current_dir(worktree)
                    .output()
                    .await
            }
            .map_err(|error| GitError::Command {
                command: command.clone(),
                message: error.to_string(),
            })?;
            let text = bounded_command_output(&output.stdout, &output.stderr);
            results.push(CheckResult {
                command: command.clone(),
                success: output.status.success(),
                output: text,
            });
            if !output.status.success() {
                break;
            }
        }
        Ok(results)
    }

    pub async fn squash_merge(
        &self,
        task_branch: &str,
        mission_branch: &str,
        message: &str,
    ) -> Result<()> {
        if message.trim().is_empty()
            || message.len() > 4_096
            || message.chars().any(|character| character == '\0')
        {
            return Err(GitError::Command {
                command: "git merge --squash".into(),
                message: "merge commit message is invalid".into(),
            });
        }
        if mission_branch == self.project.base_branch
            || mission_branch == "main"
            || mission_branch == "master"
            || task_branch == self.project.base_branch
            || task_branch == "main"
            || task_branch == "master"
            || mission_branch.starts_with('-')
            || task_branch.starts_with('-')
            || mission_branch
                .chars()
                .any(|character| character.is_control())
            || task_branch.chars().any(|character| character.is_control())
        {
            return Err(GitError::DefaultBranchMutation);
        }
        self.git(["check-ref-format", "--branch", mission_branch])
            .await
            .map_err(|_| GitError::Command {
                command: "git merge --squash".into(),
                message: "mission branch name is invalid".into(),
            })?;
        self.git(["check-ref-format", "--branch", task_branch])
            .await
            .map_err(|_| GitError::Command {
                command: "git merge --squash".into(),
                message: "task branch name is invalid".into(),
            })?;
        let mission_path = self.worktree_root().join(mission_branch.replace('/', "-"));
        if !safe_worktree_path(&mission_path, &self.allowed_roots) || !mission_path.is_dir() {
            return Err(GitError::OutsideAllowedRoot);
        }
        let checked_out = self
            .git_at(&mission_path, ["symbolic-ref", "--short", "HEAD"])
            .await
            .map_err(|_| GitError::Command {
                command: "git merge --squash".into(),
                message: "mission worktree is not a checked-out branch".into(),
            })?;
        if checked_out.trim() != mission_branch {
            return Err(GitError::Command {
                command: "git merge --squash".into(),
                message: "mission worktree is on an unexpected branch".into(),
            });
        }
        // A daemon can crash after the squash commit but before the durable
        // task transition. A squash commit deliberately does *not* make the
        // task tip an ancestor of the mission branch, so merge-base alone
        // cannot detect this retry window. First look for the stable commit
        // subject in the mission history, then fall back to the tree identity
        // for artifact-only/empty task branches. Both checks are read-only and
        // make a retry a no-op instead of creating a duplicate integration
        // commit.
        let history = self
            .git_at(
                &mission_path,
                [
                    "log",
                    "--format=%s",
                    "--fixed-strings",
                    "--grep",
                    message,
                    mission_branch,
                ],
            )
            .await
            .unwrap_or_default();
        if history.lines().any(|line| line.trim() == message.trim()) {
            return Ok(());
        }
        let task_tree = self
            .git_at(
                &mission_path,
                ["rev-parse", &format!("{task_branch}^{{tree}}")],
            )
            .await
            .ok();
        let mission_tree = self
            .git_at(
                &mission_path,
                ["rev-parse", &format!("{mission_branch}^{{tree}}")],
            )
            .await
            .ok();
        if task_tree.is_some() && task_tree == mission_tree {
            return Ok(());
        }
        self.git_at(&mission_path, ["merge", "--squash", task_branch])
            .await?;
        self.git_at(&mission_path, ["commit", "-m", message])
            .await
            .map(|_| ())
    }

    /// Push a mission branch without opening a pull request.  Push and PR
    /// creation are deliberately separate operations in the daemon journal so
    /// a crash after the push cannot cause a second PR mutation on retry.
    pub async fn push_mission(&self, mission_branch: &str) -> Result<()> {
        if self.project.push_policy == PushPolicy::Disabled {
            return Err(GitError::DeliveryUnavailable(
                "push policy is disabled".into(),
            ));
        }
        self.validate_delivery_branch(mission_branch).await?;
        let remote = self
            .project
            .remote
            .as_deref()
            .filter(|remote| !remote.trim().is_empty())
            .unwrap_or("origin");
        if remote.starts_with('-') || remote.chars().any(|character| character.is_control()) {
            return Err(GitError::DeliveryUnavailable(
                "configured Git remote name is invalid".into(),
            ));
        }
        self.git(["push", "--set-upstream", remote, mission_branch])
            .await?;
        Ok(())
    }

    /// Open (or discover) the configured draft PR for a mission branch.  The
    /// `gh pr list` lookup makes this operation idempotent when a daemon dies
    /// after GitHub accepted `gh pr create` but before Frank persisted the
    /// completion event.
    pub async fn open_draft_pr(&self, mission_branch: &str) -> Result<Option<String>> {
        self.validate_delivery_branch(mission_branch).await?;
        if self.project.pr_policy != PrPolicy::Draft {
            return Ok(None);
        }
        let existing = Command::new("gh")
            .current_dir(&self.project.path)
            .args([
                "pr",
                "list",
                "--state",
                "all",
                "--head",
                mission_branch,
                "--json",
                "url",
                "--jq",
                ".[0].url",
            ])
            .output()
            .await
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|url| !url.is_empty());
        if existing.is_some() {
            return Ok(existing);
        }
        let output = Command::new("gh")
            .current_dir(&self.project.path)
            .args([
                "pr",
                "create",
                "--draft",
                "--base",
                &self.project.base_branch,
                "--head",
                mission_branch,
            ])
            .output()
            .await
            .map_err(|error| GitError::DeliveryUnavailable(error.to_string()))?;
        if !output.status.success() {
            return Err(GitError::DeliveryUnavailable(bounded_text(&output.stderr)));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .last()
            .map(str::to_string))
    }

    /// Backwards-compatible convenience API for callers that do not need the
    /// durable operation split.  The daemon uses `push_mission` followed by
    /// `open_draft_pr` instead.
    pub async fn deliver(&self, mission_branch: &str) -> Result<DeliveryResult> {
        self.push_mission(mission_branch).await?;
        let draft_pr_url = self.open_draft_pr(mission_branch).await?;
        Ok(DeliveryResult {
            branch: mission_branch.to_string(),
            draft_pr_url,
        })
    }

    async fn validate_delivery_branch(&self, mission_branch: &str) -> Result<()> {
        if mission_branch == self.project.base_branch
            || mission_branch == "main"
            || mission_branch == "master"
        {
            return Err(GitError::DefaultBranchMutation);
        }
        if mission_branch.starts_with('-')
            || mission_branch
                .chars()
                .any(|character| character.is_control())
            || self
                .git(["check-ref-format", "--branch", mission_branch])
                .await
                .is_err()
        {
            return Err(GitError::DeliveryUnavailable(
                "configured mission branch name is invalid".into(),
            ));
        }
        Ok(())
    }

    fn worktree_root(&self) -> PathBuf {
        if self.project.worktree_root.is_empty() {
            Path::new(&self.project.path).join(".frank-worktrees")
        } else {
            PathBuf::from(&self.project.worktree_root)
        }
    }

    async fn git<const N: usize>(&self, args: [&str; N]) -> Result<String> {
        self.git_at(Path::new(&self.project.path), args).await
    }

    async fn git_at<const N: usize>(&self, cwd: &Path, args: [&str; N]) -> Result<String> {
        let command = format!("git {}", args.join(" "));
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .await
            .map_err(|error| GitError::Command {
                command: command.clone(),
                message: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(GitError::Command {
                command,
                message: bounded_text(&output.stderr),
            });
        }
        Ok(bounded_text(&output.stdout))
    }
}

/// Keep provider/check/Git diagnostics within the same body cap as command
/// responses.  Git can emit an arbitrarily large diff or hook traceback; a
/// daemon must not persist that entire buffer in an operation/error row.
fn bounded_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut bytes = Vec::with_capacity((stdout.len() + stderr.len()).min(MAX_COMMAND_BODY_BYTES));
    let stdout_take = stdout.len().min(MAX_COMMAND_BODY_BYTES);
    bytes.extend_from_slice(&stdout[..stdout_take]);
    if bytes.len() < MAX_COMMAND_BODY_BYTES {
        let remaining = MAX_COMMAND_BODY_BYTES - bytes.len();
        bytes.extend_from_slice(&stderr[..stderr.len().min(remaining)]);
    }
    let truncated = stdout.len().saturating_add(stderr.len()) > bytes.len();
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[output truncated]");
    }
    text
}

fn sanitize_slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars().take(48) {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "mission".into()
    } else {
        slug
    }
}

fn safe_worktree_path(path: &Path, roots: &[PathBuf]) -> bool {
    if path.exists() {
        return roots.is_empty() || roots.iter().any(|root| safe_path_within(path, root));
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    roots.is_empty() || roots.iter().any(|root| safe_path_within(parent, root))
}

fn safe_path_within(path: &Path, root: &Path) -> bool {
    let Ok(root) = canonicalize_allow_missing(root) else {
        return false;
    };
    canonicalize_allow_missing(path).is_ok_and(|candidate| candidate.starts_with(root))
}

/// Canonicalize a path while allowing the final (not-yet-created) components.
/// Every existing component is resolved first, so a symlink cannot smuggle a
/// clone/worktree outside an allowed root between validation and creation.
fn canonicalize_allow_missing(path: &Path) -> std::io::Result<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    while std::fs::symlink_metadata(current).is_err() {
        let Some(name) = current.file_name() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path has no existing ancestor",
            ));
        };
        missing.push(name.to_os_string());
        current = current.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent")
        })?;
    }
    let mut resolved = std::fs::canonicalize(current)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_protocol::{PrPolicy, ProjectId, PushPolicy};

    fn project() -> ProjectView {
        ProjectView {
            id: ProjectId::nil(),
            name: "demo".into(),
            path: "/tmp/demo".into(),
            base_branch: "main".into(),
            remote: None,
            check_commands: vec![],
            worktree_root: String::new(),
            push_policy: PushPolicy::MissionBranch,
            pr_policy: PrPolicy::Draft,
            archived: false,
        }
    }

    #[test]
    fn branch_names_are_never_the_default_branch() {
        let workflow = GitWorkflow::new(project(), vec![]).unwrap();
        let mission = workflow.mission_plan("12345678-xxxx", "Ship feature");
        assert_eq!(mission.branch, "frank/mission-ship-feature-12345678");
        let task = workflow.task_plan(TaskId::nil());
        assert!(task.branch.starts_with("frank/task-"));
    }

    #[test]
    fn allowed_root_is_enforced_before_git_runs() {
        assert!(matches!(
            GitWorkflow::new(project(), vec![PathBuf::from("/safe")]),
            Err(GitError::OutsideAllowedRoot)
        ));
    }
}
