//! Control-flow Steps: `if`, `each`, `parallel` and `checkpoint`.

use super::payload::{Condition, EachOver, Step, StepResult, StepStatus};
use super::{Runner, refs, state};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct IfArgs {
    condition: Condition,
    #[serde(default)]
    then: Vec<Step>,
    #[serde(default, rename = "else")]
    otherwise: Vec<Step>,
}

#[derive(Deserialize)]
struct EachArgs {
    over: EachOver,
    #[serde(default = "default_as", rename = "as")]
    var: String,
    #[serde(default = "crate::defaults::yes")]
    parallel: bool,
    step: Box<Step>,
}

fn default_as() -> String {
    "item".to_string()
}

#[derive(Deserialize)]
struct ParallelArgs {
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct CheckpointArgs {
    checkpoint_id: String,
}

fn parse<T: for<'de> Deserialize<'de>>(
    step: &Step,
    index: usize,
    args: Value,
) -> Result<T, Box<StepResult>> {
    serde_json::from_value(args).map_err(|e| {
        Box::new(StepResult::failed(
            index,
            step,
            format!("invalid `{}` step: {e}", step.kind),
        ))
    })
}

/// Ok when every nested Step is ok (or skipped), otherwise Partial.
fn summarize(index: usize, step: &Step, output: Value, results: Vec<StepResult>) -> StepResult {
    let all_ok = results
        .iter()
        .all(|r| matches!(r.status, StepStatus::Ok | StepStatus::Skipped));
    StepResult {
        output: Some(output),
        results,
        ..StepResult::new(
            index,
            step,
            if all_ok {
                StepStatus::Ok
            } else {
                StepStatus::Partial
            },
        )
    }
}

impl Runner {
    pub(super) fn run_if(
        &self,
        step: &Step,
        index: usize,
        args: Value,
        prev: &[Option<StepResult>],
    ) -> StepResult {
        let args: IfArgs = match parse(step, index, args) {
            Ok(a) => a,
            Err(r) => return *r,
        };
        let met = self.eval(&args.condition, prev);
        let branch = if met { &args.then } else { &args.otherwise };
        let results = branch
            .iter()
            .enumerate()
            .map(|(i, s)| self.execute(s, i, prev))
            .collect();
        let output = json!({ "condition_met": met, "branch": if met { "then" } else { "else" } });
        summarize(index, step, output, results)
    }

    pub(super) fn run_each(
        &self,
        step: &Step,
        index: usize,
        args: Value,
        prev: &[Option<StepResult>],
    ) -> StepResult {
        let args: EachArgs = match parse(step, index, args) {
            Ok(a) => a,
            Err(r) => return *r,
        };
        let items: Vec<String> = match &args.over {
            EachOver::List(items) => items.clone(),
            EachOver::Ref { step: r, pick } => match refs::resolve(prev, r, pick) {
                Some(Value::Array(values)) => values
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect(),
                Some(other) => {
                    return StepResult::failed(
                        index,
                        step,
                        format!("`each.over` resolved to a non-list: {other}"),
                    );
                }
                None => {
                    return StepResult::failed(
                        index,
                        step,
                        "`each.over` reference resolved to nothing",
                    );
                }
            },
        };
        let run_one = |(i, item): (usize, &String)| {
            let mut inner = serde_json::to_value(&*args.step).unwrap_or_default();
            refs::substitute_var(&mut inner, &args.var, item);
            match serde_json::from_value::<Step>(inner) {
                Ok(s) => self.execute(&s, i, prev),
                Err(e) => StepResult::failed(i, &args.step, e.to_string()),
            }
        };
        let results: Vec<StepResult> = if args.parallel {
            items.par_iter().enumerate().map(run_one).collect()
        } else {
            items.iter().enumerate().map(run_one).collect()
        };
        summarize(index, step, json!({ "items": items }), results)
    }

    pub(super) fn run_parallel(
        &self,
        step: &Step,
        index: usize,
        args: Value,
        prev: &[Option<StepResult>],
    ) -> StepResult {
        let args: ParallelArgs = match parse(step, index, args) {
            Ok(a) => a,
            Err(r) => return *r,
        };
        let results = args
            .steps
            .par_iter()
            .enumerate()
            .map(|(i, s)| self.execute(s, i, prev))
            .collect();
        summarize(index, step, json!({}), results)
    }

    pub(super) fn run_checkpoint(
        &self,
        step: &Step,
        index: usize,
        args: Value,
        prev: &[Option<StepResult>],
    ) -> StepResult {
        let args: CheckpointArgs = match parse(step, index, args) {
            Ok(a) => a,
            Err(r) => return *r,
        };
        match state::save_checkpoint(&self.cx.cwd, &args.checkpoint_id, prev) {
            Ok(cp) => StepResult {
                output: Some(
                    json!({ "checkpoint_id": cp.checkpoint_id, "steps_saved": cp.completed.len() }),
                ),
                ..StepResult::new(index, step, StepStatus::Ok)
            },
            Err(e) => StepResult::failed(index, step, e.to_string()),
        }
    }

    fn eval(&self, condition: &Condition, prev: &[Option<StepResult>]) -> bool {
        match condition {
            Condition::Exists { path } => self.cx.resolve(path).exists(),
            Condition::Contains {
                path,
                pattern,
                regex,
            } => {
                let Ok(content) = std::fs::read_to_string(self.cx.resolve(path)) else {
                    return false;
                };
                if *regex {
                    regex::Regex::new(pattern).is_ok_and(|re| re.is_match(&content))
                } else {
                    content.contains(pattern.as_str())
                }
            }
            Condition::GrepHasResults { step } => refs::resolve(prev, step, "matches")
                .and_then(|m| m.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false),
            Condition::StepOk { step } => refs::lookup(prev, step).is_some_and(StepResult::is_ok),
            Condition::StepFailed { step } => refs::lookup(prev, step)
                .is_some_and(|r| matches!(r.status, StepStatus::Error | StepStatus::Partial)),
            Condition::FileChanged { path, since } => {
                let Ok(since) = chrono::DateTime::parse_from_rfc3339(since) else {
                    return false;
                };
                std::fs::metadata(self.cx.resolve(path))
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| chrono::DateTime::<chrono::Utc>::from(t) > since)
            }
            Condition::Not { condition } => !self.eval(condition, prev),
            Condition::And { conditions } => conditions.iter().all(|c| self.eval(c, prev)),
            Condition::Or { conditions } => conditions.iter().any(|c| self.eval(c, prev)),
        }
    }
}
