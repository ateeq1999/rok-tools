//! Regex-based import/export extraction shared by `scan` and `summarize`.

use regex::Regex;
use std::sync::LazyLock;

static JS_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"import\s+(?:[\w{}\s,*]+\s+from\s+)?['"]([^'"]+)['"]"#).unwrap());
static RS_USE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"use\s+([\w:]+)").unwrap());
static JS_EXPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:export\s+(?:default\s+)?(?:const|let|var|function|class|interface|type)\s+(\w+)|export\s+\{\s*([^}]+)\s*\})",
    )
    .unwrap()
});
static RS_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:pub\s+)?(?:fn|struct|enum|trait|impl|const|mod)\s+(\w+)").unwrap()
});

/// Whether `ext` is a JavaScript/TypeScript extension.
pub(crate) fn is_js(ext: &str) -> bool {
    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
}

/// Module specifiers imported by `content` (JS/TS `import … from`, Rust `use`).
pub(crate) fn imports(content: &str, ext: &str) -> Vec<String> {
    let re = match ext {
        e if is_js(e) => &*JS_IMPORT,
        "rs" => &*RS_USE,
        _ => return Vec::new(),
    };
    re.captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

/// Names exported by JS/TS `export` statements, or Rust items.
pub(crate) fn exports(content: &str, ext: &str) -> Vec<String> {
    if is_js(ext) {
        let mut out = Vec::new();
        for cap in JS_EXPORT.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                out.push(name.as_str().to_string());
            } else if let Some(list) = cap.get(2) {
                out.extend(
                    list.as_str()
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                );
            }
        }
        return out;
    }
    if ext == "rs" {
        return RS_ITEM
            .captures_iter(content)
            .map(|c| c[1].to_string())
            .collect();
    }
    Vec::new()
}
