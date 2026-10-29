//! `diff`: compare two files.

use crate::Result;
use crate::tool::{Context, Tool, read_to_string};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// Whether [`run`] includes a unified diff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum DiffFormat {
    /// Counts only.
    #[default]
    Stat,
    /// Counts plus a unified diff.
    Unified,
    /// Same as `stat`; kept for Payload compatibility.
    Json,
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct DiffOptions {
    /// The original file.
    pub a: String,
    /// The changed file.
    pub b: String,
    /// Output detail.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t = DiffFormat::Unified))]
    pub format: DiffFormat,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    /// The original file.
    pub a: String,
    /// The changed file.
    pub b: String,
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
    /// Starting line (in `a`) of each changed hunk.
    pub changed_sections: Vec<String>,
    /// Whether the files are identical.
    pub is_identical: bool,
    /// Unified diff (`unified` format).
    pub unified_diff: Option<String>,
}

/// The `diff` Tool.
#[derive(Debug)]
pub struct Diff;

impl Tool for Diff {
    const NAME: &'static str = "diff";
    const ABOUT: &'static str = "Compare two files";
    const GROUP: &'static str = "code";
    type Options = DiffOptions;
    type Output = DiffOutput;

    fn run(o: &DiffOptions, cx: &Context) -> Result<DiffOutput> {
        run(o, cx)
    }

    fn to_text(out: &DiffOutput) -> String {
        match &out.unified_diff {
            Some(d) if !out.is_identical => d.clone(),
            _ if out.is_identical => "identical\n".to_string(),
            _ => format!("+{} -{}\n", out.added, out.removed),
        }
    }
}

/// Compares `o.a` with `o.b`.
pub fn run(o: &DiffOptions, cx: &Context) -> Result<DiffOutput> {
    let a = read_to_string(&cx.resolve(&o.a))?;
    let b = read_to_string(&cx.resolve(&o.b))?;
    let diff = TextDiff::from_lines(&a, &b);
    let (mut added, mut removed) = (0, 0);
    let mut changed_sections = Vec::new();
    for group in diff.grouped_ops(3) {
        if let Some(first) = group.first() {
            changed_sections.push(format!("line {}", first.old_range().start + 1));
        }
        for op in &group {
            for change in diff.iter_changes(op) {
                match change.tag() {
                    ChangeTag::Insert => added += 1,
                    ChangeTag::Delete => removed += 1,
                    ChangeTag::Equal => {}
                }
            }
        }
    }
    let unified_diff = (o.format == DiffFormat::Unified)
        .then(|| diff.unified_diff().header(&o.a, &o.b).to_string());
    Ok(DiffOutput {
        a: o.a.clone(),
        b: o.b.clone(),
        added,
        removed,
        changed_sections,
        is_identical: added == 0 && removed == 0,
        unified_diff,
    })
}
