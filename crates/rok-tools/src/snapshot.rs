//! `snapshot`: save a copy of chosen files that `restore` can bring back.

use crate::Result;
use crate::archive;
use crate::tool::{Context, Tool};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct SnapshotOptions {
    /// File or directory to save.
    pub path: String,
    /// Name of the Snapshot (letters, digits, `-`, `_`, `.`).
    #[cfg_attr(feature = "cli", arg(long = "id"))]
    pub snapshot_id: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotOutput {
    /// The saved path.
    pub path: String,
    /// The Snapshot's name.
    pub id: String,
    /// Number of files stored.
    pub files: usize,
    /// The archive, relative to the working directory.
    pub archive: String,
}

/// The `snapshot` Tool.
#[derive(Debug)]
pub struct Snapshot;

impl Tool for Snapshot {
    const NAME: &'static str = "snapshot";
    const ABOUT: &'static str = "Save a Snapshot of files that `restore` can bring back";
    const GROUP: &'static str = "fs";
    type Options = SnapshotOptions;
    type Output = SnapshotOutput;

    fn run(o: &SnapshotOptions, cx: &Context) -> Result<SnapshotOutput> {
        run(o, cx)
    }

    fn to_text(out: &SnapshotOutput) -> String {
        format!(
            "snapshot `{}`: {} file(s) -> {}",
            out.id, out.files, out.archive
        )
    }
}

/// Saves `o.path` as the Snapshot `o.snapshot_id`, replacing any previous one.
pub fn run(o: &SnapshotOptions, cx: &Context) -> Result<SnapshotOutput> {
    let dest = archive::snapshot_path(cx, &o.snapshot_id)?;
    let files = archive::create(cx, &cx.resolve(&o.path), &dest)?;
    Ok(SnapshotOutput {
        path: o.path.clone(),
        id: o.snapshot_id.clone(),
        files,
        archive: cx.relative(&dest),
    })
}
