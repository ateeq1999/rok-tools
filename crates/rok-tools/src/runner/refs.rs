//! References between Steps and `{{…}}` substitution in Step arguments.

use super::payload::{StepRef, StepResult};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The result an earlier Step produced, by index or id.
pub(crate) fn lookup<'a>(
    results: &'a [Option<StepResult>],
    step: &StepRef,
) -> Option<&'a StepResult> {
    match step {
        StepRef::Index(i) => results.get(*i)?.as_ref(),
        StepRef::Id(id) => results.iter().flatten().find(|r| r.id == *id),
    }
}

/// The JSON a reference resolves against: the Tool output, or nested results for control flow.
fn view(result: &StepResult) -> Value {
    match &result.output {
        Some(Value::Object(m)) if result.results.is_empty() => Value::Object(m.clone()),
        Some(v) if result.results.is_empty() => v.clone(),
        output => {
            let mut m = serde_json::Map::new();
            if let Some(Value::Object(o)) = output {
                m.extend(o.clone());
            }
            m.insert(
                "results".into(),
                serde_json::to_value(&result.results).unwrap_or_default(),
            );
            Value::Object(m)
        }
    }
}

/// Picks `pick` (e.g. `matches[*].path`, `stdout`, `*`) out of an earlier Step's output.
pub(crate) fn resolve(results: &[Option<StepResult>], step: &StepRef, pick: &str) -> Option<Value> {
    let value = view(lookup(results, step)?);
    if pick.is_empty() || pick == "*" {
        return Some(value);
    }
    pick_path(&value, pick)
}

/// A small JSONPath subset: dotted keys, `[n]` indexes and `[*]` fan-out.
pub(crate) fn pick_path(value: &Value, path: &str) -> Option<Value> {
    let mut current = vec![value.clone()];
    let mut fanned = false;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let (key, brackets) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        let mut next = Vec::new();
        for v in &current {
            let v = if key.is_empty() { Some(v) } else { v.get(key) };
            if let Some(v) = v {
                next.push(v.clone());
            }
        }
        for index in brackets
            .split('[')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches(']'))
        {
            next = if index == "*" {
                fanned = true;
                next.iter()
                    .filter_map(Value::as_array)
                    .flatten()
                    .cloned()
                    .collect()
            } else {
                let i: usize = index.parse().ok()?;
                next.iter().filter_map(|v| v.get(i).cloned()).collect()
            };
        }
        current = next;
    }
    if fanned {
        Some(Value::Array(current))
    } else {
        current.into_iter().next()
    }
}

static ENV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*env\.(\w+)\s*\}\}").unwrap());

/// Replaces `{{env.NAME}}` (from `env`, then the process environment) in every string.
pub(crate) fn substitute_env(value: &mut Value, env: &BTreeMap<String, String>) {
    map_strings(value, &|s| {
        ENV.replace_all(s, |c: &regex::Captures| {
            env.get(&c[1])
                .cloned()
                .or_else(|| std::env::var(&c[1]).ok())
                .unwrap_or_else(|| c[0].to_string())
        })
        .into_owned()
    });
}

/// Replaces `{{var}}` with `value` in every string.
pub(crate) fn substitute_var(target: &mut Value, var: &str, value: &str) {
    let placeholder = format!("{{{{{var}}}}}");
    map_strings(target, &|s| s.replace(&placeholder, value));
}

fn map_strings(value: &mut Value, f: &dyn Fn(&str) -> String) {
    match value {
        Value::String(s) => *s = f(s),
        Value::Array(items) => items.iter_mut().for_each(|v| map_strings(v, f)),
        Value::Object(map) => map.values_mut().for_each(|v| map_strings(v, f)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_paths() {
        let v = json!({ "matches": [{ "path": "a" }, { "path": "b" }], "n": { "x": [1, 2] } });
        assert_eq!(pick_path(&v, "matches[*].path"), Some(json!(["a", "b"])));
        assert_eq!(pick_path(&v, "matches[1].path"), Some(json!("b")));
        assert_eq!(pick_path(&v, "n.x[0]"), Some(json!(1)));
        assert_eq!(pick_path(&v, "missing"), None);
    }

    #[test]
    fn substitutes_vars_everywhere() {
        let mut v = json!({ "path": "{{file}}", "nested": ["x/{{file}}"] });
        substitute_var(&mut v, "file", "a.rs");
        assert_eq!(v, json!({ "path": "a.rs", "nested": ["x/a.rs"] }));
    }

    #[test]
    fn substitutes_env() {
        let env = [("ROK_TEST_NAME".to_string(), "rok".to_string())]
            .into_iter()
            .collect();
        let mut v = json!({ "cmd": "echo {{env.ROK_TEST_NAME}} {{env.ROK_SURELY_UNSET_VAR}}" });
        substitute_env(&mut v, &env);
        assert_eq!(v["cmd"], "echo rok {{env.ROK_SURELY_UNSET_VAR}}");
    }
}
