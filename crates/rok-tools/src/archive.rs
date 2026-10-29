//! Snapshot archives shared by the `snapshot` and `restore` Tools.
//!
//! A Snapshot is `.rok/snapshots/<id>.tar.gz` under the working directory,
//! holding files with paths relative to that directory.

use crate::tool::Context;
use crate::{Result, ToolError};
use std::path::{Path, PathBuf};

/// Where the Snapshot `id` is stored.
pub(crate) fn snapshot_path(cx: &Context, id: &str) -> Result<PathBuf> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.');
    if !valid {
        return Err(ToolError::invalid(format!(
            "snapshot id `{id}` must be letters, digits, `-`, `_` or `.`"
        )));
    }
    Ok(cx
        .cwd
        .join(".rok")
        .join("snapshots")
        .join(format!("{id}.tar.gz")))
}

/// Archives every file under `source` into `dest`, returning how many were stored.
#[cfg(feature = "snapshot")]
pub(crate) fn create(cx: &Context, source: &Path, dest: &Path) -> Result<usize> {
    if !source.exists() {
        return Err(ToolError::NotFound(source.to_path_buf()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::io("create", parent, e))?;
    }
    let file = std::fs::File::create(dest).map_err(|e| ToolError::io("create", dest, e))?;
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        file,
        flate2::Compression::default(),
    ));
    let mut files = Vec::new();
    collect(source, &mut files)?;
    for path in &files {
        tar.append_path_with_name(path, cx.relative(path))
            .map_err(|e| ToolError::io("archive", path, e))?;
    }
    tar.into_inner()
        .and_then(|enc| enc.finish())
        .map_err(|e| ToolError::io("write", dest, e))?;
    Ok(files.len())
}

/// Unpacks `archive` into the working directory.
#[cfg(feature = "restore")]
pub(crate) fn extract(cx: &Context, archive: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).map_err(|e| ToolError::io("open", archive, e))?;
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(&cx.cwd)
        .map_err(|e| ToolError::io("unpack", archive, e))
}

/// Every file under `path`, skipping `.rok` (Snapshots themselves) and `.git`.
#[cfg(feature = "snapshot")]
fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .map_err(|e| ToolError::io("read", path, e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let name = entry.file_name().unwrap_or_default();
        if entry.is_dir() && (name == ".rok" || name == ".git") {
            continue;
        }
        collect(&entry, out)?;
    }
    Ok(())
}
