# ADR 0001: rok-tools contains only Tools and the Runner

Date: 2026-10-29 · Status: accepted

rok-tools started as a copy of the old `rok` CLI (`rok/crates/rok-cli`), which also shipped `generate`, `migrate`, `deploy`, `serve`, `watch` and `history` subcommands. We keep only the Tools (each a feature-gated module of the `rok-tools` library) and the optional Payload Runner, and drop the rest: rok-data owns code generation and migrations, and the others depended on crates that no longer exist. The library itself never reads config files; it takes typed options, and config loading (`[tools.<tool>]` in `rok.toml`) lives in the CLI layer and behind an opt-in `config` feature, which is why `rok-config` was dropped too.
