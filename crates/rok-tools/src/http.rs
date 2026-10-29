//! `http`: make an HTTP request and check its status.
//!
//! Opt-in only: this Tool belongs to no Tool group, and the Runner refuses it
//! unless the Payload sets `allow_net: true`.

use crate::kv::{self, KeyValue};
use crate::tool::{Context, Permission, Tool};
use crate::{Result, ToolError};
use serde::{Deserialize, Serialize};

fn default_method() -> String {
    "GET".to_string()
}

fn default_status() -> u16 {
    200
}

/// Options for [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct HttpOptions {
    /// URL to request.
    pub url: String,
    /// HTTP method.
    #[serde(default = "default_method")]
    #[cfg_attr(feature = "cli", arg(long, short = 'X', default_value = "GET"))]
    pub method: String,
    /// Request headers, as `NAME=VALUE` (or `NAME:VALUE`).
    #[serde(default, with = "kv::as_map")]
    #[cfg_attr(feature = "cli", arg(long = "header", short = 'H'))]
    pub headers: Vec<KeyValue>,
    /// Request body.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long, short = 'd'))]
    pub body: Option<String>,
    /// Status code that counts as success.
    #[serde(default = "default_status")]
    #[cfg_attr(feature = "cli", arg(long, default_value_t = 200))]
    pub expect_status: u16,
}

/// Output of [`run`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpOutput {
    /// Method used.
    pub method: String,
    /// URL requested.
    pub url: String,
    /// Response status code.
    pub status: u16,
    /// Expected status code.
    pub expect_status: u16,
    /// Response body, when it was text.
    pub body: Option<String>,
}

/// The `http` Tool.
#[derive(Debug)]
pub struct Http;

impl Tool for Http {
    const NAME: &'static str = "http";
    const ABOUT: &'static str = "Make an HTTP request and check its status";
    const GROUP: &'static str = "opt-in";
    const PERMISSION: Option<Permission> = Some(Permission::Net);
    type Options = HttpOptions;
    type Output = HttpOutput;

    fn run(o: &HttpOptions, cx: &Context) -> Result<HttpOutput> {
        run(o, cx)
    }

    fn succeeded(out: &HttpOutput) -> bool {
        out.status == out.expect_status
    }

    fn to_text(out: &HttpOutput) -> String {
        format!(
            "{} {} -> {}\n{}",
            out.method,
            out.url,
            out.status,
            out.body.as_deref().unwrap_or("")
        )
    }
}

/// Sends the request described by `o`.
pub fn run(o: &HttpOptions, _cx: &Context) -> Result<HttpOutput> {
    let method = reqwest::Method::from_bytes(o.method.to_uppercase().as_bytes())
        .map_err(|_| ToolError::invalid(format!("invalid HTTP method `{}`", o.method)))?;
    let client = reqwest::blocking::Client::new();
    let mut request = client.request(method.clone(), &o.url);
    for h in &o.headers {
        request = request.header(&h.key, &h.value);
    }
    if let Some(body) = &o.body {
        request = request.body(body.clone());
    }
    let response = request
        .send()
        .map_err(|e| ToolError::Failed(format!("{} {}: {e}", method, o.url)))?;
    let status = response.status().as_u16();
    Ok(HttpOutput {
        method: method.to_string(),
        url: o.url.clone(),
        status,
        expect_status: o.expect_status,
        body: response.text().ok(),
    })
}
