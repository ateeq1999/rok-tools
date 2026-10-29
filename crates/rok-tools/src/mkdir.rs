//! `mkdir`: create a directory and its parents.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct MkdirOptions {
    /// Directory to create.
    pub path: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdirOutput {
    /// The created directory.
    pub path: String,
}

/// The `mkdir` Tool.
#[derive(Debug)]
pub struct Mkdir;

impl Tool for Mkdir {
    const NAME: &'static str = "mkdir";
    const ABOUT: &'static str = "Create a directory and its parents";
    const GROUP: &'static str = "fs";
    type Options = MkdirOptions;
    type Output = MkdirOutput;

    fn run(o: &MkdirOptions, cx: &Context) -> Result<MkdirOutput> {
        run(o, cx)
    }

    fn to_text(out: &MkdirOutput) -> String {
        format!("created {}", out.path)
    }
}

/// Creates `o.path`.
pub fn run(o: &MkdirOptions, cx: &Context) -> Result<MkdirOutput> {
    let target = cx.resolve(&o.path);
    std::fs::create_dir_all(&target).map_err(|e| ToolError::io("create", &target, e))?;
    Ok(MkdirOutput {
        path: o.path.clone(),
    })
}
