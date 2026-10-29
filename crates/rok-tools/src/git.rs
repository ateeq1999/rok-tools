//! `git`: run a git operation and return structured output.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// The git operation to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum GitOp {
    /// `git status --porcelain`.
    Status,
    /// `git diff`.
    Diff,
    /// `git log --oneline`.
    Log,
    /// `git add`.
    Add,
    /// `git commit`.
    Commit,
    /// `git branch`.
    Branch,
}

impl GitOp {
    fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Diff => "diff",
            Self::Log => "log",
            Self::Add => "add",
            Self::Commit => "commit",
            Self::Branch => "branch",
        }
    }
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct GitOptions {
    /// Operation to run.
    #[cfg_attr(feature = "cli", arg(value_enum))]
    pub op: GitOp,
    /// Extra arguments passed to git after the operation.
    #[serde(default)]
    #[cfg_attr(
        feature = "cli",
        arg(trailing_var_arg = true, allow_hyphen_values = true)
    )]
    pub args: Vec<String>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOutput {
    /// The operation run.
    pub op: GitOp,
    /// Raw standard output.
    pub stdout: String,
    /// Output lines (status entries, branches, commits…).
    pub lines: Vec<String>,
    /// For `status`: whether the working tree is clean.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean: Option<bool>,
}

/// The `git` Tool.
#[derive(Debug)]
pub struct Git;

impl Tool for Git {
    const NAME: &'static str = "git";
    const ABOUT: &'static str = "Run a git operation with structured output";
    const GROUP: &'static str = "vcs";
    type Options = GitOptions;
    type Output = GitOutput;

    fn run(o: &GitOptions, cx: &Context) -> Result<GitOutput> {
        run(o, cx)
    }

    fn to_text(out: &GitOutput) -> String {
        match out.clean {
            Some(true) => "working tree clean\n".to_string(),
            _ => out.stdout.clone(),
        }
    }
}

/// Runs `git <op> <args…>` in the working directory; a non-zero exit is an error.
pub fn run(o: &GitOptions, cx: &Context) -> Result<GitOutput> {
    let mut args: Vec<&str> = vec![o.op.as_str()];
    match o.op {
        GitOp::Status => args.push("--porcelain"),
        GitOp::Log if o.args.is_empty() => args.extend(["--oneline", "-n", "20"]),
        _ => {}
    }
    args.extend(o.args.iter().map(String::as_str));
    let output = Command::new("git")
        .args(&args)
        .current_dir(&cx.cwd)
        .envs(&cx.env)
        .output()
        .map_err(|e| ToolError::Command {
            command: "git".into(),
            message: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(ToolError::Command {
            command: format!("git {}", args.join(" ")),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let lines: Vec<String> = stdout.lines().map(String::from).collect();
    let clean = (o.op == GitOp::Status).then_some(lines.is_empty());
    Ok(GitOutput {
        op: o.op,
        stdout,
        lines,
        clean,
    })
}
