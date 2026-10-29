//! Benchmarks for Payload parsing, a few Tools, and a Runner pass.
//!
//! Run with: `cargo bench -p rok-tools --features full,runner --bench tools`
#![allow(missing_docs)] // criterion_group! generates undocumented items

use criterion::{Criterion, criterion_group, criterion_main};
use rok_tools::runner::{Payload, Runner};
use rok_tools::{Context, grep, loc};
use std::hint::black_box;
use tempfile::TempDir;

const PAYLOAD: &str = r#"{
    "name": "bench",
    "options": { "history": false },
    "steps": [
        { "type": "grep", "pattern": "TODO", "path": "." },
        { "type": "loc", "path": "." },
        { "type": "read", "path": "src/file_0.rs" },
        { "type": "write", "path": "out.txt", "content": "hello" }
    ]
}"#;

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    for i in 0..50 {
        let body = format!("// file {i}\nfn f{i}() {{\n    // TODO: {i}\n    let x = {i};\n}}\n")
            .repeat(20);
        std::fs::write(dir.path().join(format!("src/file_{i}.rs")), body).unwrap();
    }
    dir
}

fn parsing(c: &mut Criterion) {
    c.bench_function("parse payload", |b| {
        b.iter(|| Payload::from_json(black_box(PAYLOAD)).unwrap())
    });
}

fn tools(c: &mut Criterion) {
    let dir = fixture();
    let cx = Context::new(dir.path()).unwrap();
    c.bench_function("grep 50 files", |b| {
        b.iter(|| grep::run(black_box(&grep::GrepOptions::new("TODO", ".")), &cx).unwrap())
    });
    c.bench_function("loc 50 files", |b| {
        b.iter(|| loc::run(black_box(&loc::LocOptions::new(".")), &cx).unwrap())
    });
    c.bench_function("runner 4 steps", |b| {
        b.iter(|| {
            Runner::in_dir(Payload::from_json(PAYLOAD).unwrap(), dir.path())
                .unwrap()
                .run()
        })
    });
}

criterion_group!(benches, parsing, tools);
criterion_main!(benches);
