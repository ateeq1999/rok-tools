//! `restore`: bring back the files saved by a `snapshot`.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError, archive};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct RestoreOptions {
    /// Name of the Snapshot to restore.
    pub snapshot_id: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOutput {
    /// The restored Snapshot's name.
    pub id: String,
}

/// The `restore` Tool.
#[derive(Debug)]
pub struct Restore;

impl Tool for Restore {
    const NAME: &'static str = "restore";
    const ABOUT: &'static str = "Restore the files saved by a Snapshot";
    const GROUP: &'static str = "fs";
    type Options = RestoreOptions;
    type Output = RestoreOutput;

    fn run(o: &RestoreOptions, cx: &Context) -> Result<RestoreOutput> {
        run(o, cx)
    }

    fn to_text(out: &RestoreOutput) -> String {
        format!("restored snapshot `{}`", out.id)
    }
}

/// Restores the Snapshot `o.snapshot_id` into the working directory.
pub fn run(o: &RestoreOptions, cx: &Context) -> Result<RestoreOutput> {
    let path = archive::snapshot_path(cx, &o.snapshot_id)?;
    if !path.is_file() {
        return Err(ToolError::Failed(format!(
            "snapshot `{}` not found",
            o.snapshot_id
        )));
    }
    archive::extract(cx, &path)?;
    Ok(RestoreOutput {
        id: o.snapshot_id.clone(),
    })
}
