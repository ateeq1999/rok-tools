//! `loc`: count lines of code per file and language, and flag files over the Line limit.
//!
//! A line of code is a line that is neither blank nor only a comment. Comments
//! are recognised for line (`//`, `#`, `--`) and block (`/* */`, `<!-- -->`, …)
//! syntax of known languages; files of unknown type count every non-blank line.
//!
//! The limit is 600 lines of code per file unless configured otherwise, and
//! individual paths can be given their own limit (`0` exempts them):
//!
//! ```toml
//! [tools.loc]
//! max_lines = 600
//! exclude = ["**/generated/**"]
//! [[tools.loc.override]]
//! path = "src/schema.rs"
//! max_lines = 1500
//! ```

use crate::tool::{Context, Tool, extension};
use crate::{Result, ToolError, walk};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The Line limit used when none is configured.
pub const DEFAULT_MAX_LINES: usize = 600;

fn default_max_lines() -> usize {
    DEFAULT_MAX_LINES
}

/// A per-path Line limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocOverride {
    /// Glob matched against paths relative to the working directory.
    pub path: String,
    /// Line limit for matching files; `0` exempts them.
    pub max_lines: usize,
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct LocOptions {
    /// File or directory to count.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Line limit: the most lines of code a file may have (`0` disables the limit).
    #[serde(default = "default_max_lines")]
    #[cfg_attr(feature = "cli", arg(long, default_value_t = DEFAULT_MAX_LINES))]
    pub max_lines: usize,
    /// Only count files matching these globs (comma-separated); every known source language when empty.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub include: Vec<String>,
    /// Skip files matching these globs (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub exclude: Vec<String>,
    /// Per-path limits; the first matching override wins (config file only).
    #[serde(default, rename = "override", alias = "overrides")]
    #[cfg_attr(feature = "cli", arg(skip))]
    pub overrides: Vec<LocOverride>,
    /// Fail (exit 1) when any file is over its limit.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub check: bool,
}

impl LocOptions {
    /// Counts `path` with the default Line limit.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            max_lines: DEFAULT_MAX_LINES,
            include: Vec::new(),
            exclude: Vec::new(),
            overrides: Vec::new(),
            check: false,
        }
    }
}

/// Line counts for a file, a language or the whole run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineCounts {
    /// Lines of code (neither blank nor only comment).
    pub code: usize,
    /// Comment-only lines.
    pub comments: usize,
    /// Blank lines.
    pub blank: usize,
    /// All lines.
    pub total: usize,
}

impl std::ops::AddAssign for LineCounts {
    fn add_assign(&mut self, o: Self) {
        self.code += o.code;
        self.comments += o.comments;
        self.blank += o.blank;
        self.total += o.total;
    }
}

/// One counted file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLoc {
    /// Path relative to the working directory.
    pub path: String,
    /// Language name, or `Other`.
    pub language: String,
    /// The file's counts.
    #[serde(flatten)]
    pub counts: LineCounts,
    /// The Line limit that applies (`0` = none).
    pub limit: usize,
}

/// Totals for one language.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageLoc {
    /// Number of files.
    pub files: usize,
    /// Summed counts.
    #[serde(flatten)]
    pub counts: LineCounts,
}

/// A file over its Line limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocViolation {
    /// Path relative to the working directory.
    pub path: String,
    /// Its lines of code.
    pub code: usize,
    /// The limit it exceeds.
    pub limit: usize,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocOutput {
    /// The counted path.
    pub path: String,
    /// The default Line limit used.
    pub max_lines: usize,
    /// Every counted file, largest first.
    pub files: Vec<FileLoc>,
    /// Totals per language.
    pub languages: BTreeMap<String, LanguageLoc>,
    /// Totals over every file.
    pub totals: LineCounts,
    /// Files over their limit, largest first.
    pub violations: Vec<LocViolation>,
    /// Whether violations count as failure.
    pub check: bool,
}

/// The `loc` Tool.
#[derive(Debug)]
pub struct Loc;

impl Tool for Loc {
    const NAME: &'static str = "loc";
    const ABOUT: &'static str = "Count lines of code and flag files over the Line limit";
    const GROUP: &'static str = "code";
    type Options = LocOptions;
    type Output = LocOutput;

    fn run(o: &LocOptions, cx: &Context) -> Result<LocOutput> {
        run(o, cx)
    }

    fn succeeded(out: &LocOutput) -> bool {
        !out.check || out.violations.is_empty()
    }

    fn to_text(out: &LocOutput) -> String {
        let mut s = format!(
            "{:<14}{:>7}{:>9}{:>10}{:>8}\n",
            "language", "files", "code", "comments", "blank"
        );
        for (lang, l) in &out.languages {
            s.push_str(&format!(
                "{lang:<14}{:>7}{:>9}{:>10}{:>8}\n",
                l.files, l.counts.code, l.counts.comments, l.counts.blank
            ));
        }
        let t = out.totals;
        s.push_str(&format!(
            "{:<14}{:>7}{:>9}{:>10}{:>8}\n",
            "total",
            out.files.len(),
            t.code,
            t.comments,
            t.blank
        ));
        if out.violations.is_empty() {
            s.push_str(&format!(
                "\nno file exceeds its Line limit (default {})\n",
                out.max_lines
            ));
        } else {
            s.push_str(&format!(
                "\n{} file(s) over their Line limit:\n",
                out.violations.len()
            ));
            for v in &out.violations {
                s.push_str(&format!(
                    "  {}: {} lines of code (limit {})\n",
                    v.path, v.code, v.limit
                ));
            }
        }
        s
    }
}

/// Counts the files under `o.path` and checks them against their Line limits.
pub fn run(o: &LocOptions, cx: &Context) -> Result<LocOutput> {
    let root = cx.resolve(&o.path);
    if !root.exists() {
        return Err(ToolError::NotFound(root));
    }
    let include = walk::glob_set(&o.include)?;
    let exclude = walk::glob_set(&o.exclude)?;
    let overrides = o
        .overrides
        .iter()
        .map(|ov| {
            Ok((
                globset::Glob::new(&ov.path)
                    .map_err(ToolError::invalid)?
                    .compile_matcher(),
                ov.max_lines,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut out = LocOutput {
        path: o.path.clone(),
        max_lines: o.max_lines,
        files: Vec::new(),
        languages: BTreeMap::new(),
        totals: LineCounts::default(),
        violations: Vec::new(),
        check: o.check,
    };
    for file in walk::files(&root, cx, None) {
        let rel = cx.relative(&file);
        let syntax = syntax(&extension(&file));
        let selected = match &include {
            Some(set) => set.is_match(&rel),
            None => syntax.is_some(),
        };
        if !selected || exclude.as_ref().is_some_and(|set| set.is_match(&rel)) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let counts = count(&content, syntax.as_ref());
        let limit = overrides
            .iter()
            .find(|(m, _)| m.is_match(&rel))
            .map_or(o.max_lines, |(_, l)| *l);
        let language = syntax.map_or("Other", |s| s.name).to_string();
        let entry = out.languages.entry(language.clone()).or_default();
        entry.files += 1;
        entry.counts += counts;
        out.totals += counts;
        if limit > 0 && counts.code > limit {
            out.violations.push(LocViolation {
                path: rel.clone(),
                code: counts.code,
                limit,
            });
        }
        out.files.push(FileLoc {
            path: rel,
            language,
            counts,
            limit,
        });
    }
    out.files.sort_by(|a, b| {
        b.counts
            .code
            .cmp(&a.counts.code)
            .then_with(|| a.path.cmp(&b.path))
    });
    out.violations
        .sort_by(|a, b| b.code.cmp(&a.code).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

/// Comment syntax of one language.
#[derive(Debug, Clone, Copy)]
pub struct Syntax {
    /// Display name.
    pub name: &'static str,
    /// Line-comment markers.
    pub line: &'static [&'static str],
    /// Block-comment start and end markers.
    pub block: Option<(&'static str, &'static str)>,
    /// Whether `'` delimits strings (not in Rust, where it marks lifetimes and chars).
    pub single_quote_strings: bool,
}

const C_BLOCK: Option<(&str, &str)> = Some(("/*", "*/"));
const HTML_BLOCK: Option<(&str, &str)> = Some(("<!--", "-->"));

const fn s(
    name: &'static str,
    line: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
    sq: bool,
) -> Syntax {
    Syntax {
        name,
        line,
        block,
        single_quote_strings: sq,
    }
}

/// The comment syntax for a file extension, if rok-tools knows the language.
pub fn syntax(ext: &str) -> Option<Syntax> {
    Some(match ext {
        "rs" => s("Rust", &["//"], C_BLOCK, false),
        "ts" | "tsx" | "mts" | "cts" => s("TypeScript", &["//"], C_BLOCK, true),
        "js" | "jsx" | "mjs" | "cjs" => s("JavaScript", &["//"], C_BLOCK, true),
        "go" => s("Go", &["//"], C_BLOCK, false),
        "java" => s("Java", &["//"], C_BLOCK, false),
        "kt" | "kts" => s("Kotlin", &["//"], C_BLOCK, false),
        "scala" => s("Scala", &["//"], C_BLOCK, false),
        "swift" => s("Swift", &["//"], C_BLOCK, false),
        "dart" => s("Dart", &["//"], C_BLOCK, true),
        "c" | "h" => s("C", &["//"], C_BLOCK, false),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => s("C++", &["//"], C_BLOCK, false),
        "cs" => s("C#", &["//"], C_BLOCK, false),
        "php" => s("PHP", &["//", "#"], C_BLOCK, true),
        "css" => s("CSS", &[], C_BLOCK, true),
        "scss" | "less" => s("SCSS", &["//"], C_BLOCK, true),
        "py" | "pyi" => s("Python", &["#"], None, true),
        "rb" => s("Ruby", &["#"], None, true),
        "sh" | "bash" | "zsh" => s("Shell", &["#"], None, true),
        "ps1" | "psm1" => s("PowerShell", &["#"], Some(("<#", "#>")), true),
        "ex" | "exs" => s("Elixir", &["#"], None, false),
        "lua" => s("Lua", &["--"], Some(("--[[", "]]")), true),
        "sql" => s("SQL", &["--"], C_BLOCK, true),
        "hs" => s("Haskell", &["--"], Some(("{-", "-}")), false),
        "html" | "htm" => s("HTML", &[], HTML_BLOCK, true),
        "vue" => s("Vue", &["//"], HTML_BLOCK, true),
        "svelte" => s("Svelte", &["//"], HTML_BLOCK, true),
        _ => return None,
    })
}

/// Counts `content` using `syntax` (every non-blank line is code without one).
pub fn count(content: &str, syntax: Option<&Syntax>) -> LineCounts {
    let mut counts = LineCounts::default();
    let mut in_block = false;
    for line in content.lines() {
        counts.total += 1;
        match syntax {
            Some(syn) => match classify(line, syn, &mut in_block) {
                Kind::Code => counts.code += 1,
                Kind::Comment => counts.comments += 1,
                Kind::Blank => counts.blank += 1,
            },
            None if line.trim().is_empty() => counts.blank += 1,
            None => counts.code += 1,
        }
    }
    counts
}

enum Kind {
    Code,
    Comment,
    Blank,
}

fn classify(line: &str, syn: &Syntax, in_block: &mut bool) -> Kind {
    let t = line.trim();
    if t.is_empty() {
        return Kind::Blank;
    }
    let bytes = t.as_bytes();
    let (mut code, mut comment) = (false, false);
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        if !t.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &t[i..];
        if *in_block {
            comment = true;
            match syn.block {
                Some((_, end)) if rest.starts_with(end) => {
                    *in_block = false;
                    i += end.len();
                }
                _ => i += 1,
            }
            continue;
        }
        if let Some(q) = quote {
            match bytes[i] {
                b'\\' => i += 2,
                b if b == q => {
                    quote = None;
                    i += 1;
                }
                _ => i += 1,
            }
            continue;
        }
        if let Some((start, _)) = syn.block
            && rest.starts_with(start)
        {
            *in_block = true;
            comment = true;
            i += start.len();
            continue;
        }
        if syn.line.iter().any(|m| rest.starts_with(m)) {
            comment = true;
            break;
        }
        let b = bytes[i];
        if b == b'"' || (syn.single_quote_strings && b == b'\'') {
            quote = Some(b);
        }
        if !b.is_ascii_whitespace() {
            code = true;
        }
        i += 1;
    }
    if code {
        Kind::Code
    } else if comment {
        Kind::Comment
    } else {
        Kind::Blank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust(src: &str) -> LineCounts {
        count(src, syntax("rs").as_ref())
    }

    #[test]
    fn counts_code_comments_and_blanks() {
        let c = rust("// header\n\nfn main() {\n    /* inline */ let x = 1; // trailing\n}\n");
        assert_eq!(
            c,
            LineCounts {
                code: 3,
                comments: 1,
                blank: 1,
                total: 5
            }
        );
    }

    #[test]
    fn block_comments_span_lines() {
        let c = rust("/*\n * doc\n */\nfn a() {}\n");
        assert_eq!((c.code, c.comments), (1, 3));
    }

    #[test]
    fn comment_markers_inside_strings_are_code() {
        let c = rust("let url = \"http://x/*y\";\nlet z = 1;\n");
        assert_eq!((c.code, c.comments), (2, 0));
    }

    #[test]
    fn unknown_language_counts_non_blank_lines() {
        let c = count("a\n\n# b\n", None);
        assert_eq!((c.code, c.blank), (2, 1));
    }

    #[test]
    fn python_hash_comments() {
        let c = count("# c\nx = '#not'\n", syntax("py").as_ref());
        assert_eq!((c.code, c.comments), (1, 1));
    }
}
