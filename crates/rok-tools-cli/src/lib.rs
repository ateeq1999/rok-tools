//! The rok-tools command line, usable as the `rok-tools` binary or embedded
//! (e.g. as `rok tools …` in rok-data's CLI) through [`run_from`].
//!
//! Every Tool compiled into rok-tools becomes a subcommand. Settings come from
//! the command line first, then `[tools.<tool>]` in the nearest `rok.toml`,
//! then the Tool's defaults. Exit codes: 0 success, 1 a Tool failed or a
//! check found violations, 2 a usage or config error.

use clap::parser::ValueSource;
use clap::{Arg, ArgAction, ArgMatches, Command};
use rok_tools::config::ToolsConfig;
use rok_tools::{Context, ToolError};
use std::ffi::OsString;
use std::path::PathBuf;

/// Exit code for success.
pub const EXIT_OK: i32 = 0;
/// Exit code when a Tool fails or a check reports violations.
pub const EXIT_FAILURE: i32 = 1;
/// Exit code for usage and config errors.
pub const EXIT_USAGE: i32 = 2;

/// The full clap command: global flags plus one subcommand per enabled Tool.
pub fn command() -> Command {
    let mut cmd = Command::new("rok-tools")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Developer tools: search, edit, scan, refactor, count lines of code, and more")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print the Tool's output as JSON (and nothing else) on stdout"),
        )
        .arg(
            Arg::new("cwd")
                .long("cwd")
                .short('C')
                .global(true)
                .value_name("DIR")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Run as if started in DIR"),
        )
        .arg(
            Arg::new("no_ignore")
                .long("no-ignore")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Do not respect .gitignore or skip target/, node_modules/ when walking"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .global(true)
                .value_name("FILE")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Read [tools.*] settings from FILE instead of the nearest rok.toml"),
        )
        .subcommand(Command::new("list").about("List the Tools in this build"));
    for entry in rok_tools::registry() {
        cmd = cmd.subcommand(entry.command());
    }
    #[cfg(feature = "runner")]
    {
        cmd = cmd.subcommand(run_cmd::command());
    }
    cmd
}

/// Parses `args` (the first item is the program name) and runs; returns the exit code.
pub fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = match command().try_get_matches_from(args) {
        Ok(m) => m,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    match dispatch(&matches) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            match e {
                ToolError::InvalidOptions(_) | ToolError::NotEnabled(_) => EXIT_USAGE,
                _ => EXIT_FAILURE,
            }
        }
    }
}

fn context(m: &ArgMatches) -> Result<Context, ToolError> {
    let mut cx = match m.get_one::<PathBuf>("cwd") {
        Some(dir) => Context::new(dir)?,
        None => Context::current()?,
    };
    cx.respect_ignore = !m.get_flag("no_ignore");
    Ok(cx)
}

fn dispatch(m: &ArgMatches) -> Result<i32, ToolError> {
    let (name, sub) = m.subcommand().expect("subcommand_required");
    let json = sub.get_flag("json");
    let cx = context(sub)?;
    if name == "list" {
        return Ok(list(json));
    }
    #[cfg(feature = "runner")]
    if name == run_cmd::NAME {
        return run_cmd::run(sub, cx, json);
    }
    let config = match sub.get_one::<PathBuf>("config") {
        Some(file) => ToolsConfig::from_file(file)?,
        None => ToolsConfig::load(&cx.cwd)?,
    };
    let entry = rok_tools::find(name)?;
    let mut options = entry.options_from_matches(sub)?;
    let arg_ids: Vec<String> = entry
        .command()
        .get_arguments()
        .map(|a| a.get_id().to_string())
        .collect();
    config.merge_into(entry.name, &mut options, |key| {
        arg_ids.iter().any(|id| id == key)
            && sub.value_source(key) == Some(ValueSource::CommandLine)
    });
    let outcome = entry.run_json(options, &cx)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&outcome.output).unwrap_or_default()
        );
    } else {
        print_text(&outcome.text);
    }
    Ok(if outcome.succeeded {
        EXIT_OK
    } else {
        EXIT_FAILURE
    })
}

fn print_text(text: &str) {
    if text.ends_with('\n') || text.is_empty() {
        print!("{text}");
    } else {
        println!("{text}");
    }
}

fn list(json: bool) -> i32 {
    let enabled = rok_tools::registry();
    let rows: Vec<serde_json::Value> = rok_tools::ALL_TOOLS
        .iter()
        .map(|name| {
            let entry = enabled.iter().find(|e| e.name == *name);
            serde_json::json!({
                "name": name,
                "enabled": entry.is_some(),
                "group": entry.map(|e| e.group),
                "about": entry.map(|e| e.about),
            })
        })
        .collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
    } else {
        for e in &enabled {
            println!("{:<12} {:<9} {}", e.name, e.group, e.about);
        }
        let missing: Vec<_> = rok_tools::ALL_TOOLS
            .iter()
            .filter(|n| !enabled.iter().any(|e| e.name == **n))
            .collect();
        if !missing.is_empty() {
            println!(
                "\nnot in this build: {}",
                missing
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    EXIT_OK
}

#[cfg(feature = "runner")]
mod run_cmd {
    use super::*;
    use rok_tools::runner::{Payload, Runner};
    use std::io::Read;

    pub(crate) const NAME: &str = "run";

    pub(crate) fn command() -> Command {
        Command::new(NAME)
            .about("Run a JSON Payload of Steps")
            .arg(
                Arg::new("file")
                    .long("file")
                    .short('f')
                    .value_name("FILE")
                    .conflicts_with("payload")
                    .help("Read the Payload from FILE (default: stdin)"),
            )
            .arg(
                Arg::new("payload")
                    .long("payload")
                    .short('p')
                    .value_name("JSON")
                    .help("The Payload as an inline JSON string"),
            )
            .arg(
                Arg::new("dry_run")
                    .long("dry-run")
                    .action(ArgAction::SetTrue)
                    .help("Print the Steps in execution order without running them"),
            )
    }

    pub(crate) fn run(m: &ArgMatches, cx: Context, json: bool) -> Result<i32, ToolError> {
        let text = if let Some(inline) = m.get_one::<String>("payload") {
            inline.clone()
        } else if let Some(file) = m.get_one::<String>("file") {
            std::fs::read_to_string(cx.resolve(file)).map_err(|e| ToolError::io("read", file, e))?
        } else {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| ToolError::io("read", "<stdin>", e))?;
            buf
        };
        if text.trim().is_empty() {
            return Err(ToolError::invalid(
                "no Payload given (use --file, --payload or stdin)",
            ));
        }
        let payload = Payload::from_json(&text)?;
        let respect_ignore = cx.respect_ignore;
        let mut runner_cx = Context::new(cx.resolve(&payload.options.cwd))?;
        runner_cx.env = payload.options.env.clone();
        runner_cx.respect_ignore = respect_ignore;
        let runner = Runner::with_context(payload, runner_cx);
        if m.get_flag("dry_run") {
            let plan = runner.plan();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&plan).unwrap_or_default()
                );
            } else {
                println!("dry run: {} step(s)", plan.len());
                plan.iter().for_each(|line| println!("  {line}"));
            }
            return Ok(EXIT_OK);
        }
        let output = runner.run();
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            print_text(&output.to_text());
        }
        Ok(if output.is_ok() {
            EXIT_OK
        } else {
            EXIT_FAILURE
        })
    }
}
