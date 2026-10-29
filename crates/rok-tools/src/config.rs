//! `[tools.<tool>]` sections of `rok.toml`.
//!
//! ```toml
//! [tools.loc]
//! max_lines = 600
//! exclude = ["**/generated/**"]
//!
//! [tools.grep]
//! ignore_case = true
//! ```
//!
//! Keys are the Tool's option names (`-` and `_` are interchangeable). Other
//! sections of `rok.toml` (such as rok-data's) are ignored.

use crate::{Result, ToolError};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The config file name.
pub const FILE_NAME: &str = "rok.toml";

/// Per-Tool settings loaded from `rok.toml`.
#[derive(Debug, Clone, Default)]
pub struct ToolsConfig {
    tools: Map<String, Value>,
    source: Option<PathBuf>,
}

impl ToolsConfig {
    /// Loads the nearest `rok.toml` in `dir` or its ancestors; empty when there is none.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let start = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        match start
            .ancestors()
            .map(|d| d.join(FILE_NAME))
            .find(|p| p.is_file())
        {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default()),
        }
    }

    /// Loads one specific file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| ToolError::io("read", path, e))?;
        let mut config = Self::from_toml_str(&text)
            .map_err(|e| ToolError::invalid(format!("{}: {e}", path.display())))?;
        config.source = Some(path.to_path_buf());
        Ok(config)
    }

    /// Parses TOML text.
    pub fn from_toml_str(text: &str) -> Result<Self> {
        let table: toml::Table = toml::from_str(text).map_err(ToolError::invalid)?;
        let root = serde_json::to_value(table).map_err(ToolError::invalid)?;
        let tools = match root.get("tools") {
            None => Map::new(),
            Some(Value::Object(t)) => t.clone(),
            Some(_) => return Err(ToolError::invalid("`tools` must be a table")),
        };
        let tools = tools
            .into_iter()
            .map(|(name, v)| (normalize(&name), normalize_keys(v)))
            .collect();
        Ok(Self {
            tools,
            source: None,
        })
    }

    /// The file the settings came from, if any.
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// The raw `[tools.<name>]` table, keys normalized to `snake_case`.
    pub fn tool(&self, name: &str) -> Option<&Map<String, Value>> {
        self.tools.get(&normalize(name)).and_then(Value::as_object)
    }

    /// `[tools.<name>]` deserialized into a Tool's options; option defaults fill the gaps.
    ///
    /// Fails when the table lacks a required option (e.g. `grep`'s `pattern`);
    /// use [`ToolsConfig::merge_into`] to layer settings over options you already have.
    pub fn options<T: DeserializeOwned>(&self, name: &str) -> Result<T> {
        let table = self.tool(name).cloned().unwrap_or_default();
        serde_json::from_value(Value::Object(table))
            .map_err(|e| ToolError::invalid(format!("[tools.{name}]: {e}")))
    }

    /// Copies `[tools.<name>]` settings into `options` (a JSON object), skipping
    /// any key for which `keep(key)` returns true.
    pub fn merge_into(&self, name: &str, options: &mut Value, keep: impl Fn(&str) -> bool) {
        let (Some(table), Some(target)) = (self.tool(name), options.as_object_mut()) else {
            return;
        };
        for (key, value) in table {
            if !keep(key) {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

/// `dead-code`, `dead_code` → `dead_code`.
fn normalize(name: &str) -> String {
    name.replace('-', "_").to_lowercase()
}

fn normalize_keys(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (normalize(&k), v)).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tool_sections_and_ignores_others() {
        let cfg = ToolsConfig::from_toml_str(
            "[database]\nurl = 'x'\n[tools.loc]\nmax-lines = 400\n[tools.dead-code]\ninclude = ['rs']\n",
        )
        .unwrap();
        assert_eq!(cfg.tool("loc").unwrap()["max_lines"], 400);
        assert!(cfg.tool("dead_code").is_some());
        assert!(cfg.tool("grep").is_none());
    }

    #[test]
    fn merge_keeps_explicit_keys() {
        let cfg =
            ToolsConfig::from_toml_str("[tools.loc]\nmax_lines = 400\ncheck = true\n").unwrap();
        let mut opts = serde_json::json!({ "max_lines": 100, "check": false });
        cfg.merge_into("loc", &mut opts, |k| k == "max_lines");
        assert_eq!(opts, serde_json::json!({ "max_lines": 100, "check": true }));
    }
}
