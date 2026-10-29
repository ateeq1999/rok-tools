//! `deps`: build a file-level import graph and find two-file cycles.

use crate::tool::{Context, Tool, extension, has_extension};
use crate::{Result, walk};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct DepsOptions {
    /// Directory to analyse.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// How many directory levels to descend.
    #[serde(default = "crate::defaults::depth")]
    #[cfg_attr(feature = "cli", arg(long, default_value_t = crate::defaults::depth()))]
    pub depth: usize,
    /// Only include these extensions (comma-separated); JS/TS, Python, Rust and Go by default.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub include: Vec<String>,
    /// Only report this file's dependencies and dependents.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub focus: Option<String>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepsOutput {
    /// The analysed path.
    pub path: String,
    /// File to the modules it imports.
    pub graph: BTreeMap<String, Vec<String>>,
    /// Module to the files importing it.
    pub dependents: BTreeMap<String, Vec<String>>,
    /// Number of files analysed.
    pub file_count: usize,
    /// Pairs of files that import each other.
    pub cycles: Vec<Vec<String>>,
}

/// The `deps` Tool.
#[derive(Debug)]
pub struct Deps;

impl Tool for Deps {
    const NAME: &'static str = "deps";
    const ABOUT: &'static str = "Build a file-level import graph";
    const GROUP: &'static str = "code";
    type Options = DepsOptions;
    type Output = DepsOutput;

    fn run(o: &DepsOptions, cx: &Context) -> Result<DepsOutput> {
        run(o, cx)
    }

    fn to_text(out: &DepsOutput) -> String {
        let mut s: String = out
            .graph
            .iter()
            .map(|(f, deps)| format!("{f} -> {}\n", deps.join(", ")))
            .collect();
        for c in &out.cycles {
            s.push_str(&format!("cycle: {}\n", c.join(" <-> ")));
        }
        s
    }
}

static JS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:import|from|require\()\s*['"]([^'"]+)['"]"#).unwrap());
static PY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:import\s+(\S+)|from\s+(\S+)\s+import)").unwrap());
static RS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:pub\s+)?use\s+([A-Za-z_]\w*)").unwrap());
static GO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*(?:import\s+)?(?:\w+\s+)?"([^"]+)""#).unwrap());

fn imports(content: &str, ext: &str) -> Vec<String> {
    match ext {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => JS
            .captures_iter(content)
            .map(|c| c[1].to_string())
            .collect(),
        "py" => content
            .lines()
            .filter_map(|l| PY.captures(l))
            .filter_map(|c| {
                c.get(1)
                    .or_else(|| c.get(2))
                    .map(|m| m.as_str().to_string())
            })
            .collect(),
        "rs" => content
            .lines()
            .filter_map(|l| RS.captures(l).map(|c| c[1].to_string()))
            .filter(|s| !matches!(s.as_str(), "crate" | "super" | "self"))
            .collect(),
        "go" => content
            .lines()
            .filter_map(|l| GO.captures(l).map(|c| c[1].to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

const DEFAULT_EXTS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs", "py", "rs", "go"];

/// Analyses the imports under `o.path`.
pub fn run(o: &DepsOptions, cx: &Context) -> Result<DepsOutput> {
    let include: Vec<String> = if o.include.is_empty() {
        DEFAULT_EXTS.iter().map(|s| s.to_string()).collect()
    } else {
        o.include.clone()
    };
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let files: Vec<_> = walk::files(&cx.resolve(&o.path), cx, Some(o.depth))
        .into_iter()
        .filter(|f| has_extension(f, &include))
        .collect();
    for file in &files {
        let rel = cx.relative(file);
        let content = std::fs::read_to_string(file).unwrap_or_default();
        let mut deps = imports(&content, &extension(file));
        deps.dedup();
        for dep in &deps {
            dependents.entry(dep.clone()).or_default().push(rel.clone());
        }
        graph.insert(rel, deps);
    }
    let cycles = two_cycles(&graph);
    if let Some(focus) = &o.focus {
        let focus = focus.replace('\\', "/");
        graph.retain(|k, v| *k == focus || v.contains(&focus));
        dependents.retain(|k, _| *k == focus);
    }
    Ok(DepsOutput {
        path: o.path.clone(),
        graph,
        dependents,
        file_count: files.len(),
        cycles,
    })
}

/// Pairs `[a, b]` where `a` imports `b` and `b` imports `a`.
fn two_cycles(graph: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    for (a, deps) in graph {
        for b in deps {
            if a < b && graph.get(b).is_some_and(|bd| bd.contains(a)) {
                cycles.push(vec![a.clone(), b.clone()]);
            }
        }
    }
    cycles
}
