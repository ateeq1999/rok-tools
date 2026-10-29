//! `import`: add, remove and sort import lines in JS/TS, Python, Rust and Go files.

use crate::tool::{Context, Tool, extension, read_to_string, write_file};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct ImportOptions {
    /// Source file to edit.
    pub path: String,
    /// Imports to add (full lines, or just the module part).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub add: Vec<String>,
    /// Remove the first import line containing each of these texts.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub remove: Vec<String>,
    /// Sort the import lines.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub organize: bool,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOutput {
    /// The edited file.
    pub path: String,
    /// Lines added.
    pub added: Vec<String>,
    /// Lines removed.
    pub removed: Vec<String>,
    /// Whether imports were sorted.
    pub organized: bool,
}

/// The `import` Tool.
#[derive(Debug)]
pub struct Import;

impl Tool for Import {
    const NAME: &'static str = "import";
    const ABOUT: &'static str = "Add, remove and sort import lines";
    const GROUP: &'static str = "code";
    type Options = ImportOptions;
    type Output = ImportOutput;

    fn run(o: &ImportOptions, cx: &Context) -> Result<ImportOutput> {
        run(o, cx)
    }

    fn to_text(out: &ImportOutput) -> String {
        let mut s = String::new();
        out.added
            .iter()
            .for_each(|l| s.push_str(&format!("+ {l}\n")));
        out.removed
            .iter()
            .for_each(|l| s.push_str(&format!("- {l}\n")));
        if out.organized {
            s.push_str("imports sorted\n");
        }
        s
    }
}

struct Lang {
    is_import: fn(&str) -> bool,
    prefix: &'static str,
    already_full: fn(&str) -> bool,
}

fn lang(ext: &str) -> Option<Lang> {
    Some(match ext {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => Lang {
            is_import: |l| l.starts_with("import "),
            prefix: "import ",
            already_full: |s| s.starts_with("import ") || s.starts_with("export "),
        },
        "py" => Lang {
            is_import: |l| l.starts_with("import ") || l.starts_with("from "),
            prefix: "import ",
            already_full: |s| s.starts_with("import ") || s.starts_with("from "),
        },
        "rs" => Lang {
            is_import: |l| l.starts_with("use ") || l.starts_with("pub use "),
            prefix: "use ",
            already_full: |s| s.starts_with("use ") || s.starts_with("pub use "),
        },
        "go" => Lang {
            is_import: |l| l.starts_with("\t\"") || l.starts_with("import \""),
            prefix: "",
            already_full: |s| s.starts_with('\t') || s.starts_with("import "),
        },
        _ => return None,
    })
}

/// Edits the imports of `o.path`.
pub fn run(o: &ImportOptions, cx: &Context) -> Result<ImportOutput> {
    let full = cx.resolve(&o.path);
    let content = read_to_string(&full)?;
    let ext = extension(&full);
    let lang =
        lang(&ext).ok_or_else(|| ToolError::invalid(format!("unsupported file type `.{ext}`")))?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut removed = Vec::new();
    for needle in &o.remove {
        if let Some(i) = lines
            .iter()
            .position(|l| (lang.is_import)(l) && l.contains(needle.as_str()))
        {
            removed.push(lines.remove(i));
        }
    }
    let mut added = Vec::new();
    for imp in &o.add {
        if lines.iter().any(|l| l.contains(imp.as_str())) {
            continue;
        }
        let line = if ext == "go" && !(lang.already_full)(imp) {
            format!("\t\"{}\"", imp.trim_matches('"'))
        } else if (lang.already_full)(imp) {
            imp.clone()
        } else {
            format!("{}{imp}", lang.prefix)
        };
        let at = insertion_point(&lines, &lang, &ext)?;
        lines.insert(at, line.clone());
        added.push(line);
    }
    if o.organize {
        organize(&mut lines, &lang, &ext);
    }
    let mut new = lines.join("\n");
    if content.ends_with('\n') {
        new.push('\n');
    }
    if new != content {
        write_file(&full, &new)?;
    }
    Ok(ImportOutput {
        path: o.path.clone(),
        added,
        removed,
        organized: o.organize,
    })
}

fn insertion_point(lines: &[String], lang: &Lang, ext: &str) -> Result<usize> {
    if ext == "go" {
        let start = lines
            .iter()
            .position(|l| l.trim() == "import (")
            .ok_or_else(|| {
                ToolError::Failed("Go file has no `import ( … )` block to add to".into())
            })?;
        let end = lines[start..]
            .iter()
            .position(|l| l.trim() == ")")
            .map(|i| start + i);
        return end.ok_or_else(|| ToolError::Failed("unterminated Go import block".into()));
    }
    Ok(match lines.iter().rposition(|l| (lang.is_import)(l)) {
        Some(last) => last + 1,
        None => lines
            .iter()
            .take_while(|l| l.starts_with("#!") || l.starts_with("//!"))
            .count(),
    })
}

/// Sorts each contiguous run of import lines in place (JS: relative paths after packages).
fn organize(lines: &mut [String], lang: &Lang, ext: &str) {
    let mut i = 0;
    while i < lines.len() {
        if !(lang.is_import)(&lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && (lang.is_import)(&lines[i]) {
            i += 1;
        }
        let block = &mut lines[start..i];
        if matches!(ext, "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs") {
            block.sort_by_key(|l| (l.contains("'.") || l.contains("\"."), l.clone()));
        } else {
            block.sort();
        }
    }
}
