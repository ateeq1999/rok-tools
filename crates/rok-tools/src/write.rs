//! `write`: write a file (creating parent directories), reporting a diff.

use crate::Result;
use crate::tool::{Context, Tool, write_file};
use serde::{Deserialize, Serialize};
use similar::TextDiff;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct WriteOptions {
    /// File to write.
    pub path: String,
    /// Content to write.
    pub content: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteOutput {
    /// The written path.
    pub path: String,
    /// Unified diff from the previous content, if it changed.
    pub diff: Option<String>,
}

/// The `write` Tool.
#[derive(Debug)]
pub struct Write;

impl Tool for Write {
    const NAME: &'static str = "write";
    const ABOUT: &'static str = "Write a file, creating parent directories";
    const GROUP: &'static str = "fs";
    type Options = WriteOptions;
    type Output = WriteOutput;

    fn run(o: &WriteOptions, cx: &Context) -> Result<WriteOutput> {
        run(o, cx)
    }

    fn to_text(out: &WriteOutput) -> String {
        out.diff
            .clone()
            .unwrap_or_else(|| format!("{}: unchanged", out.path))
    }
}

/// Writes `o.content` to `o.path`.
pub fn run(o: &WriteOptions, cx: &Context) -> Result<WriteOutput> {
    let target = cx.resolve(&o.path);
    let original = std::fs::read_to_string(&target).unwrap_or_default();
    let diff = (original != o.content).then(|| {
        TextDiff::from_lines(&original, &o.content)
            .unified_diff()
            .header("a", "b")
            .to_string()
    });
    write_file(&target, &o.content)?;
    Ok(WriteOutput {
        path: o.path.clone(),
        diff,
    })
}
