//! The Payload document: options, Steps, conditions, and the results the Runner reports.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// A JSON document listing Steps to execute.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Payload {
    /// Optional name, recorded in history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// How the Runner behaves.
    #[serde(default)]
    pub options: RunOptions,
    /// The Steps, in order.
    pub steps: Vec<Step>,
}

impl Payload {
    /// Parses a Payload from JSON text.
    pub fn from_json(text: &str) -> crate::Result<Self> {
        serde_json::from_str(text)
            .map_err(|e| crate::ToolError::invalid(format!("invalid payload: {e}")))
    }
}

/// Runner settings (`options` in a Payload). Keys are camelCase; snake_case is accepted too.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    /// Working directory, relative to where the Runner starts.
    #[serde(default = "crate::defaults::dot")]
    pub cwd: String,
    /// Stop at the first failed Step (remaining Steps are skipped).
    #[serde(default = "crate::defaults::yes", alias = "stop_on_error")]
    pub stop_on_error: bool,
    /// Default per-Step timeout in milliseconds.
    #[serde(default, alias = "timeout_ms")]
    pub timeout_ms: Option<u64>,
    /// Extra environment variables for Tools that spawn processes and for `{{env.NAME}}`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Reuse successful Step results from the cache directory.
    #[serde(default)]
    pub cache: bool,
    /// Cache directory (default `.rok/cache`).
    #[serde(default, alias = "cache_dir")]
    pub cache_dir: Option<String>,
    /// Skip file-reading Steps whose files have not changed since the last run.
    #[serde(default)]
    pub incremental: bool,
    /// Allow `bash` Steps.
    #[serde(default, alias = "allow_shell")]
    pub allow_shell: bool,
    /// Allow `http` Steps.
    #[serde(default, alias = "allow_net")]
    pub allow_net: bool,
    /// Resume from this Checkpoint: Steps it recorded as done are not run again.
    #[serde(default)]
    pub resume: Option<String>,
    /// Record the run in `.rok/history.json`.
    #[serde(default = "crate::defaults::yes")]
    pub history: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Map::new()))
            .expect("all RunOptions fields have defaults")
    }
}

/// How often and how patiently to retry a failed Step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryConfig {
    /// Maximum attempts.
    #[serde(default = "default_retry_count")]
    pub count: usize,
    /// Delay before the second attempt, in milliseconds.
    #[serde(default = "default_retry_delay", alias = "delay_ms")]
    pub delay_ms: u64,
    /// Double the delay after each attempt.
    #[serde(default)]
    pub backoff: bool,
}

fn default_retry_count() -> usize {
    3
}

fn default_retry_delay() -> u64 {
    1000
}

/// One invocation of a Tool (or a control-flow construct) inside a Payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// A Tool name, or `if`, `each`, `parallel`, `checkpoint`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Identifier other Steps can depend on or reference.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Ids of Steps that must finish successfully first.
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "dependsOn")]
    pub depends_on: Vec<String>,
    /// Retry policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
    /// Timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "timeoutMs")]
    pub timeout_ms: Option<u64>,
    /// The Tool's options (or the construct's arguments).
    #[serde(flatten)]
    pub args: Map<String, Value>,
}

impl Step {
    /// A Step running `kind` with `args` (a JSON object).
    pub fn new(kind: impl Into<String>, args: Value) -> Self {
        Self {
            kind: kind.into(),
            id: String::new(),
            depends_on: Vec::new(),
            retry: None,
            timeout_ms: None,
            args: match args {
                Value::Object(m) => m,
                _ => Map::new(),
            },
        }
    }

    /// Sets the Step's id.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
}

/// A reference to an earlier Step: its index, or its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StepRef {
    /// 0-based index into the Payload's `steps`.
    Index(usize),
    /// A Step's `id`.
    Id(String),
}

/// A test evaluated by an `if` Step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Condition {
    /// The path exists.
    Exists {
        /// Path to test.
        path: String,
    },
    /// The file contains a pattern.
    Contains {
        /// File to read.
        path: String,
        /// Text (or regex with `regex`) to look for.
        pattern: String,
        /// Treat `pattern` as a regular expression.
        #[serde(default)]
        regex: bool,
    },
    /// The referenced Step's output has a non-empty `matches` list (e.g. `grep`).
    GrepHasResults {
        /// The Step.
        #[serde(rename = "ref")]
        step: StepRef,
    },
    /// The referenced Step succeeded.
    StepOk {
        /// The Step.
        #[serde(rename = "ref")]
        step: StepRef,
    },
    /// The referenced Step failed.
    StepFailed {
        /// The Step.
        #[serde(rename = "ref")]
        step: StepRef,
    },
    /// The file was modified after `since` (RFC 3339).
    FileChanged {
        /// File to test.
        path: String,
        /// RFC 3339 time.
        since: String,
    },
    /// The inner condition is false.
    Not {
        /// Condition to negate.
        condition: Box<Condition>,
    },
    /// Every condition is true.
    And {
        /// Conditions.
        conditions: Vec<Condition>,
    },
    /// Any condition is true.
    Or {
        /// Conditions.
        conditions: Vec<Condition>,
    },
}

/// What an `each` Step iterates over.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EachOver {
    /// A literal list.
    List(Vec<String>),
    /// Values picked out of an earlier Step's output.
    Ref {
        /// The Step.
        #[serde(rename = "ref", alias = "ref_")]
        step: StepRef,
        /// Path into its output, e.g. `matches[*].path`.
        #[serde(default = "default_pick")]
        pick: String,
    },
}

fn default_pick() -> String {
    "*".to_string()
}

/// How a Step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    /// It succeeded.
    Ok,
    /// It failed.
    Error,
    /// A control-flow Step where some nested Steps failed.
    Partial,
    /// It did not run.
    Skipped,
}

/// The outcome of one Step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    /// Index within its list of Steps.
    pub index: usize,
    /// The Step's `type`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The Step's id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// How it ended.
    pub status: StepStatus,
    /// Wall-clock time.
    pub duration_ms: u64,
    /// The Tool's output (or the construct's summary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Why it failed or was skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Nested results of `if`, `each` and `parallel`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<StepResult>,
    /// Human-readable rendering of the output.
    #[serde(skip)]
    pub text: String,
}

impl StepResult {
    /// A result with no output yet.
    pub(crate) fn new(index: usize, step: &Step, status: StepStatus) -> Self {
        Self {
            index,
            kind: step.kind.clone(),
            id: step.id.clone(),
            status,
            duration_ms: 0,
            output: None,
            error: None,
            results: Vec::new(),
            text: String::new(),
        }
    }

    /// A skipped result with a reason.
    pub(crate) fn skipped(index: usize, step: &Step, reason: impl Into<String>) -> Self {
        Self {
            error: Some(reason.into()),
            ..Self::new(index, step, StepStatus::Skipped)
        }
    }

    /// A failed result with a reason.
    pub(crate) fn failed(index: usize, step: &Step, reason: impl Into<String>) -> Self {
        Self {
            error: Some(reason.into()),
            ..Self::new(index, step, StepStatus::Error)
        }
    }

    /// Whether the Step succeeded.
    pub fn is_ok(&self) -> bool {
        self.status == StepStatus::Ok
    }
}

/// The outcome of a whole run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    /// `ok` when no Step failed, otherwise `partial`.
    pub status: String,
    /// Number of top-level Steps.
    pub steps_total: usize,
    /// Steps that succeeded.
    pub steps_ok: usize,
    /// Steps that failed.
    pub steps_failed: usize,
    /// Steps that did not run.
    pub steps_skipped: usize,
    /// Wall-clock time.
    pub duration_ms: u64,
    /// One result per top-level Step, in Payload order.
    pub results: Vec<StepResult>,
}

impl RunOutput {
    /// Whether every Step succeeded or was skipped for a non-error reason.
    pub fn is_ok(&self) -> bool {
        self.steps_failed == 0
    }
}
