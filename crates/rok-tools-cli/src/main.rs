//! The `rok-tools` binary.

fn main() {
    std::process::exit(rok_tools_cli::run_from(std::env::args_os()));
}
