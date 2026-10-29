//! `scan`: map a project's files, stack, entry points, exports and imports.

use crate::tool::{Context, Tool, extension, has_extension};
use crate::{Result, lang, walk};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How much detail [`run`] collects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum ScanDetail {
    /// Tree, stack and entry points only.
    #[default]
    Summary,
    /// Everything, including exports and imports.
    Full,
    /// Summary plus each file's imports.
    Imports,
    /// Summary plus each file's exports.
    Exports,
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct ScanOptions {
    /// Directory to scan.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// How many directory levels to descend.
    #[serde(default = "crate::defaults::depth")]
    #[cfg_attr(feature = "cli", arg(long, default_value_t = crate::defaults::depth()))]
    pub depth: usize,
    /// Only include files with these extensions (comma-separated).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub include: Vec<String>,
    /// How much detail to collect.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t = ScanDetail::Summary))]
    pub output: ScanDetail,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutput {
    /// The scanned path.
    pub path: String,
    /// Frameworks and tools recognised from file paths.
    pub stack: Vec<String>,
    /// Likely entry-point files.
    pub entry_points: Vec<String>,
    /// Number of files included.
    pub file_count: usize,
    /// Directory (relative to `path`) to the file names inside it.
    pub tree: BTreeMap<String, Vec<String>>,
    /// File to exported names (`full`/`exports`).
    pub exports: BTreeMap<String, Vec<String>>,
    /// File to imported modules (`full`/`imports`).
    pub imports_graph: BTreeMap<String, Vec<String>>,
}

/// The `scan` Tool.
#[derive(Debug)]
pub struct Scan;

impl Tool for Scan {
    const NAME: &'static str = "scan";
    const ABOUT: &'static str = "Map a project's files, stack and entry points";
    const GROUP: &'static str = "search";
    type Options = ScanOptions;
    type Output = ScanOutput;

    fn run(o: &ScanOptions, cx: &Context) -> Result<ScanOutput> {
        run(o, cx)
    }

    fn to_text(out: &ScanOutput) -> String {
        let mut s = format!("{}: {} file(s)\n", out.path, out.file_count);
        if !out.stack.is_empty() {
            s.push_str(&format!("stack: {}\n", out.stack.join(", ")));
        }
        if !out.entry_points.is_empty() {
            s.push_str(&format!("entry points: {}\n", out.entry_points.join(", ")));
        }
        for (dir, files) in &out.tree {
            s.push_str(&format!("{dir}/ {}\n", files.join(" ")));
        }
        s
    }
}

/// Scans `o.path`.
pub fn run(o: &ScanOptions, cx: &Context) -> Result<ScanOutput> {
    let root = cx.resolve(&o.path);
    if !root.exists() {
        return Err(crate::ToolError::NotFound(root));
    }
    let want_exports = matches!(o.output, ScanDetail::Exports | ScanDetail::Full);
    let want_imports = matches!(o.output, ScanDetail::Imports | ScanDetail::Full);
    let mut out = ScanOutput {
        path: o.path.clone(),
        stack: Vec::new(),
        entry_points: Vec::new(),
        file_count: 0,
        tree: BTreeMap::new(),
        exports: BTreeMap::new(),
        imports_graph: BTreeMap::new(),
    };
    for file in walk::files(&root, cx, Some(o.depth)) {
        if !has_extension(&file, &o.include) {
            continue;
        }
        out.file_count += 1;
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let (dir, name) = rel.rsplit_once('/').unwrap_or((".", &rel));
        out.tree
            .entry(dir.to_string())
            .or_default()
            .push(name.to_string());
        detect_stack(&rel, &mut out.stack);
        let ext = extension(&file);
        if is_entry_point(&file, &ext) {
            out.entry_points.push(rel.clone());
        }
        if want_exports || want_imports {
            let Ok(content) = std::fs::read_to_string(&file) else {
                continue;
            };
            let exports = if want_exports {
                lang::exports(&content, &ext)
            } else {
                Vec::new()
            };
            if !exports.is_empty() {
                out.exports.insert(rel.clone(), exports);
            }
            let imports = if want_imports {
                lang::imports(&content, &ext)
            } else {
                Vec::new()
            };
            if !imports.is_empty() {
                out.imports_graph.insert(rel, imports);
            }
        }
    }
    Ok(out)
}

fn detect_stack(path: &str, stack: &mut Vec<String>) {
    const DETECTORS: &[(&str, &str)] = &[
        ("react", "react"),
        ("vue", "vue"),
        ("svelte", "svelte"),
        ("angular", "angular"),
        ("next", "next.js"),
        ("nuxt", "nuxt"),
        ("vite", "vite"),
        ("webpack", "webpack"),
        ("typescript", "typescript"),
        ("tailwind", "tailwind"),
        ("tanstack", "tanstack"),
        ("trpc", "trpc"),
        ("prisma", "prisma"),
        ("deno", "deno"),
        ("tauri", "tauri"),
        ("electron", "electron"),
        ("cargo.toml", "rust"),
    ];
    let lower = path.to_lowercase();
    for (keyword, name) in DETECTORS {
        if lower.contains(keyword) && !stack.iter().any(|s| s == name) {
            stack.push((*name).to_string());
        }
    }
}

fn is_entry_point(path: &std::path::Path, ext: &str) -> bool {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        (ext, stem.as_str()),
        ("ts", "index" | "main" | "app" | "server")
            | ("tsx", "index" | "main" | "app" | "_app")
            | ("js", "index" | "main" | "app")
            | ("rs", "main" | "lib" | "mod")
            | ("go", "main")
            | ("py", "__init__" | "main")
    )
}
