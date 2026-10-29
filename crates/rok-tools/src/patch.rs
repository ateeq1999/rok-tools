//! `patch`: apply find/replace edits to one file.

use crate::tool::{Context, Tool, read_to_string, write_file};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use similar::TextDiff;

/// One literal edit: every occurrence of `find` becomes `replace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchEdit {
    /// Text to look for.
    pub find: String,
    /// Replacement text.
    pub replace: String,
}

impl std::str::FromStr for PatchEdit {
    type Err = String;

    /// Parses `FIND=>REPLACE`.
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let (find, replace) = s
            .split_once("=>")
            .ok_or_else(|| format!("expected FIND=>REPLACE, got `{s}`"))?;
        Ok(Self {
            find: find.to_string(),
            replace: replace.to_string(),
        })
    }
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct PatchOptions {
    /// File to patch.
    pub path: String,
    /// Edits to apply in order, as `FIND=>REPLACE`.
    #[cfg_attr(feature = "cli", arg(long = "edit", required = true))]
    pub edits: Vec<PatchEdit>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchOutput {
    /// The patched path.
    pub path: String,
    /// How many edits found their text.
    pub edits_applied: usize,
    /// Unified diff of the change.
    pub diff: String,
}

/// The `patch` Tool.
#[derive(Debug)]
pub struct Patch;

impl Tool for Patch {
    const NAME: &'static str = "patch";
    const ABOUT: &'static str = "Apply find/replace edits to a file";
    const GROUP: &'static str = "fs";
    type Options = PatchOptions;
    type Output = PatchOutput;

    fn run(o: &PatchOptions, cx: &Context) -> Result<PatchOutput> {
        run(o, cx)
    }

    fn to_text(out: &PatchOutput) -> String {
        format!(
            "{}: {} edit(s) applied\n{}",
            out.path, out.edits_applied, out.diff
        )
    }
}

/// Applies `o.edits` to `o.path`.
pub fn run(o: &PatchOptions, cx: &Context) -> Result<PatchOutput> {
    if o.edits.is_empty() {
        return Err(ToolError::invalid("patch needs at least one edit"));
    }
    let full = cx.resolve(&o.path);
    let original = read_to_string(&full)?;
    let mut content = original.clone();
    let mut edits_applied = 0;
    for edit in &o.edits {
        if content.contains(&edit.find) {
            content = content.replace(&edit.find, &edit.replace);
            edits_applied += 1;
        }
    }
    if content != original {
        write_file(&full, &content)?;
    }
    let diff = TextDiff::from_lines(&original, &content)
        .unified_diff()
        .header("a", "b")
        .to_string();
    Ok(PatchOutput {
        path: o.path.clone(),
        edits_applied,
        diff,
    })
}
