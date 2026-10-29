//! `replace`: replace a pattern across many files.

use crate::tool::{Context, Tool, has_extension};
use crate::{Result, pattern, walk};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct ReplaceOptions {
    /// Text to replace (a regular expression with `--regex`).
    pub pattern: String,
    /// Replacement text (`$1`, `${name}` refer to regex groups).
    pub replacement: String,
    /// File or directory to rewrite.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Only touch files with these extensions (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub ext: Vec<String>,
    /// Only touch files whose path (relative to the working directory) matches this glob.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub glob: Option<String>,
    /// Treat the pattern as a regular expression.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'e'))]
    pub regex: bool,
    /// Match case-insensitively.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'i'))]
    pub ignore_case: bool,
    /// Match whole words only.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'w'))]
    pub whole_word: bool,
    /// Report what would change without writing.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub dry_run: bool,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceOutput {
    /// The pattern replaced.
    pub pattern: String,
    /// The replacement used.
    pub replacement: String,
    /// Files considered.
    pub files_scanned: usize,
    /// Files rewritten (or that would be, with `dry_run`).
    pub files_modified: usize,
    /// Total occurrences replaced.
    pub total_replacements: usize,
    /// Whether this was a dry run.
    pub dry_run: bool,
}

/// The `replace` Tool.
#[derive(Debug)]
pub struct Replace;

impl Tool for Replace {
    const NAME: &'static str = "replace";
    const ABOUT: &'static str = "Replace a pattern across files";
    const GROUP: &'static str = "search";
    type Options = ReplaceOptions;
    type Output = ReplaceOutput;

    fn run(o: &ReplaceOptions, cx: &Context) -> Result<ReplaceOutput> {
        run(o, cx)
    }

    fn to_text(out: &ReplaceOutput) -> String {
        format!(
            "{}{} replacement(s) in {} of {} file(s)",
            if out.dry_run { "[dry run] " } else { "" },
            out.total_replacements,
            out.files_modified,
            out.files_scanned
        )
    }
}

/// Replaces `o.pattern` with `o.replacement` under `o.path`.
pub fn run(o: &ReplaceOptions, cx: &Context) -> Result<ReplaceOutput> {
    let re = pattern::compile(&o.pattern, o.regex, o.whole_word, o.ignore_case)?;
    let glob = o
        .glob
        .as_deref()
        .map(|g| walk::glob_set(&[g.to_string()]))
        .transpose()?
        .flatten();
    let files: Vec<_> = walk::files(&cx.resolve(&o.path), cx, None)
        .into_iter()
        .filter(|f| has_extension(f, &o.ext))
        .filter(|f| glob.as_ref().is_none_or(|g| g.is_match(cx.relative(f))))
        .collect();
    let replaced = AtomicUsize::new(0);
    let modified = AtomicUsize::new(0);
    let replacement = if o.regex {
        o.replacement.clone()
    } else {
        o.replacement.replace('$', "$$")
    };
    let errors: Vec<_> = files
        .par_iter()
        .filter_map(|file| {
            let content = std::fs::read_to_string(file).ok()?;
            let count = re.find_iter(&content).count();
            if count == 0 {
                return None;
            }
            let new = re.replace_all(&content, replacement.as_str());
            if new == content {
                return None;
            }
            if !o.dry_run
                && let Err(e) = std::fs::write(file, new.as_ref())
            {
                return Some(crate::ToolError::io("write", file, e));
            }
            replaced.fetch_add(count, Ordering::Relaxed);
            modified.fetch_add(1, Ordering::Relaxed);
            None
        })
        .collect();
    if let Some(e) = errors.into_iter().next() {
        return Err(e);
    }
    Ok(ReplaceOutput {
        pattern: o.pattern.clone(),
        replacement: o.replacement.clone(),
        files_scanned: files.len(),
        files_modified: modified.into_inner(),
        total_replacements: replaced.into_inner(),
        dry_run: o.dry_run,
    })
}
