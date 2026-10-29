//! Shared directory walking for every Tool that visits many files.
//!
//! With [`Context::respect_ignore`] set (the default), `.gitignore`, `.ignore`
//! and global git excludes are honoured — even outside a git repository — and
//! `.git`, `target` and `node_modules` are always skipped. Hidden files are
//! still visited so dotfiles like `.env` remain searchable.

use crate::tool::Context;
use std::path::{Path, PathBuf};

const ALWAYS_SKIPPED: &[&str] = &[".git", "target", "node_modules"];

/// Every file under `root` (or `root` itself when it is a file), in sorted order.
pub(crate) fn files(root: &Path, cx: &Context, max_depth: Option<usize>) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    let respect = cx.respect_ignore;
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(respect)
        .hidden(false)
        .require_git(false)
        .max_depth(max_depth)
        .sort_by_file_name(|a, b| a.cmp(b));
    if respect {
        builder.filter_entry(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && e.depth() > 0 && ALWAYS_SKIPPED.iter().any(|s| e.file_name() == *s))
        });
    }
    builder
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(ignore::DirEntry::into_path)
        .collect()
}

/// Compiles glob patterns into one matcher; `None` when `patterns` is empty.
#[allow(dead_code)]
pub(crate) fn glob_set(patterns: &[String]) -> crate::Result<Option<globset::GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        builder.add(globset::Glob::new(p).map_err(crate::ToolError::invalid)?);
    }
    builder.build().map(Some).map_err(crate::ToolError::invalid)
}
