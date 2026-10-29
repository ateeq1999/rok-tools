//! `grep`: find lines matching a pattern.

use crate::tool::{Context, Tool, has_extension};
use crate::{Result, pattern, walk};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct GrepOptions {
    /// Text to look for (a regular expression with `--regex`).
    pub pattern: String,
    /// File or directory to search.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Only search files with these extensions (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub ext: Vec<String>,
    /// Treat the pattern as a regular expression.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'e'))]
    pub regex: bool,
    /// Match case-insensitively.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'i'))]
    pub ignore_case: bool,
    /// Match whole words only.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'w'))]
    pub whole_word: bool,
}

impl GrepOptions {
    /// Searches `path` for the literal text `pattern`.
    pub fn new(pattern: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            path: path.into(),
            ext: Vec::new(),
            regex: false,
            ignore_case: false,
            whole_word: false,
        }
    }
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrepMatch {
    /// File, relative to the working directory.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// The line's text.
    pub text: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepOutput {
    /// The pattern searched for.
    pub pattern: String,
    /// Every matching line, in path then line order.
    pub matches: Vec<GrepMatch>,
}

/// The `grep` Tool.
#[derive(Debug)]
pub struct Grep;

impl Tool for Grep {
    const NAME: &'static str = "grep";
    const ABOUT: &'static str = "Find lines matching a pattern";
    const GROUP: &'static str = "search";
    type Options = GrepOptions;
    type Output = GrepOutput;

    fn run(o: &GrepOptions, cx: &Context) -> Result<GrepOutput> {
        run(o, cx)
    }

    fn to_text(out: &GrepOutput) -> String {
        out.matches
            .iter()
            .map(|m| format!("{}:{}: {}\n", m.path, m.line, m.text))
            .collect()
    }
}

/// Searches `o.path` for `o.pattern`.
pub fn run(o: &GrepOptions, cx: &Context) -> Result<GrepOutput> {
    let re = pattern::compile(&o.pattern, o.regex, o.whole_word, o.ignore_case)?;
    let mut matches = Vec::new();
    for file in walk::files(&cx.resolve(&o.path), cx, None) {
        if !has_extension(&file, &o.ext) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = cx.relative(&file);
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                matches.push(GrepMatch {
                    path: rel.clone(),
                    line: i + 1,
                    text: line.to_string(),
                });
            }
        }
    }
    Ok(GrepOutput {
        pattern: o.pattern.clone(),
        matches,
    })
}
