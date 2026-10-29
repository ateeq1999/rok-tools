//! What the Runner keeps on disk under `.rok/`: the Step cache, incremental
//! state, run history and Checkpoints.

use super::payload::{Payload, RunOutput, Step, StepResult};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const HISTORY_LIMIT: usize = 100;

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| ToolError::Failed(e.to_string()))?;
    crate::tool::write_file(path, &text)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Successful Step results keyed by a hash of the Step and its position.
#[derive(Debug)]
pub(crate) struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn key(step: &Step, index: usize) -> String {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut h = DefaultHasher::new();
        serde_json::to_string(step).unwrap_or_default().hash(&mut h);
        index.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    pub(crate) fn get(&self, step: &Step, index: usize) -> Option<StepResult> {
        read_json(&self.dir.join(format!("{}.json", Self::key(step, index))))
    }

    pub(crate) fn put(&self, step: &Step, index: usize, result: &StepResult) {
        if result.is_ok() {
            let _ = write_json(
                &self.dir.join(format!("{}.json", Self::key(step, index))),
                result,
            );
        }
    }
}

/// Modification times of files seen by file-reading Steps on the previous run.
#[derive(Debug)]
pub(crate) struct Incremental {
    file: PathBuf,
    saved: BTreeMap<String, u64>,
    current: BTreeMap<String, u64>,
}

/// Tools whose result depends only on the files under their `path`.
pub(crate) const INCREMENTAL_TOOLS: &[&str] = &[
    "read",
    "grep",
    "scan",
    "summarize",
    "deps",
    "dead-code",
    "deadcode",
    "dead_code",
    "loc",
];

impl Incremental {
    pub(crate) fn load(cwd: &Path) -> Self {
        let file = cwd.join(".rok").join("incremental_state.json");
        let saved = read_json(&file).unwrap_or_default();
        Self {
            file,
            saved,
            current: BTreeMap::new(),
        }
    }

    /// Whether any file under `path` changed (or appeared) since the previous run.
    pub(crate) fn changed(&mut self, cwd: &Path, path: &str) -> bool {
        let mut files = Vec::new();
        collect(&cwd.join(path), &mut files);
        let mut changed = files.is_empty();
        for f in files {
            let key = f
                .strip_prefix(cwd)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let mtime = f
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            changed |= self.saved.get(&key) != Some(&mtime);
            self.current.insert(key, mtime);
        }
        changed
    }

    pub(crate) fn save(&self) {
        let mut merged = self.saved.clone();
        merged.extend(self.current.clone());
        let _ = write_json(&self.file, &merged);
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name();
        if p.is_dir()
            && [".git", ".rok", "target", "node_modules"]
                .iter()
                .any(|s| name == *s)
        {
            continue;
        }
        collect(&p, out);
    }
}

/// One entry of `.rok/history.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// `YYYYMMDD-HHMMSS` of the run.
    pub run_id: String,
    /// RFC 3339 timestamp.
    pub timestamp: String,
    /// The Payload's name.
    pub name: Option<String>,
    /// `ok` or `partial`.
    pub status: String,
    /// Steps that succeeded.
    pub steps_ok: usize,
    /// Steps that failed.
    pub steps_failed: usize,
    /// Wall-clock time.
    pub duration_ms: u64,
    /// The Payload that ran, so it can be replayed.
    pub payload: Payload,
}

/// Prepends a run to `.rok/history.json`, keeping the latest 100.
pub(crate) fn record_history(cwd: &Path, payload: &Payload, output: &RunOutput) {
    let file = cwd.join(".rok").join("history.json");
    let mut history: Vec<HistoryEntry> = read_json(&file).unwrap_or_default();
    let now = chrono::Utc::now();
    history.insert(
        0,
        HistoryEntry {
            run_id: now.format("%Y%m%d-%H%M%S").to_string(),
            timestamp: now.to_rfc3339(),
            name: payload.name.clone(),
            status: output.status.clone(),
            steps_ok: output.steps_ok,
            steps_failed: output.steps_failed,
            duration_ms: output.duration_ms,
            payload: payload.clone(),
        },
    );
    history.truncate(HISTORY_LIMIT);
    let _ = write_json(&file, &history);
}

/// Reads `.rok/history.json` (newest first).
pub fn history(cwd: &Path) -> Vec<HistoryEntry> {
    read_json(&cwd.join(".rok").join("history.json")).unwrap_or_default()
}

/// The Runner's saved progress through a Payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The Checkpoint's name.
    pub checkpoint_id: String,
    /// RFC 3339 time it was saved.
    pub created_at: String,
    /// Results of the top-level Steps that had succeeded, by index.
    pub completed: BTreeMap<usize, StepResult>,
}

fn checkpoint_path(cwd: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(ToolError::invalid(format!(
            "checkpoint id `{id}` must be letters, digits, `-` or `_`"
        )));
    }
    Ok(cwd
        .join(".rok")
        .join("checkpoints")
        .join(format!("{id}.json")))
}

/// Saves the results that have succeeded so far as Checkpoint `id`.
pub(crate) fn save_checkpoint(
    cwd: &Path,
    id: &str,
    results: &[Option<StepResult>],
) -> Result<Checkpoint> {
    let checkpoint = Checkpoint {
        checkpoint_id: id.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        completed: results
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.as_ref().filter(|r| r.is_ok()).map(|r| (i, r.clone())))
            .collect(),
    };
    write_json(&checkpoint_path(cwd, id)?, &checkpoint)?;
    Ok(checkpoint)
}

/// Loads Checkpoint `id`.
pub fn load_checkpoint(cwd: &Path, id: &str) -> Result<Checkpoint> {
    let path = checkpoint_path(cwd, id)?;
    read_json(&path).ok_or_else(|| ToolError::Failed(format!("checkpoint `{id}` not found")))
}
