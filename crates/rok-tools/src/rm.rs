//! `rm`: remove a file, or a directory with `recursive`.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct RmOptions {
    /// Path to remove.
    pub path: String,
    /// Remove a directory and everything inside it.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'r'))]
    pub recursive: bool,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RmOutput {
    /// The removed path.
    pub path: String,
}

/// The `rm` Tool.
#[derive(Debug)]
pub struct Rm;

impl Tool for Rm {
    const NAME: &'static str = "rm";
    const ABOUT: &'static str = "Remove a file or directory";
    const GROUP: &'static str = "fs";
    type Options = RmOptions;
    type Output = RmOutput;

    fn run(o: &RmOptions, cx: &Context) -> Result<RmOutput> {
        run(o, cx)
    }

    fn to_text(out: &RmOutput) -> String {
        format!("removed {}", out.path)
    }
}

/// Removes `o.path`.
pub fn run(o: &RmOptions, cx: &Context) -> Result<RmOutput> {
    let target = cx.resolve(&o.path);
    let result = if target.is_dir() {
        if !o.recursive {
            return Err(ToolError::invalid(format!(
                "{} is a directory (use recursive)",
                o.path
            )));
        }
        std::fs::remove_dir_all(&target)
    } else {
        std::fs::remove_file(&target)
    };
    result.map_err(|e| ToolError::io("remove", &target, e))?;
    Ok(RmOutput {
        path: o.path.clone(),
    })
}
