//! `dead-code`: heuristically find unused functions, unused imports and unreachable lines.
//!
//! This is word-level analysis, not a compiler: a function counts as used when
//! its name appears anywhere else in the scanned files.

use crate::tool::{Context, Tool, extension, has_extension};
use crate::{Result, ToolError, walk};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct DeadCodeOptions {
    /// File or directory to analyse.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Only include these extensions (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub include: Vec<String>,
}

/// One finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeIssue {
    /// File, relative to the working directory.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// The symbol involved.
    pub symbol: String,
    /// `function`, `import` or `unreachable`.
    pub kind: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeOutput {
    /// The analysed path.
    pub path: String,
    /// Functions whose name never appears again.
    pub unused_functions: Vec<DeadCodeIssue>,
    /// Imported names never used in their file.
    pub unused_imports: Vec<DeadCodeIssue>,
    /// Lines directly after `return`/`throw`/`panic!`.
    pub unreachable_code: Vec<DeadCodeIssue>,
}

/// The `dead-code` Tool.
#[derive(Debug)]
pub struct DeadCode;

impl Tool for DeadCode {
    const NAME: &'static str = "dead-code";
    const ABOUT: &'static str = "Find unused functions, unused imports and unreachable lines";
    const GROUP: &'static str = "code";
    type Options = DeadCodeOptions;
    type Output = DeadCodeOutput;

    fn run(o: &DeadCodeOptions, cx: &Context) -> Result<DeadCodeOutput> {
        run(o, cx)
    }

    fn to_text(out: &DeadCodeOutput) -> String {
        out.unused_functions
            .iter()
            .chain(&out.unused_imports)
            .chain(&out.unreachable_code)
            .map(|i| format!("{}:{}: {}\n", i.file, i.line, i.message))
            .collect()
    }
}

static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*\b").unwrap());
static JS_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap());
static PY_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^def\s+(\w+)\s*\(").unwrap());
static RS_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)").unwrap());
static GO_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^func\s+(?:\([^)]+\)\s+)?(\w+)").unwrap());
static JS_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"import\s+(?:\{([^}]+)\}|(\w+))\s+from").unwrap());
static PY_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(?:from\s+\S+\s+)?import\s+(.+)$").unwrap());
static RS_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*use\s+[\w:]*::\{([^}]+)\};|^\s*use\s+[\w:]*::(\w+);").unwrap()
});
static UNREACHABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(return\b[^;\n]*;|throw\b[^;\n]*;|panic!\([^;\n]*;)").unwrap()
});

/// Names entry points and trait/framework hooks that are used without being called.
const ENTRY_POINTS: &[&str] = &[
    "main", "new", "default", "fmt", "drop", "from", "into", "init",
];

fn line_of(content: &str, offset: usize) -> usize {
    content[..offset].matches('\n').count() + 1
}

/// Analyses `o.path`.
pub fn run(o: &DeadCodeOptions, cx: &Context) -> Result<DeadCodeOutput> {
    let root = cx.resolve(&o.path);
    if !root.exists() {
        return Err(ToolError::NotFound(root));
    }
    let files: Vec<(String, String, String)> = walk::files(&root, cx, None)
        .into_iter()
        .filter(|f| has_extension(f, &o.include))
        .filter_map(|f| {
            Some((
                cx.relative(&f),
                extension(&f),
                std::fs::read_to_string(&f).ok()?,
            ))
        })
        .collect();

    let mut word_counts: HashMap<&str, usize> = HashMap::new();
    for (_, _, content) in &files {
        for w in WORD.find_iter(content) {
            *word_counts.entry(w.as_str()).or_default() += 1;
        }
    }

    let mut out = DeadCodeOutput {
        path: o.path.clone(),
        unused_functions: Vec::new(),
        unused_imports: Vec::new(),
        unreachable_code: Vec::new(),
    };
    let issue = |file: &str, line, symbol: &str, kind: &str, message: String| DeadCodeIssue {
        file: file.to_string(),
        line,
        symbol: symbol.to_string(),
        kind: kind.to_string(),
        message,
    };
    for (rel, ext, content) in &files {
        let fn_re = match ext.as_str() {
            "js" | "jsx" | "ts" | "tsx" => Some(&*JS_FN),
            "py" => Some(&*PY_FN),
            "rs" => Some(&*RS_FN),
            "go" => Some(&*GO_FN),
            _ => None,
        };
        for cap in fn_re.into_iter().flat_map(|re| re.captures_iter(content)) {
            let name = &cap[1];
            if word_counts.get(name).copied().unwrap_or(0) <= 1
                && !ENTRY_POINTS.contains(&name)
                && !name.starts_with("test")
            {
                let line = line_of(content, cap.get(0).map_or(0, |m| m.start()));
                out.unused_functions.push(issue(
                    rel,
                    line,
                    name,
                    "function",
                    format!("function `{name}` is never used"),
                ));
            }
        }
        let mut local: BTreeMap<&str, usize> = BTreeMap::new();
        for w in WORD.find_iter(content) {
            *local.entry(w.as_str()).or_default() += 1;
        }
        for (symbol, line) in imported_names(content, ext) {
            if local.get(symbol.as_str()).copied().unwrap_or(0) <= 1 {
                out.unused_imports.push(issue(
                    rel,
                    line,
                    &symbol,
                    "import",
                    format!("import `{symbol}` is unused"),
                ));
            }
        }
        for m in UNREACHABLE.find_iter(content) {
            let next = content[m.end()..]
                .lines()
                .nth(1)
                .map(str::trim)
                .unwrap_or("");
            if !next.is_empty()
                && !next.starts_with('}')
                && !next.starts_with("//")
                && !next.starts_with('#')
            {
                let line = line_of(content, m.end()) + 1;
                out.unreachable_code.push(issue(
                    rel,
                    line,
                    "",
                    "unreachable",
                    "code after return/throw/panic may be unreachable".into(),
                ));
            }
        }
    }
    Ok(out)
}

/// `(name, line)` for every name a file imports.
fn imported_names(content: &str, ext: &str) -> Vec<(String, usize)> {
    let re = match ext {
        "js" | "jsx" | "ts" | "tsx" => &*JS_IMPORT,
        "py" => &*PY_IMPORT,
        "rs" => &*RS_IMPORT,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for cap in re.captures_iter(content) {
        let line = line_of(content, cap.get(0).map_or(0, |m| m.start()));
        for group in cap.iter().skip(1).flatten() {
            for part in group.as_str().split(',') {
                // `a as b` imports `b`; `self` and globs are skipped.
                let name = part
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .trim_matches(|c| c == '{' || c == '}');
                if !name.is_empty() && name != "self" && name != "*" {
                    out.push((name.to_string(), line));
                }
            }
        }
    }
    out
}
