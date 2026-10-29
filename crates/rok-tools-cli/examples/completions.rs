//! Generate shell completions for `rok-tools`.
//!
//! Usage: `cargo run -p rok-tools-cli --example completions -- <bash|zsh|fish|powershell|elvish>`

use clap_complete::{Shell, generate};

fn main() {
    let Some(shell) = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<Shell>().ok())
    else {
        eprintln!("usage: completions <bash|zsh|fish|powershell|elvish>");
        std::process::exit(2);
    };
    generate(
        shell,
        &mut rok_tools_cli::command(),
        "rok-tools",
        &mut std::io::stdout(),
    );
}
