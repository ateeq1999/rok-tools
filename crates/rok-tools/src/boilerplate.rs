//! `boilerplate`: add a shebang, header comment or license notice to a file.

use crate::Result;
use crate::tool::{Context, Tool, extension, read_to_string, write_file};
use serde::{Deserialize, Serialize};

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct BoilerplateOptions {
    /// File to edit.
    pub path: String,
    /// Header text, wrapped in the file type's comment syntax.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long = "header"))]
    pub add_header: Option<String>,
    /// License notice: `mit`, `apache`, `gpl`, `bsd`, `isc`, `unlicense`, or literal text.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long = "license"))]
    pub add_license: Option<String>,
    /// Shebang line to add when the file has none (e.g. `#!/usr/bin/env bash`).
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long = "shebang"))]
    pub add_shebang: Option<String>,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoilerplateOutput {
    /// The edited file.
    pub path: String,
    /// Whether a header was added.
    pub header_added: bool,
    /// Whether a license notice was added.
    pub license_added: bool,
    /// Whether a shebang was added.
    pub shebang_added: bool,
}

/// The `boilerplate` Tool.
#[derive(Debug)]
pub struct Boilerplate;

impl Tool for Boilerplate {
    const NAME: &'static str = "boilerplate";
    const ABOUT: &'static str = "Add a shebang, header comment or license notice";
    const GROUP: &'static str = "code";
    type Options = BoilerplateOptions;
    type Output = BoilerplateOutput;

    fn run(o: &BoilerplateOptions, cx: &Context) -> Result<BoilerplateOutput> {
        run(o, cx)
    }
}

/// Adds the requested boilerplate to `o.path`; anything already present is left alone.
pub fn run(o: &BoilerplateOptions, cx: &Context) -> Result<BoilerplateOutput> {
    let full = cx.resolve(&o.path);
    let original = read_to_string(&full)?;
    let ext = extension(&full);
    let (shebang, mut body) = match original.strip_prefix("#!") {
        Some(_) => {
            let (first, rest) = original.split_once('\n').unwrap_or((&original, ""));
            (Some(first.to_string()), rest.to_string())
        }
        None => (None, original.clone()),
    };
    let mut out = BoilerplateOutput {
        path: o.path.clone(),
        header_added: false,
        license_added: false,
        shebang_added: false,
    };
    // License first so that a header ends up above it.
    if let Some(license) = &o.add_license {
        let comment = comment(&license_text(license), &ext);
        if !body.contains(&comment) {
            body = format!("{comment}\n{body}");
            out.license_added = true;
        }
    }
    if let Some(header) = &o.add_header {
        let comment = comment(header, &ext);
        if !body.contains(&comment) {
            body = format!("{comment}\n{body}");
            out.header_added = true;
        }
    }
    let shebang = match (shebang, &o.add_shebang) {
        (Some(existing), _) => Some(existing),
        (None, Some(new)) => {
            out.shebang_added = true;
            Some(new.clone())
        }
        (None, None) => None,
    };
    let content = match shebang {
        Some(s) => format!("{s}\n{body}"),
        None => body,
    };
    if content != original {
        write_file(&full, &content)?;
    }
    Ok(out)
}

fn comment(text: &str, ext: &str) -> String {
    let prefixed = |p: &str| {
        text.lines()
            .map(|l| format!("{p}{l}").trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    match ext {
        "py" | "sh" | "bash" | "rb" | "toml" | "yaml" | "yml" => prefixed("# "),
        "lua" | "sql" => prefixed("-- "),
        "html" | "xml" | "md" => format!("<!--\n{text}\n-->"),
        "css" | "scss" | "less" => format!("/*\n{}\n */", prefixed(" * ")),
        _ if text.lines().count() > 1 => format!("/*\n{}\n */", prefixed(" * ")),
        _ => format!("// {text}"),
    }
}

fn license_text(license: &str) -> String {
    match license.to_lowercase().as_str() {
        "mit" => "SPDX-License-Identifier: MIT".to_string(),
        "apache" | "apache-2.0" => "SPDX-License-Identifier: Apache-2.0".to_string(),
        "gpl" | "gpl-3.0" => "SPDX-License-Identifier: GPL-3.0-or-later".to_string(),
        "bsd" | "bsd-3-clause" => "SPDX-License-Identifier: BSD-3-Clause".to_string(),
        "isc" => "SPDX-License-Identifier: ISC".to_string(),
        "unlicense" => "SPDX-License-Identifier: Unlicense".to_string(),
        _ => license.to_string(),
    }
}
