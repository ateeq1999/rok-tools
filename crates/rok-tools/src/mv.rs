//! `mv`: move or rename a file or directory.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct MvOptions {
    /// Source path.
    pub from: String,
    /// Destination path; an existing directory receives the source inside it.
    pub to: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvOutput {
    /// Source path.
    pub from: String,
    /// Final destination, relative to the working directory.
    pub to: String,
}

/// The `mv` Tool.
#[derive(Debug)]
pub struct Mv;

impl Tool for Mv {
    const NAME: &'static str = "mv";
    const ABOUT: &'static str = "Move or rename a file or directory";
    const GROUP: &'static str = "fs";
    type Options = MvOptions;
    type Output = MvOutput;

    fn run(o: &MvOptions, cx: &Context) -> Result<MvOutput> {
        run(o, cx)
    }

    fn to_text(out: &MvOutput) -> String {
        format!("{} -> {}", out.from, out.to)
    }
}

/// Moves `o.from` to `o.to`.
pub fn run(o: &MvOptions, cx: &Context) -> Result<MvOutput> {
    let from = cx.resolve(&o.from);
    if !from.exists() {
        return Err(ToolError::NotFound(from));
    }
    let mut to = cx.resolve(&o.to);
    if to.is_dir() {
        to = to.join(from.file_name().unwrap_or_default());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::io("create", parent, e))?;
    }
    std::fs::rename(&from, &to).map_err(|e| ToolError::io("move", &from, e))?;
    Ok(MvOutput {
        from: o.from.clone(),
        to: cx.relative(&to),
    })
}
