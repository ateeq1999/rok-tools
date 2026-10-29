//! `refactor`: rename a symbol across files, with a per-line change list.

use crate::tool::{Context, Tool, has_extension};
use crate::{Result, ToolError, pattern, walk};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct RefactorOptions {
    /// Symbol to rename.
    pub symbol: String,
    /// New name.
    pub rename_to: String,
    /// File or directory to rewrite.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Only touch files with these extensions (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub ext: Vec<String>,
    /// Report changes without writing.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub dry_run: bool,
    /// Match whole words only.
    #[serde(default = "crate::defaults::yes")]
    #[cfg_attr(feature = "cli", arg(long, default_value_t = true, action = clap::ArgAction::Set))]
    pub whole_word: bool,
}

/// One changed line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorChange {
    /// File, relative to the working directory.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// The line before.
    pub old_text: String,
    /// The line after.
    pub new_text: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorOutput {
    /// The renamed symbol.
    pub symbol: String,
    /// Its new name.
    pub rename_to: String,
    /// Files considered.
    pub files_scanned: usize,
    /// Files containing the symbol.
    pub files_modified: usize,
    /// Total occurrences renamed.
    pub total_replacements: usize,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Every changed line, in path then line order.
    pub changes: Vec<RefactorChange>,
}

/// The `refactor` Tool.
#[derive(Debug)]
pub struct Refactor;

impl Tool for Refactor {
    const NAME: &'static str = "refactor";
    const ABOUT: &'static str = "Rename a symbol across files";
    const GROUP: &'static str = "code";
    type Options = RefactorOptions;
    type Output = RefactorOutput;

    fn run(o: &RefactorOptions, cx: &Context) -> Result<RefactorOutput> {
        run(o, cx)
    }

    fn to_text(out: &RefactorOutput) -> String {
        let mut s: String = out
            .changes
            .iter()
            .map(|c| {
                format!(
                    "{}:{}\n- {}\n+ {}\n",
                    c.path, c.line, c.old_text, c.new_text
                )
            })
            .collect();
        s.push_str(&format!(
            "{}{} → {}: {} occurrence(s) in {} file(s)\n",
            if out.dry_run { "[dry run] " } else { "" },
            out.symbol,
            out.rename_to,
            out.total_replacements,
            out.files_modified
        ));
        s
    }
}

/// Renames `o.symbol` to `o.rename_to` under `o.path`.
pub fn run(o: &RefactorOptions, cx: &Context) -> Result<RefactorOutput> {
    let re = pattern::compile(&o.symbol, false, o.whole_word, false)?;
    let replacement = o.rename_to.replace('$', "$$");
    let files: Vec<_> = walk::files(&cx.resolve(&o.path), cx, None)
        .into_iter()
        .filter(|f| has_extension(f, &o.ext))
        .collect();
    let per_file: Vec<Result<(usize, Vec<RefactorChange>)>> = files
        .par_iter()
        .filter_map(|file| {
            let content = std::fs::read_to_string(file).ok()?;
            let count = re.find_iter(&content).count();
            if count == 0 {
                return None;
            }
            let rel = cx.relative(file);
            let changes = content
                .lines()
                .enumerate()
                .filter(|(_, l)| re.is_match(l))
                .map(|(i, l)| RefactorChange {
                    path: rel.clone(),
                    line: i + 1,
                    old_text: l.to_string(),
                    new_text: re.replace_all(l, replacement.as_str()).into_owned(),
                })
                .collect();
            if !o.dry_run {
                let new = re.replace_all(&content, replacement.as_str());
                if let Err(e) = std::fs::write(file, new.as_ref()) {
                    return Some(Err(ToolError::io("write", file, e)));
                }
            }
            Some(Ok((count, changes)))
        })
        .collect();
    let mut out = RefactorOutput {
        symbol: o.symbol.clone(),
        rename_to: o.rename_to.clone(),
        files_scanned: files.len(),
        files_modified: 0,
        total_replacements: 0,
        dry_run: o.dry_run,
        changes: Vec::new(),
    };
    for result in per_file {
        let (count, changes) = result?;
        out.files_modified += 1;
        out.total_replacements += count;
        out.changes.extend(changes);
    }
    Ok(out)
}
