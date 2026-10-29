//! Search-pattern compilation shared by `grep`, `replace` and `refactor`.

use crate::{Result, ToolError};
use regex::{Regex, RegexBuilder};

/// Compiles `pattern`: literal unless `regex`, optionally whole-word and case-insensitive.
pub(crate) fn compile(
    pattern: &str,
    regex: bool,
    whole_word: bool,
    ignore_case: bool,
) -> Result<Regex> {
    if pattern.is_empty() {
        return Err(ToolError::invalid("pattern must not be empty"));
    }
    let body = if regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    let body = if whole_word {
        format!(r"\b(?:{body})\b")
    } else {
        body
    };
    RegexBuilder::new(&body)
        .case_insensitive(ignore_case)
        .build()
        .map_err(|e| ToolError::invalid(format!("invalid pattern `{pattern}`: {e}")))
}
