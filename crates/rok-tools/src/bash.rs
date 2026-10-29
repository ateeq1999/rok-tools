//! `bash`: run a shell command (`sh -c` on Unix, `cmd /C` on Windows).
//!
//! Opt-in only: this Tool belongs to no Tool group, and the Runner refuses it
//! unless the Payload sets `allow_shell: true`.

use crate::tool::{Context, Permission, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct BashOptions {
    /// Command line to run.
    pub cmd: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashOutput {
    /// The command that ran.
    pub cmd: String,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code (`-1` when killed by a signal).
    pub exit_code: i32,
}

/// The `bash` Tool.
#[derive(Debug)]
pub struct Bash;

impl Tool for Bash {
    const NAME: &'static str = "bash";
    const ABOUT: &'static str = "Run a shell command";
    const GROUP: &'static str = "opt-in";
    const PERMISSION: Option<Permission> = Some(Permission::Shell);
    type Options = BashOptions;
    type Output = BashOutput;

    fn run(o: &BashOptions, cx: &Context) -> Result<BashOutput> {
        run(o, cx)
    }

    fn succeeded(out: &BashOutput) -> bool {
        out.exit_code == 0
    }

    fn to_text(out: &BashOutput) -> String {
        format!("{}{}", out.stdout, out.stderr)
    }
}

/// Runs `o.cmd` in the working directory with the context's extra environment.
pub fn run(o: &BashOptions, cx: &Context) -> Result<BashOutput> {
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", &o.cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", &o.cmd]);
        c
    };
    let output = command
        .current_dir(&cx.cwd)
        .envs(&cx.env)
        .output()
        .map_err(|e| ToolError::Command {
            command: o.cmd.clone(),
            message: e.to_string(),
        })?;
    Ok(BashOutput {
        cmd: o.cmd.clone(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}
