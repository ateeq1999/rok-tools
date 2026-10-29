//! `read`: read a file, a directory or a glob of files, optionally filtered.

use crate::tool::{Context, Tool, read_to_string};
use crate::{Result, ToolError, walk};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct ReadOptions {
    /// File, directory, or glob (e.g. `src/**/*.rs`) to read.
    pub path: String,
    /// Truncate each file's content to this many bytes.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub max_bytes: Option<usize>,
    /// Keep only files whose content contains this text.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub filter_imports: Option<String>,
    /// Keep only files that export/define this symbol.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub filter_exports: Option<String>,
    /// Keep only files modified at or after this time (RFC 3339, `YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DD`).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub since: Option<String>,
}

impl ReadOptions {
    /// Reads `path` with no filters.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            max_bytes: None,
            filter_imports: None,
            filter_exports: None,
            since: None,
        }
    }
}

/// One file's content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    /// Path relative to the working directory.
    pub path: String,
    /// The file's (possibly truncated) content.
    pub content: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadOutput {
    /// The requested path.
    pub path: String,
    /// The files that were read and kept.
    pub files: Vec<FileContent>,
    /// How many files the filters dropped.
    pub files_filtered: usize,
}

/// The `read` Tool.
#[derive(Debug)]
pub struct Read;

impl Tool for Read {
    const NAME: &'static str = "read";
    const ABOUT: &'static str = "Read a file, directory or glob of files";
    const GROUP: &'static str = "fs";
    type Options = ReadOptions;
    type Output = ReadOutput;

    fn run(o: &ReadOptions, cx: &Context) -> Result<ReadOutput> {
        run(o, cx)
    }

    fn to_text(out: &ReadOutput) -> String {
        out.files
            .iter()
            .map(|f| format!("==> {} <==\n{}\n", f.path, f.content))
            .collect()
    }
}

/// Reads the files selected by `o`.
pub fn run(o: &ReadOptions, cx: &Context) -> Result<ReadOutput> {
    let since = o.since.as_deref().map(parse_since).transpose()?;
    let mut files = Vec::new();
    let mut files_filtered = 0;
    for path in select(&o.path, cx)? {
        if let Some(since) = since
            && modified_secs(&path).is_some_and(|m| m < since)
        {
            files_filtered += 1;
            continue;
        }
        let Ok(mut content) = std::fs::read_to_string(&path) else {
            continue; // binary or unreadable files are skipped
        };
        let keep_import = o
            .filter_imports
            .as_deref()
            .is_none_or(|f| content.contains(f));
        let keep_export = o
            .filter_exports
            .as_deref()
            .is_none_or(|f| exports(&content, f));
        if !keep_import || !keep_export {
            files_filtered += 1;
            continue;
        }
        if let Some(max) = o.max_bytes {
            truncate_at_char_boundary(&mut content, max);
        }
        files.push(FileContent {
            path: cx.relative(&path),
            content,
        });
    }
    Ok(ReadOutput {
        path: o.path.clone(),
        files,
        files_filtered,
    })
}

fn select(path: &str, cx: &Context) -> Result<Vec<PathBuf>> {
    if path.contains('*') || path.contains('?') {
        let matcher = globset::Glob::new(path)
            .map_err(ToolError::invalid)?
            .compile_matcher();
        return Ok(walk::files(&cx.cwd, cx, None)
            .into_iter()
            .filter(|p| matcher.is_match(cx.relative(p)))
            .collect());
    }
    let full = cx.resolve(path);
    if full.is_dir() {
        Ok(walk::files(&full, cx, None))
    } else {
        read_to_string(&full)?; // surfaces NotFound / permission errors
        Ok(vec![full])
    }
}

fn parse_since(s: &str) -> Result<i64> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime};
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(ndt.and_utc().timestamp());
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc().timestamp())
        .ok_or_else(|| ToolError::invalid(format!("unrecognised `since` time: {s}")))
}

fn modified_secs(path: &std::path::Path) -> Option<i64> {
    let modified = path.metadata().ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn exports(content: &str, symbol: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "export ",
        "export const ",
        "export function ",
        "export class ",
        "export default ",
        "pub fn ",
        "pub const ",
        "pub struct ",
        "pub enum ",
        "def ",
        "func ",
    ];
    PREFIXES
        .iter()
        .any(|p| content.contains(&format!("{p}{symbol}")))
}

fn truncate_at_char_boundary(s: &mut String, max: usize) {
    if s.len() > max {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
}
