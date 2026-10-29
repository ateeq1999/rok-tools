# rok-tools

Developer tools (search, edit, scan, refactor, count lines of code, …) as
plain Rust functions you import one at a time, and as a command line:
the standalone `rok-tools` binary or `rok tools …` inside rok-data's `rok`.

## As a library

Every Tool is a module behind a cargo feature of the same name. Nothing is
enabled by default, so you compile only what you ask for:

```toml
[dependencies]
rok-tools = { version = "0.1", features = ["grep", "loc"] }
```

```rust
use rok_tools::{Context, grep, loc};

let cx = Context::new(".")?;

let todos = grep::run(&grep::GrepOptions::new("TODO", "src"), &cx)?;
for m in &todos.matches {
    println!("{}:{}: {}", m.path, m.line, m.text);
}

let mut opts = loc::LocOptions::new(".");
opts.max_lines = 600;
let report = loc::run(&opts, &cx)?;
assert!(report.violations.is_empty(), "files over the Line limit: {:?}", report.violations);
# Ok::<(), rok_tools::ToolError>(())
```

Each Tool takes typed options and returns a typed output or a `ToolError`.
Calls are synchronous; from async code, use `spawn_blocking`.

### Features

| Feature | Tools |
| --- | --- |
| `fs` | read, write, patch, mv, cp, rm, mkdir, snapshot, restore |
| `search` | grep, replace, scan |
| `code` | summarize, extract, diff, lint, import, refactor, deps, boilerplate, dead-code, loc |
| `template` | template |
| `vcs` | git |
| `bash`, `http` | opt-in only: in no group, enable them by name or with `full` |
| `full` | every Tool above |
| `runner` | the JSON Payload Runner (`rok_tools::runner`) |
| `config` | `ToolsConfig::load`, which reads `[tools.<tool>]` from `rok.toml` |
| `cli` | clap derives on every options struct (used by `rok-tools-cli`) |

The single-tool features (`grep`, `loc`, `dead-code`, …) are also available
individually. Tools that walk directories respect `.gitignore` and skip
`.git/`, `target/` and `node_modules/`. Set `Context::respect_ignore = false`
(`--no-ignore`) to walk everything.

## As a command line

```sh
cargo install rok-tools-cli          # the `rok-tools` binary, every Tool
rok-tools list
rok-tools grep TODO src --ext rs
rok-tools loc --check                # exit 1 if any file exceeds its Line limit
rok-tools refactor old_name new_name src --dry-run
rok-tools run -f payload.json        # run a JSON Payload of Steps
```

The same commands work as `rok tools <tool>` in rok-data's `rok` binary.

- `--json` prints only the Tool's output, as JSON, on stdout.
- `-C DIR` runs as if the command was started in `DIR`.
- Exit codes: `0` success, `1` a Tool failed or a check found violations,
  `2` a usage or config error.

### Configuration

Settings come from the command line first, then `[tools.<tool>]` in the
nearest `rok.toml` (the same file rok-data uses), then each Tool's defaults:

```toml
[tools.loc]
max_lines = 600                 # the Line limit; 0 disables it
exclude = ["**/generated/**"]

[[tools.loc.override]]
path = "src/schema.rs"
max_lines = 1500                # or 0 to exempt the file

[tools.grep]
ignore_case = true
```

### Payloads

```json
{
  "options": { "stopOnError": true, "allowShell": true },
  "steps": [
    { "type": "grep", "id": "todos", "pattern": "TODO", "path": "src" },
    { "type": "each", "over": { "ref": "todos", "pick": "matches[*].path" },
      "step": { "type": "boilerplate", "path": "{{item}}", "add_header": "Needs review" } },
    { "type": "checkpoint", "checkpoint_id": "after-headers" },
    { "type": "bash", "cmd": "cargo test", "depends_on": ["todos"] }
  ]
}
```

A Step's `type` is a Tool name, or one of `if`, `each`, `parallel` and
`checkpoint`. `bash` and `http` Steps run only with `allowShell` / `allowNet`.
`{{env.NAME}}` is substituted in every string argument. Set
`"resume": "<checkpoint id>"` in `options` to skip the Steps that Checkpoint
recorded as done.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
scripts/check-features.sh                     # every feature on its own
cargo run -p rok-tools-cli -- loc --check     # rok-tools keeps its own Line limit
```

Vocabulary is in [GLOSSARY.md](GLOSSARY.md), and decisions are in [docs/decisions](docs/decisions).

## License

MIT OR Apache-2.0
