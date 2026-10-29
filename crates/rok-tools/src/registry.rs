//! Name-based access to the Tools compiled into this build.
//!
//! The Runner and the CLIs dispatch through [`registry`] instead of matching
//! on each Tool, so a Tool that is not enabled is simply absent and asking for
//! it yields [`ToolError::NotEnabled`].

use crate::error::{Result, ToolError};
use crate::tool::{Context, Permission, Tool};
use serde_json::Value;

/// The result of running a Tool by name: its output as JSON and whether it succeeded.
#[derive(Debug, Clone)]
pub struct JsonOutcome {
    /// The Tool's output, serialized.
    pub output: Value,
    /// [`Tool::succeeded`] for that output.
    pub succeeded: bool,
    /// [`Tool::to_text`] for that output.
    pub text: String,
}

/// A type-erased handle to one Tool.
#[derive(Clone, Copy)]
pub struct ToolEntry {
    /// [`Tool::NAME`].
    pub name: &'static str,
    /// [`Tool::ABOUT`].
    pub about: &'static str,
    /// [`Tool::GROUP`].
    pub group: &'static str,
    /// [`Tool::PERMISSION`].
    pub permission: Option<Permission>,
    run_json: fn(Value, &Context) -> Result<JsonOutcome>,
    #[cfg(feature = "cli")]
    command: fn() -> clap::Command,
    #[cfg(feature = "cli")]
    from_matches: fn(&clap::ArgMatches) -> Result<Value>,
}

impl std::fmt::Debug for ToolEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolEntry")
            .field("name", &self.name)
            .field("group", &self.group)
            .finish()
    }
}

impl ToolEntry {
    /// Deserializes `options` (a JSON object) and runs the Tool.
    pub fn run_json(&self, options: Value, cx: &Context) -> Result<JsonOutcome> {
        (self.run_json)(options, cx)
    }

    /// The clap command for this Tool's options.
    #[cfg(feature = "cli")]
    pub fn command(&self) -> clap::Command {
        (self.command)()
    }

    /// Parses clap matches for this Tool into its options, as JSON.
    #[cfg(feature = "cli")]
    pub fn options_from_matches(&self, matches: &clap::ArgMatches) -> Result<Value> {
        (self.from_matches)(matches)
    }
}

fn run_json<T: Tool>(options: Value, cx: &Context) -> Result<JsonOutcome> {
    let options: T::Options = serde_json::from_value(options).map_err(ToolError::invalid)?;
    let output = T::run(&options, cx)?;
    Ok(JsonOutcome {
        succeeded: T::succeeded(&output),
        text: T::to_text(&output),
        output: serde_json::to_value(&output).map_err(|e| ToolError::Failed(e.to_string()))?,
    })
}

#[cfg(feature = "cli")]
fn command<T: Tool>() -> clap::Command {
    <T::Options as clap::Args>::augment_args(clap::Command::new(T::NAME))
        .about(T::ABOUT)
        .long_about(None)
}

#[cfg(feature = "cli")]
fn from_matches<T: Tool>(matches: &clap::ArgMatches) -> Result<Value> {
    let options = <T::Options as clap::FromArgMatches>::from_arg_matches(matches)
        .map_err(ToolError::invalid)?;
    serde_json::to_value(options).map_err(ToolError::invalid)
}

#[allow(dead_code)]
fn entry<T: Tool>() -> ToolEntry {
    ToolEntry {
        name: T::NAME,
        about: T::ABOUT,
        group: T::GROUP,
        permission: T::PERMISSION,
        run_json: run_json::<T>,
        #[cfg(feature = "cli")]
        command: command::<T>,
        #[cfg(feature = "cli")]
        from_matches: from_matches::<T>,
    }
}

/// Every Tool compiled into this build, in a stable order.
#[allow(unused_mut, clippy::vec_init_then_push)]
pub fn registry() -> Vec<ToolEntry> {
    let mut tools = Vec::new();
    #[cfg(feature = "read")]
    tools.push(entry::<crate::read::Read>());
    #[cfg(feature = "write")]
    tools.push(entry::<crate::write::Write>());
    #[cfg(feature = "patch")]
    tools.push(entry::<crate::patch::Patch>());
    #[cfg(feature = "mv")]
    tools.push(entry::<crate::mv::Mv>());
    #[cfg(feature = "cp")]
    tools.push(entry::<crate::cp::Cp>());
    #[cfg(feature = "rm")]
    tools.push(entry::<crate::rm::Rm>());
    #[cfg(feature = "mkdir")]
    tools.push(entry::<crate::mkdir::Mkdir>());
    #[cfg(feature = "snapshot")]
    tools.push(entry::<crate::snapshot::Snapshot>());
    #[cfg(feature = "restore")]
    tools.push(entry::<crate::restore::Restore>());
    #[cfg(feature = "grep")]
    tools.push(entry::<crate::grep::Grep>());
    #[cfg(feature = "replace")]
    tools.push(entry::<crate::replace::Replace>());
    #[cfg(feature = "scan")]
    tools.push(entry::<crate::scan::Scan>());
    #[cfg(feature = "summarize")]
    tools.push(entry::<crate::summarize::Summarize>());
    #[cfg(feature = "extract")]
    tools.push(entry::<crate::extract::Extract>());
    #[cfg(feature = "diff")]
    tools.push(entry::<crate::diff::Diff>());
    #[cfg(feature = "lint")]
    tools.push(entry::<crate::lint::Lint>());
    #[cfg(feature = "import")]
    tools.push(entry::<crate::import::Import>());
    #[cfg(feature = "refactor")]
    tools.push(entry::<crate::refactor::Refactor>());
    #[cfg(feature = "deps")]
    tools.push(entry::<crate::deps::Deps>());
    #[cfg(feature = "boilerplate")]
    tools.push(entry::<crate::boilerplate::Boilerplate>());
    #[cfg(feature = "dead-code")]
    tools.push(entry::<crate::dead_code::DeadCode>());
    #[cfg(feature = "loc")]
    tools.push(entry::<crate::loc::Loc>());
    #[cfg(feature = "template")]
    tools.push(entry::<crate::template::Template>());
    #[cfg(feature = "git")]
    tools.push(entry::<crate::git::Git>());
    #[cfg(feature = "bash")]
    tools.push(entry::<crate::bash::Bash>());
    #[cfg(feature = "http")]
    tools.push(entry::<crate::http::Http>());
    tools
}

/// Every Tool name rok-tools knows about, enabled or not.
pub const ALL_TOOLS: &[&str] = &[
    "read",
    "write",
    "patch",
    "mv",
    "cp",
    "rm",
    "mkdir",
    "snapshot",
    "restore",
    "grep",
    "replace",
    "scan",
    "summarize",
    "extract",
    "diff",
    "lint",
    "import",
    "refactor",
    "deps",
    "boilerplate",
    "dead-code",
    "loc",
    "template",
    "git",
    "bash",
    "http",
];

/// `dead_code`, `deadCode` and `dead-code` all name the same Tool.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Looks a Tool up by name (`-`, `_` and case are ignored).
///
/// Returns [`ToolError::NotEnabled`] for a known Tool left out of this build,
/// and [`ToolError::InvalidOptions`] for a name rok-tools does not know.
pub fn find(name: &str) -> Result<ToolEntry> {
    let wanted = normalize(name);
    if let Some(entry) = registry().into_iter().find(|t| normalize(t.name) == wanted) {
        return Ok(entry);
    }
    match ALL_TOOLS.iter().find(|t| normalize(t) == wanted) {
        Some(known) => Err(ToolError::NotEnabled((*known).to_string())),
        None => Err(ToolError::invalid(format!("unknown tool `{name}`"))),
    }
}
