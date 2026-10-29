//! `summarize`: list a source file's imports, exports, functions and types.

use crate::tool::{Context, Tool, extension, read_to_string};
use crate::{Result, lang};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct SummarizeOptions {
    /// Source file to summarize.
    pub path: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizeOutput {
    /// The summarized file.
    pub path: String,
    /// Imported modules.
    pub imports: Vec<String>,
    /// Exported names.
    pub exports: Vec<String>,
    /// Functions defined, as `name()`.
    pub functions: Vec<String>,
    /// Types defined.
    pub types: Vec<String>,
    /// Physical line count.
    pub line_count: usize,
    /// Last modification time (RFC 3339), if known.
    pub last_modified: Option<String>,
}

/// The `summarize` Tool.
#[derive(Debug)]
pub struct Summarize;

impl Tool for Summarize {
    const NAME: &'static str = "summarize";
    const ABOUT: &'static str = "List a file's imports, exports, functions and types";
    const GROUP: &'static str = "code";
    type Options = SummarizeOptions;
    type Output = SummarizeOutput;

    fn run(o: &SummarizeOptions, cx: &Context) -> Result<SummarizeOutput> {
        run(o, cx)
    }

    fn to_text(out: &SummarizeOutput) -> String {
        format!(
            "{} ({} lines)\nimports: {}\nexports: {}\nfunctions: {}\ntypes: {}\n",
            out.path,
            out.line_count,
            out.imports.join(", "),
            out.exports.join(", "),
            out.functions.join(", "),
            out.types.join(", ")
        )
    }
}

static JS_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:function\s+(\w+)|(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\()").unwrap()
});
static RS_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bfn\s+(\w+)").unwrap());
static PY_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*def\s+(\w+)").unwrap());
static GO_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^func\s+(?:\([^)]*\)\s+)?(\w+)").unwrap());
static TS_TYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:interface|type|class)\s+(\w+)").unwrap());
static RS_TYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:struct|enum|trait|type)\s+(\w+)").unwrap());

/// Summarizes `o.path`.
pub fn run(o: &SummarizeOptions, cx: &Context) -> Result<SummarizeOutput> {
    let full = cx.resolve(&o.path);
    let content = read_to_string(&full)?;
    let ext = extension(&full);
    let last_modified = std::fs::metadata(&full)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        });
    let fn_re = match ext.as_str() {
        e if lang::is_js(e) => Some(&*JS_FN),
        "rs" => Some(&*RS_FN),
        "py" => Some(&*PY_FN),
        "go" => Some(&*GO_FN),
        _ => None,
    };
    let type_re = match ext.as_str() {
        "ts" | "tsx" => Some(&*TS_TYPE),
        "rs" => Some(&*RS_TYPE),
        _ => None,
    };
    Ok(SummarizeOutput {
        path: o.path.clone(),
        imports: lang::imports(&content, &ext),
        exports: lang::exports(&content, &ext),
        functions: fn_re
            .map(|re| captures(re, &content, "()"))
            .unwrap_or_default(),
        types: type_re
            .map(|re| captures(re, &content, ""))
            .unwrap_or_default(),
        line_count: content.lines().count(),
        last_modified,
    })
}

/// The first participating group of every match, deduplicated in order.
fn captures(re: &Regex, content: &str, suffix: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.iter().skip(1).flatten().next() {
            let name = format!("{}{suffix}", m.as_str());
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}
