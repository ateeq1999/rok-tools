//! The Payload Runner: executes a Payload's Steps through the Tool registry.
//!
//! ```no_run
//! use rok_tools::runner::{Payload, Runner};
//!
//! let payload = Payload::from_json(r#"{"steps":[{"type":"grep","pattern":"TODO","path":"src"}]}"#)?;
//! let output = Runner::new(payload)?.run();
//! println!("{} of {} steps ok", output.steps_ok, output.steps_total);
//! # Ok::<(), rok_tools::ToolError>(())
//! ```
//!
//! Besides Tool Steps, a Payload may use `if`, `each`, `parallel` and
//! `checkpoint`. Steps run in Payload order, after any `depends_on`; a Step
//! whose dependency failed is skipped. `{{env.NAME}}` is substituted in every
//! string argument. `bash` and `http` Steps need `allowShell` / `allowNet`.

mod control;
mod payload;
mod refs;
mod state;

pub use payload::{
    Condition, EachOver, Payload, RetryConfig, RunOptions, RunOutput, Step, StepRef, StepResult,
    StepStatus,
};
pub use state::{Checkpoint, HistoryEntry, history, load_checkpoint};

use crate::tool::{Context, Permission};
use crate::{Result, ToolError};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Control-flow Step types handled by the Runner itself.
pub const CONTROL_STEPS: &[&str] = &["if", "each", "parallel", "checkpoint"];

/// Executes a [`Payload`].
#[derive(Debug)]
pub struct Runner {
    payload: Payload,
    cx: Context,
    cache: Option<state::Cache>,
    incremental: Option<Mutex<state::Incremental>>,
}

impl Runner {
    /// A Runner whose `options.cwd` is resolved against the process's current directory.
    pub fn new(payload: Payload) -> Result<Self> {
        let base = std::env::current_dir().map_err(|e| ToolError::io("read", ".", e))?;
        Self::in_dir(payload, &base)
    }

    /// A Runner whose `options.cwd` is resolved against `base`.
    pub fn in_dir(payload: Payload, base: &Path) -> Result<Self> {
        let mut cx = Context::new(base.join(&payload.options.cwd))?;
        cx.env = payload.options.env.clone();
        Ok(Self::with_context(payload, cx))
    }

    /// A Runner using `cx` as-is (`options.cwd` and `options.env` are ignored).
    pub fn with_context(payload: Payload, cx: Context) -> Self {
        let o = &payload.options;
        let cache = o.cache.then(|| {
            let dir = o
                .cache_dir
                .as_ref()
                .map_or_else(|| cx.cwd.join(".rok").join("cache"), PathBuf::from);
            state::Cache::new(if dir.is_absolute() {
                dir
            } else {
                cx.cwd.join(dir)
            })
        });
        let incremental = o
            .incremental
            .then(|| Mutex::new(state::Incremental::load(&cx.cwd)));
        Self {
            payload,
            cx,
            cache,
            incremental,
        }
    }

    /// The context Steps run in.
    pub fn context(&self) -> &Context {
        &self.cx
    }

    /// One line per top-level Step, in execution order, without running anything.
    pub fn plan(&self) -> Vec<String> {
        let order = self
            .order()
            .unwrap_or_else(|_| (0..self.payload.steps.len()).collect());
        order
            .into_iter()
            .map(|i| {
                let s = &self.payload.steps[i];
                let id = if s.id.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s.id)
                };
                format!("{i}: {}{id} {}", s.kind, Value::Object(s.args.clone()))
            })
            .collect()
    }

    /// Runs every Step and reports the outcome.
    pub fn run(&self) -> RunOutput {
        let start = Instant::now();
        let steps = &self.payload.steps;
        let mut results: Vec<Option<StepResult>> = vec![None; steps.len()];
        if let Some(id) = &self.payload.options.resume {
            match state::load_checkpoint(&self.cx.cwd, id) {
                Ok(cp) => cp
                    .completed
                    .into_iter()
                    .filter(|(i, _)| *i < steps.len())
                    .for_each(|(i, r)| results[i] = Some(r)),
                Err(e) => results
                    .iter_mut()
                    .zip(steps)
                    .enumerate()
                    .for_each(|(i, (r, s))| {
                        *r = Some(StepResult::failed(i, s, e.to_string()));
                    }),
            }
        }
        let order = match self.order() {
            Ok(order) => order,
            Err(e) => {
                for (i, s) in steps.iter().enumerate() {
                    results[i] = Some(StepResult::failed(i, s, e.to_string()));
                }
                Vec::new()
            }
        };
        let ids: BTreeMap<&str, usize> = steps
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.id.is_empty())
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let mut stopped = false;
        for i in order {
            if results[i].is_some() {
                continue; // restored from a Checkpoint
            }
            let step = &steps[i];
            let failed_dep = step
                .depends_on
                .iter()
                .find(|d| results[ids[d.as_str()]].as_ref().is_none_or(|r| !r.is_ok()));
            let result = if stopped {
                StepResult::skipped(i, step, "skipped after an earlier failure (stopOnError)")
            } else if let Some(dep) = failed_dep {
                StepResult::skipped(i, step, format!("dependency `{dep}` did not succeed"))
            } else {
                self.execute(step, i, &results)
            };
            if result.status == StepStatus::Error || result.status == StepStatus::Partial {
                stopped |= self.payload.options.stop_on_error;
            }
            results[i] = Some(result);
        }
        if let Some(inc) = &self.incremental {
            inc.lock().unwrap_or_else(|e| e.into_inner()).save();
        }
        let results: Vec<StepResult> = results.into_iter().flatten().collect();
        let count = |s: StepStatus| results.iter().filter(|r| r.status == s).count();
        let steps_failed = count(StepStatus::Error) + count(StepStatus::Partial);
        let output = RunOutput {
            status: if steps_failed == 0 { "ok" } else { "partial" }.to_string(),
            steps_total: steps.len(),
            steps_ok: count(StepStatus::Ok),
            steps_failed,
            steps_skipped: count(StepStatus::Skipped),
            duration_ms: start.elapsed().as_millis() as u64,
            results,
        };
        if self.payload.options.history {
            state::record_history(&self.cx.cwd, &self.payload, &output);
        }
        output
    }

    /// Topological order over `depends_on`, ties broken by Payload order.
    fn order(&self) -> Result<Vec<usize>> {
        let steps = &self.payload.steps;
        let ids: BTreeMap<&str, usize> = steps
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.id.is_empty())
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let mut indegree = vec![0usize; steps.len()];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); steps.len()];
        for (i, s) in steps.iter().enumerate() {
            for dep in &s.depends_on {
                let &d = ids.get(dep.as_str()).ok_or_else(|| {
                    ToolError::invalid(format!("step {i} depends on unknown step id `{dep}`"))
                })?;
                indegree[i] += 1;
                dependents[d].push(i);
            }
        }
        let mut ready: BTreeSet<usize> = (0..steps.len()).filter(|&i| indegree[i] == 0).collect();
        let mut order = Vec::with_capacity(steps.len());
        while let Some(i) = ready.pop_first() {
            order.push(i);
            for &j in &dependents[i] {
                indegree[j] -= 1;
                if indegree[j] == 0 {
                    ready.insert(j);
                }
            }
        }
        if order.len() != steps.len() {
            return Err(ToolError::invalid("depends_on contains a cycle"));
        }
        Ok(order)
    }

    /// Runs one Step (Tool or control flow) and times it.
    pub(crate) fn execute(
        &self,
        step: &Step,
        index: usize,
        prev: &[Option<StepResult>],
    ) -> StepResult {
        let start = Instant::now();
        let mut args = Value::Object(step.args.clone());
        refs::substitute_env(&mut args, &self.cx.env);
        let mut result = match step.kind.as_str() {
            "if" => self.run_if(step, index, args, prev),
            "each" => self.run_each(step, index, args, prev),
            "parallel" => self.run_parallel(step, index, args, prev),
            "checkpoint" => self.run_checkpoint(step, index, args, prev),
            _ => self.run_tool(step, index, args),
        };
        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    fn run_tool(&self, step: &Step, index: usize, args: Value) -> StepResult {
        let entry = match crate::registry::find(&step.kind) {
            Ok(e) => e,
            Err(e) => return StepResult::failed(index, step, e.to_string()),
        };
        if let Some(p) = entry.permission {
            let granted = match p {
                Permission::Shell => self.payload.options.allow_shell,
                Permission::Net => self.payload.options.allow_net,
            };
            if !granted {
                let e = ToolError::PermissionDenied {
                    tool: entry.name.to_string(),
                    permission: p.option_name(),
                };
                return StepResult::failed(index, step, e.to_string());
            }
        }
        let path = args.get("path").and_then(Value::as_str).map(str::to_string);
        if let (Some(inc), Some(path)) = (&self.incremental, &path)
            && state::INCREMENTAL_TOOLS.contains(&step.kind.as_str())
            && !inc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .changed(&self.cx.cwd, path)
        {
            return StepResult::skipped(index, step, "incremental: no changes");
        }
        if let Some(cached) = self.cache.as_ref().and_then(|c| c.get(step, index)) {
            return cached;
        }
        let timeout = step.timeout_ms.or(self.payload.options.timeout_ms);
        let (attempts, mut delay, backoff) = step
            .retry
            .as_ref()
            .map_or((1, 0, false), |r| (r.count.max(1), r.delay_ms, r.backoff));
        let mut result = StepResult::new(index, step, StepStatus::Error);
        for attempt in 0..attempts {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(delay));
                if backoff {
                    delay = delay.saturating_mul(2);
                }
            }
            result = StepResult::new(index, step, StepStatus::Error);
            match self.call(entry, args.clone(), timeout) {
                Ok(outcome) => {
                    result.status = if outcome.succeeded {
                        StepStatus::Ok
                    } else {
                        StepStatus::Error
                    };
                    result.output = Some(outcome.output);
                    result.text = outcome.text;
                }
                Err(e) => result.error = Some(e.to_string()),
            }
            if result.is_ok() {
                break;
            }
        }
        if let Some(cache) = &self.cache {
            cache.put(step, index, &result);
        }
        result
    }

    fn call(
        &self,
        entry: crate::ToolEntry,
        args: Value,
        timeout: Option<u64>,
    ) -> Result<crate::JsonOutcome> {
        let Some(ms) = timeout else {
            return entry.run_json(args, &self.cx);
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let cx = self.cx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(entry.run_json(args, &cx));
        });
        rx.recv_timeout(Duration::from_millis(ms))
            .unwrap_or_else(|_| Err(ToolError::Failed(format!("timed out after {ms}ms"))))
    }
}

impl RunOutput {
    /// Human-readable summary, one line per Step plus each Tool's text output.
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "[{}] {} step(s): {} ok, {} failed, {} skipped in {}ms\n",
            self.status.to_uppercase(),
            self.steps_total,
            self.steps_ok,
            self.steps_failed,
            self.steps_skipped,
            self.duration_ms
        );
        fn walk(s: &mut String, r: &StepResult, depth: usize) {
            let pad = "  ".repeat(depth + 1);
            let status = format!("{:?}", r.status).to_lowercase();
            let id = if r.id.is_empty() {
                String::new()
            } else {
                format!(" ({})", r.id)
            };
            s.push_str(&format!(
                "{pad}{:>3} {:<8} {}{id} [{}ms]\n",
                r.index, status, r.kind, r.duration_ms
            ));
            if let Some(e) = &r.error {
                s.push_str(&format!("{pad}    {e}\n"));
            }
            for line in r.text.lines().take(20) {
                s.push_str(&format!("{pad}    {line}\n"));
            }
            r.results.iter().for_each(|n| walk(s, n, depth + 1));
        }
        self.results.iter().for_each(|r| walk(&mut s, r, 0));
        s
    }
}
