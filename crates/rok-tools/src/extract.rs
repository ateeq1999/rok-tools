//! `extract`: read keys out of JSON, TOML, YAML or `.env` files.

use crate::tool::{Context, Tool, read_to_string};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct ExtractOptions {
    /// File to read (`.json`, `.toml`, `.yaml`/`.yml` or `.env`).
    pub path: String,
    /// Top-level keys to keep (comma-separated); all keys when empty.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_delimiter = ','))]
    pub pick: Vec<String>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractOutput {
    /// The file read.
    pub path: String,
    /// The extracted data.
    pub data: Value,
}

/// The `extract` Tool.
#[derive(Debug)]
pub struct Extract;

impl Tool for Extract {
    const NAME: &'static str = "extract";
    const ABOUT: &'static str = "Read keys out of JSON, TOML, YAML or .env files";
    const GROUP: &'static str = "code";
    type Options = ExtractOptions;
    type Output = ExtractOutput;

    fn run(o: &ExtractOptions, cx: &Context) -> Result<ExtractOutput> {
        run(o, cx)
    }

    fn to_text(out: &ExtractOutput) -> String {
        serde_json::to_string_pretty(&out.data).unwrap_or_default()
    }
}

/// Extracts `o.pick` from `o.path`.
pub fn run(o: &ExtractOptions, cx: &Context) -> Result<ExtractOutput> {
    let content = read_to_string(&cx.resolve(&o.path))?;
    let path = o.path.to_lowercase();
    let bad = |kind: &str, e: &dyn std::fmt::Display| {
        ToolError::Failed(format!("invalid {kind} in {}: {e}", o.path))
    };
    let value: Value = if path.ends_with(".json") {
        serde_json::from_str(&content).map_err(|e| bad("JSON", &e))?
    } else if path.ends_with(".toml") {
        let v: toml::Value = toml::from_str(&content).map_err(|e| bad("TOML", &e))?;
        serde_json::to_value(v).map_err(|e| bad("TOML", &e))?
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::from_str(&content).map_err(|e| bad("YAML", &e))?
    } else if path.ends_with(".env")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|f| f.starts_with(".env"))
    {
        Value::Object(parse_env(&content))
    } else {
        return Ok(ExtractOutput {
            path: o.path.clone(),
            data: serde_json::json!({ "raw": content }),
        });
    };
    Ok(ExtractOutput {
        path: o.path.clone(),
        data: pick(value, &o.pick),
    })
}

fn pick(value: Value, keys: &[String]) -> Value {
    if keys.is_empty() {
        return value;
    }
    let mut out = Map::new();
    for key in keys {
        if let Some(v) = value.get(key) {
            out.insert(key.clone(), v.clone());
        }
    }
    Value::Object(out)
}

fn parse_env(content: &str) -> Map<String, Value> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            (
                k.trim().trim_start_matches("export ").to_string(),
                Value::String(v.to_string()),
            )
        })
        .collect()
}
