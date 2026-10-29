//! `lint`: run the project's linter (eslint, biome, clippy or ruff) and collect diagnostics.

use crate::tool::{Context, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Command;

/// Which linter to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum Linter {
    /// Detect from config files in the working directory.
    #[default]
    Auto,
    /// `npx eslint`.
    Eslint,
    /// `npx @biomejs/biome lint`.
    Biome,
    /// `cargo clippy`.
    Clippy,
    /// `ruff check`.
    Ruff,
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct LintOptions {
    /// Path to lint.
    #[serde(default = "crate::defaults::dot")]
    #[cfg_attr(feature = "cli", arg(default_value = "."))]
    pub path: String,
    /// Linter to run.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t = Linter::Auto))]
    pub tool: Linter,
}

/// One diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintDiagnostic {
    /// File the diagnostic is in.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// Rule or lint code.
    pub rule: String,
    /// Message.
    pub message: String,
    /// `error` or `warning`.
    pub severity: String,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintOutput {
    /// The linter that ran.
    pub tool: Linter,
    /// Number of errors.
    pub errors_count: usize,
    /// Number of warnings.
    pub warnings_count: usize,
    /// Every diagnostic.
    pub diagnostics: Vec<LintDiagnostic>,
}

/// The `lint` Tool.
#[derive(Debug)]
pub struct Lint;

impl Tool for Lint {
    const NAME: &'static str = "lint";
    const ABOUT: &'static str = "Run the project's linter and collect diagnostics";
    const GROUP: &'static str = "code";
    type Options = LintOptions;
    type Output = LintOutput;

    fn run(o: &LintOptions, cx: &Context) -> Result<LintOutput> {
        run(o, cx)
    }

    fn succeeded(out: &LintOutput) -> bool {
        out.errors_count == 0
    }

    fn to_text(out: &LintOutput) -> String {
        let mut s: String = out
            .diagnostics
            .iter()
            .map(|d| {
                format!(
                    "{}:{}: {} [{}] {}\n",
                    d.file, d.line, d.severity, d.rule, d.message
                )
            })
            .collect();
        s.push_str(&format!(
            "{} error(s), {} warning(s)\n",
            out.errors_count, out.warnings_count
        ));
        s
    }
}

/// Lints `o.path`.
pub fn run(o: &LintOptions, cx: &Context) -> Result<LintOutput> {
    let tool = if o.tool == Linter::Auto {
        detect(cx)
    } else {
        o.tool
    };
    let (program, args): (&str, Vec<&str>) = match tool {
        Linter::Eslint => ("npx", vec!["eslint", &o.path, "--format", "json"]),
        Linter::Biome => (
            "npx",
            vec!["@biomejs/biome", "lint", &o.path, "--reporter=json"],
        ),
        Linter::Clippy => ("cargo", vec!["clippy", "--message-format=json"]),
        Linter::Ruff => ("ruff", vec!["check", &o.path, "--output-format", "json"]),
        Linter::Auto => return Err(ToolError::Failed("no linter configuration found".into())),
    };
    let output = Command::new(program)
        .args(&args)
        .current_dir(&cx.cwd)
        .envs(&cx.env)
        .output()
        .map_err(|e| ToolError::Command {
            command: program.to_string(),
            message: e.to_string(),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics = match tool {
        Linter::Clippy => parse_clippy(&stdout),
        Linter::Eslint => parse_eslint(&stdout),
        Linter::Biome => parse_biome(&stdout),
        _ => parse_ruff(&stdout),
    };
    let errors_count = diagnostics.iter().filter(|d| d.severity == "error").count();
    Ok(LintOutput {
        tool,
        errors_count,
        warnings_count: diagnostics.len() - errors_count,
        diagnostics,
    })
}

fn detect(cx: &Context) -> Linter {
    let has = |name: &str| cx.cwd.join(name).exists();
    if has("biome.json") || has("biome.jsonc") {
        Linter::Biome
    } else if [
        ".eslintrc.json",
        ".eslintrc.js",
        "eslint.config.js",
        "eslint.config.mjs",
    ]
    .iter()
    .any(|f| has(f))
    {
        Linter::Eslint
    } else if has("ruff.toml") || has("pyproject.toml") {
        Linter::Ruff
    } else if has("Cargo.toml") {
        Linter::Clippy
    } else {
        Linter::Auto
    }
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter()
        .try_fold(v, |v, k| v.get(k))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn num_at(v: &Value, path: &[&str]) -> usize {
    path.iter()
        .try_fold(v, |v, k| v.get(k))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn diag(file: &str, line: usize, rule: &str, message: &str, error: bool) -> LintDiagnostic {
    LintDiagnostic {
        file: file.to_string(),
        line,
        rule: rule.to_string(),
        message: message.to_string(),
        severity: if error { "error" } else { "warning" }.to_string(),
    }
}

fn parse_eslint(stdout: &str) -> Vec<LintDiagnostic> {
    let json: Value = serde_json::from_str(stdout).unwrap_or_default();
    let mut out = Vec::new();
    for file in json.as_array().into_iter().flatten() {
        let path = str_at(file, &["filePath"]);
        for m in file
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let error = num_at(m, &["severity"]) >= 2;
            out.push(diag(
                path,
                num_at(m, &["line"]),
                str_at(m, &["ruleId"]),
                str_at(m, &["message"]),
                error,
            ));
        }
    }
    out
}

fn parse_biome(stdout: &str) -> Vec<LintDiagnostic> {
    let json: Value = serde_json::from_str(stdout).unwrap_or_default();
    let list = json.get("diagnostics").and_then(Value::as_array);
    list.into_iter()
        .flatten()
        .map(|d| {
            let file = str_at(d, &["location", "path", "file"]);
            let error = str_at(d, &["severity"]) == "error";
            diag(
                file,
                num_at(d, &["location", "span", "start", "line"]),
                str_at(d, &["category"]),
                str_at(d, &["description"]),
                error,
            )
        })
        .collect()
}

fn parse_clippy(stdout: &str) -> Vec<LintDiagnostic> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|j| str_at(j, &["reason"]) == "compiler-message")
        .filter_map(|j| {
            let msg = j.get("message")?;
            let span = msg
                .get("spans")?
                .as_array()?
                .iter()
                .find(|s| s["is_primary"] == true)?;
            let error = str_at(msg, &["level"]) == "error";
            Some(diag(
                str_at(span, &["file_name"]),
                num_at(span, &["line_start"]),
                str_at(msg, &["code", "code"]),
                str_at(msg, &["message"]),
                error,
            ))
        })
        .collect()
}

fn parse_ruff(stdout: &str) -> Vec<LintDiagnostic> {
    let json: Value = serde_json::from_str(stdout).unwrap_or_default();
    json.as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            diag(
                str_at(v, &["filename"]),
                num_at(v, &["location", "row"]),
                str_at(v, &["code"]),
                str_at(v, &["message"]),
                true,
            )
        })
        .collect()
}
