//! The [`Tool`] trait and the [`Context`] every Tool runs in.

use crate::error::{Result, ToolError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bounds every Tool's options type satisfies: deserializable from a Payload
/// Step, serializable for caching, and (with the `cli` feature) parsable as
/// clap arguments.
#[cfg(feature = "cli")]
pub trait ToolOptions: DeserializeOwned + Serialize + clap::Args {}
#[cfg(feature = "cli")]
impl<T: DeserializeOwned + Serialize + clap::Args> ToolOptions for T {}

/// Bounds every Tool's options type satisfies: deserializable from a Payload
/// Step and serializable for caching.
#[cfg(not(feature = "cli"))]
pub trait ToolOptions: DeserializeOwned + Serialize {}
#[cfg(not(feature = "cli"))]
impl<T: DeserializeOwned + Serialize> ToolOptions for T {}

/// One named capability: typed options in, typed output (or a [`ToolError`]) out.
///
/// Each Tool module also exposes a free `run` function that calls
/// [`Tool::run`], which is the usual way to call a Tool from Rust.
pub trait Tool {
    /// The name used on the command line and as a Step's `type`.
    const NAME: &'static str;
    /// One-line description shown in `--help`.
    const ABOUT: &'static str;
    /// The Tool group this Tool belongs to (`fs`, `search`, `code`, …).
    const GROUP: &'static str;
    /// The Payload permission needed to run this Tool from the Runner, if any.
    const PERMISSION: Option<Permission> = None;

    /// The Tool's options.
    type Options: ToolOptions;
    /// The Tool's output.
    type Output: Serialize;

    /// Runs the Tool.
    fn run(options: &Self::Options, cx: &Context) -> Result<Self::Output>;

    /// Whether an output counts as success (non-zero exit code, failed check…).
    fn succeeded(_output: &Self::Output) -> bool {
        true
    }

    /// Human-readable rendering of an output, used by the CLI without `--json`.
    fn to_text(output: &Self::Output) -> String {
        serde_json::to_string_pretty(output).unwrap_or_default()
    }
}

/// A capability a Payload must grant before the Runner runs a Tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Running arbitrary shell commands (`allow_shell`).
    Shell,
    /// Making network requests (`allow_net`).
    Net,
}

impl Permission {
    /// The Payload option that grants this permission.
    pub fn option_name(self) -> &'static str {
        match self {
            Self::Shell => "allow_shell",
            Self::Net => "allow_net",
        }
    }
}

/// Where a Tool runs: the working directory relative paths resolve against,
/// extra environment variables, and whether directory walks honour ignore files.
#[derive(Debug, Clone)]
pub struct Context {
    /// Directory relative paths are resolved against.
    pub cwd: PathBuf,
    /// Extra environment variables for Tools that spawn processes.
    pub env: BTreeMap<String, String>,
    /// Respect `.gitignore`/`.ignore` and skip `.git`, `target`, `node_modules` when walking.
    pub respect_ignore: bool,
}

impl Context {
    /// A context rooted at `cwd`, which must be an existing directory.
    pub fn new(cwd: impl Into<PathBuf>) -> Result<Self> {
        let cwd = cwd.into();
        if !cwd.is_dir() {
            return Err(ToolError::invalid(format!(
                "working directory is not a directory: {}",
                cwd.display()
            )));
        }
        Ok(Self {
            cwd,
            env: BTreeMap::new(),
            respect_ignore: true,
        })
    }

    /// A context rooted at the process's current directory.
    pub fn current() -> Result<Self> {
        let cwd = std::env::current_dir().map_err(|e| ToolError::io("read", ".", e))?;
        Self::new(cwd)
    }

    /// Resolves `path` against the working directory.
    pub fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        self.cwd.join(path)
    }

    /// `path` relative to the working directory, with `/` separators.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Reads a file to a string, mapping the error to a [`ToolError`].
#[allow(dead_code)]
pub(crate) fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| ToolError::io("read", path, e))
}

/// Writes a file, creating parent directories first.
#[allow(dead_code)]
pub(crate) fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::io("create", parent, e))?;
    }
    std::fs::write(path, content).map_err(|e| ToolError::io("write", path, e))
}

/// Lower-cased extension of `path`, or `""`.
#[allow(dead_code)]
pub(crate) fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Whether `path`'s extension is in `exts` (case-insensitive); an empty list matches everything.
#[allow(dead_code)]
pub(crate) fn has_extension(path: &Path, exts: &[String]) -> bool {
    if exts.is_empty() {
        return true;
    }
    let ext = extension(path);
    exts.iter()
        .any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))
}
