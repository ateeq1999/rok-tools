//! Feature-gated developer tools as plain Rust functions.
//!
//! Every Tool lives in its own module behind a cargo feature of the same name
//! and exposes typed options, a typed output and a `run` function:
//!
//! ```toml
//! [dependencies]
//! rok-tools = { version = "0.1", features = ["grep", "loc"] }
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "grep")] {
//! use rok_tools::{Context, grep};
//!
//! let cx = Context::new(".")?;
//! let found = grep::run(&grep::GrepOptions::new("TODO", "src"), &cx)?;
//! for m in &found.matches {
//!     println!("{}:{}: {}", m.path, m.line, m.text);
//! }
//! # }
//! # Ok::<(), rok_tools::ToolError>(())
//! ```
//!
//! Features come in three layers:
//!
//! - one per Tool (`grep`, `replace`, `loc`, …);
//! - Tool groups that enable several at once: `fs`, `search`, `code`,
//!   `template`, `vcs`, and `full` for everything. `bash` and `http` belong
//!   to no group — they are only in `full` or when named explicitly;
//! - `runner` (the JSON Payload Runner), `config` (`[tools.*]` sections of
//!   `rok.toml`) and `cli` (clap derives for `rok-tools-cli`).
//!
//! The default feature set is empty. The library never reads config files on
//! its own; enable the `config` feature when you want it to.

mod error;
mod registry;
mod tool;

#[cfg(any(feature = "template", feature = "http"))]
pub mod kv;
#[cfg(any(feature = "scan", feature = "summarize"))]
mod lang;
#[cfg(any(feature = "grep", feature = "replace", feature = "refactor"))]
mod pattern;
#[cfg(feature = "_walk")]
mod walk;

pub use error::{Result, ToolError};
pub use registry::{ALL_TOOLS, JsonOutcome, ToolEntry, find, registry};
pub use tool::{Context, Permission, Tool, ToolOptions};

#[cfg(feature = "config")]
pub mod config;
#[cfg(feature = "runner")]
pub mod runner;

#[cfg(any(feature = "snapshot", feature = "restore"))]
mod archive;
#[cfg(feature = "bash")]
pub mod bash;
#[cfg(feature = "boilerplate")]
pub mod boilerplate;
#[cfg(feature = "cp")]
pub mod cp;
#[cfg(feature = "dead-code")]
pub mod dead_code;
#[cfg(feature = "deps")]
pub mod deps;
#[cfg(feature = "diff")]
pub mod diff;
#[cfg(feature = "extract")]
pub mod extract;
#[cfg(feature = "git")]
pub mod git;
#[cfg(feature = "grep")]
pub mod grep;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "import")]
pub mod import;
#[cfg(feature = "lint")]
pub mod lint;
#[cfg(feature = "loc")]
pub mod loc;
#[cfg(feature = "mkdir")]
pub mod mkdir;
#[cfg(feature = "mv")]
pub mod mv;
#[cfg(feature = "patch")]
pub mod patch;
#[cfg(feature = "read")]
pub mod read;
#[cfg(feature = "refactor")]
pub mod refactor;
#[cfg(feature = "replace")]
pub mod replace;
#[cfg(feature = "restore")]
pub mod restore;
#[cfg(feature = "rm")]
pub mod rm;
#[cfg(feature = "scan")]
pub mod scan;
#[cfg(feature = "snapshot")]
pub mod snapshot;
#[cfg(feature = "summarize")]
pub mod summarize;
#[cfg(feature = "template")]
pub mod template;
#[cfg(feature = "write")]
pub mod write;

/// `serde` default helpers shared by options structs.
#[allow(dead_code)]
pub(crate) mod defaults {
    pub(crate) fn dot() -> String {
        ".".to_string()
    }
    pub(crate) fn depth() -> usize {
        3
    }
    pub(crate) fn yes() -> bool {
        true
    }
}
