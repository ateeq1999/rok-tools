//! `template`: render a Tera template (built-in, project or file) to an output file.
//!
//! Project templates live in `.rok/templates/<dir>/` with a
//! `.rok-template.json` naming them (`{"name": "…", "output": [{"from": "…", "to": "…"}]}`);
//! the first output's `from` file is the template source.

use crate::kv::{self, KeyValue};
use crate::tool::{Context, Tool, read_to_string, write_file};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Tera, Value};

/// Options for [`run`]. Exactly one of `name`, `builtin` or `source` must be set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct TemplateOptions {
    /// Project template name (from `.rok/templates`).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub name: Option<String>,
    /// Built-in template: react-route, react-component, api-handler, rust-module, test-file.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub builtin: Option<String>,
    /// Template file to render.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub source: Option<String>,
    /// File to write the rendered result to.
    #[cfg_attr(feature = "cli", arg(long, short = 'o'))]
    pub output: String,
    /// Template variables, as `KEY=VALUE`.
    #[serde(default, with = "kv::as_map")]
    #[cfg_attr(feature = "cli", arg(long = "var"))]
    pub vars: Vec<KeyValue>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateOutput {
    /// The written file.
    pub output: String,
    /// Size of the rendered result in bytes.
    pub bytes: usize,
}

/// The `template` Tool.
#[derive(Debug)]
pub struct Template;

impl Tool for Template {
    const NAME: &'static str = "template";
    const ABOUT: &'static str = "Render a Tera template to a file";
    const GROUP: &'static str = "template";
    type Options = TemplateOptions;
    type Output = TemplateOutput;

    fn run(o: &TemplateOptions, cx: &Context) -> Result<TemplateOutput> {
        run(o, cx)
    }

    fn to_text(out: &TemplateOutput) -> String {
        format!("rendered {} ({} bytes)", out.output, out.bytes)
    }
}

/// The names of the built-in templates.
pub const BUILTINS: &[&str] = &[
    "react-route",
    "react-component",
    "api-handler",
    "rust-module",
    "test-file",
];

/// Renders the selected template with `o.vars` into `o.output`.
pub fn run(o: &TemplateOptions, cx: &Context) -> Result<TemplateOutput> {
    let source = match (&o.name, &o.builtin, &o.source) {
        (Some(name), None, None) => project_template(name, cx)?,
        (None, Some(b), None) => builtin(b)
            .ok_or_else(|| {
                ToolError::invalid(format!(
                    "unknown builtin `{b}` (have: {})",
                    BUILTINS.join(", ")
                ))
            })?
            .to_string(),
        (None, None, Some(path)) => read_to_string(&cx.resolve(path))?,
        _ => {
            return Err(ToolError::invalid(
                "set exactly one of `name`, `builtin` or `source`",
            ));
        }
    };
    let rendered = render(&source, &kv::to_map(&o.vars))?;
    write_file(&cx.resolve(&o.output), &rendered)?;
    Ok(TemplateOutput {
        output: o.output.clone(),
        bytes: rendered.len(),
    })
}

/// Renders `source` with `vars` and the rok case/plural filters.
pub fn render(source: &str, vars: &std::collections::BTreeMap<String, String>) -> Result<String> {
    let mut tera = Tera::default();
    register_filters(&mut tera);
    let context =
        tera::Context::from_serialize(vars).map_err(|e| ToolError::invalid(e.to_string()))?;
    tera.render_str(source, &context).map_err(|e| {
        let mut msg = e.to_string();
        let mut cause = std::error::Error::source(&e);
        while let Some(c) = cause {
            msg.push_str(&format!(": {c}"));
            cause = c.source();
        }
        ToolError::Failed(format!("template render failed: {msg}"))
    })
}

fn project_template(name: &str, cx: &Context) -> Result<String> {
    let root = cx.cwd.join(".rok").join("templates");
    let dirs = std::fs::read_dir(&root).map_err(|e| ToolError::io("read", &root, e))?;
    for dir in dirs.filter_map(|d| d.ok().map(|d| d.path())) {
        let Ok(raw) = std::fs::read_to_string(dir.join(".rok-template.json")) else {
            continue;
        };
        let Ok(schema) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if schema["name"] != name {
            continue;
        }
        let from = schema["output"][0]["from"]
            .as_str()
            .ok_or_else(|| ToolError::Failed(format!("template `{name}` has no output[0].from")))?;
        return read_to_string(&dir.join(from));
    }
    Err(ToolError::Failed(format!(
        "no project template named `{name}` in .rok/templates"
    )))
}

fn builtin(name: &str) -> Option<&'static str> {
    Some(match name {
        "react-route" => {
            "import { createFileRoute } from '@tanstack/react-router';\nimport Component from '{{component}}';\n\nexport const Route = createFileRoute('{{path}}')({\n  component: Component,\n});\n"
        }
        "react-component" => {
            "import React from 'react';\n\ninterface {{name}}Props {\n  className?: string;\n}\n\nexport function {{name}}({ className }: {{name}}Props) {\n  return (\n    <div className={className}>\n      {{name}} Component\n    </div>\n  );\n}\n"
        }
        "api-handler" => {
            "import { json } from '@tanstack/start';\n\nexport async function handler(request: Request) {\n  const data = {\n    message: 'Hello from {{name}}',\n    timestamp: new Date().toISOString(),\n  };\n  return json(data);\n}\n"
        }
        "rust-module" => {
            "pub mod {{name}} {\n    pub fn greet() -> &'static str {\n        \"Hello from {{name}}!\"\n    }\n}\n"
        }
        "test-file" => {
            "import { describe, it, expect } from 'vitest';\n\ndescribe('{{name}}', () => {\n  it('should work', () => {\n    expect(true).toBe(true);\n  });\n});\n"
        }
        _ => return None,
    })
}

fn words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn pluralize(s: &str) -> String {
    let vowel_before_y = s.len() > 1 && s[..s.len() - 1].ends_with(['a', 'e', 'i', 'o', 'u']);
    if ["s", "x", "z", "ch", "sh"].iter().any(|e| s.ends_with(e)) {
        format!("{s}es")
    } else if s.ends_with('y') && !vowel_before_y {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{s}s")
    }
}

fn singularize(s: &str) -> String {
    if let Some(stem) = s.strip_suffix("ies") {
        format!("{stem}y")
    } else if s.ends_with("ss") {
        s.to_string()
    } else if let Some(stem) = ["ches", "shes", "xes", "zes", "ses"]
        .iter()
        .find_map(|e| s.strip_suffix(e).map(|st| (st, e)))
    {
        format!("{}{}", stem.0, &stem.1[..stem.1.len() - 2])
    } else {
        s.strip_suffix('s').unwrap_or(s).to_string()
    }
}

type Filter = fn(&str) -> String;

fn register_filters(tera: &mut Tera) {
    let string_filters: [(&str, Filter); 9] = [
        ("camelcase", |s| {
            let w = words(s);
            w.iter()
                .enumerate()
                .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
                .collect()
        }),
        ("pascalcase", |s| {
            words(s).iter().map(|w| capitalize(w)).collect()
        }),
        ("snakecase", |s| words(s).join("_")),
        ("kebabcase", |s| words(s).join("-")),
        ("pluralize", pluralize),
        ("singularize", singularize),
        ("uppercase", |s| s.to_uppercase()),
        ("lowercase", |s| s.to_lowercase()),
        ("capitalize", |s| capitalize(&s.to_lowercase())),
    ];
    for (name, f) in string_filters {
        tera.register_filter(name, move |v: &Value, _: &HashMap<String, Value>| {
            Ok(Value::String(f(v.as_str().unwrap_or(""))))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_filters() {
        let vars = [("n".to_string(), "userProfile item".to_string())]
            .into_iter()
            .collect();
        let out = render(
            "{{n|snakecase}} {{n|kebabcase}} {{n|pascalcase}} {{n|camelcase}}",
            &vars,
        )
        .unwrap();
        assert_eq!(
            out,
            "user_profile_item user-profile-item UserProfileItem userProfileItem"
        );
    }

    #[test]
    fn plural_filters() {
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("day"), "days");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("boxes"), "box");
        assert_eq!(singularize("class"), "class");
        assert_eq!(singularize("users"), "user");
    }
}
