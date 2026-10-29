//! `cp`: copy a file, or a directory with `recursive`.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct CpOptions {
    /// Source path.
    pub from: String,
    /// Destination path.
    pub to: String,
    /// Copy directories and everything inside them.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'r'))]
    pub recursive: bool,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpOutput {
    /// Source path.
    pub from: String,
    /// Destination path.
    pub to: String,
    /// Number of files copied.
    pub files_copied: usize,
}

/// The `cp` Tool.
#[derive(Debug)]
pub struct Cp;

impl Tool for Cp {
    const NAME: &'static str = "cp";
    const ABOUT: &'static str = "Copy a file or directory";
    const GROUP: &'static str = "fs";
    type Options = CpOptions;
    type Output = CpOutput;

    fn run(o: &CpOptions, cx: &Context) -> Result<CpOutput> {
        run(o, cx)
    }

    fn to_text(out: &CpOutput) -> String {
        format!("{} -> {} ({} file(s))", out.from, out.to, out.files_copied)
    }
}

/// Copies `o.from` to `o.to`.
pub fn run(o: &CpOptions, cx: &Context) -> Result<CpOutput> {
    let from = cx.resolve(&o.from);
    let to = cx.resolve(&o.to);
    let files_copied = if from.is_dir() {
        if !o.recursive {
            return Err(ToolError::invalid(format!(
                "{} is a directory (use recursive)",
                o.from
            )));
        }
        copy_dir(&from, &to)?
    } else if from.is_file() {
        copy_file(&from, &to)?;
        1
    } else {
        return Err(ToolError::NotFound(from));
    };
    Ok(CpOutput {
        from: o.from.clone(),
        to: o.to.clone(),
        files_copied,
    })
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::io("create", parent, e))?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| ToolError::io("copy", from, e))
}

fn copy_dir(from: &Path, to: &Path) -> Result<usize> {
    std::fs::create_dir_all(to).map_err(|e| ToolError::io("create", to, e))?;
    let mut count = 0;
    for entry in std::fs::read_dir(from).map_err(|e| ToolError::io("read", from, e))? {
        let entry = entry.map_err(|e| ToolError::io("read", from, e))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            count += copy_dir(&src, &dst)?;
        } else {
            copy_file(&src, &dst)?;
            count += 1;
        }
    }
    Ok(count)
}
