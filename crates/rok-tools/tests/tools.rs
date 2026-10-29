//! One end-to-end check per Tool through its public `run` function.
#![cfg(feature = "full")]

use rok_tools::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn setup(files: &[(&str, &str)]) -> (TempDir, Context) {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let p = dir.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }
    let cx = Context::new(dir.path()).unwrap();
    (dir, cx)
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

#[test]
fn fs_tools_round_trip() {
    let (dir, cx) = setup(&[]);
    let w = write::run(
        &write::WriteOptions {
            path: "a/b.txt".into(),
            content: "hello\n".into(),
        },
        &cx,
    )
    .unwrap();
    assert!(w.diff.is_some());
    assert_eq!(read(dir.path(), "a/b.txt"), "hello\n");

    let r = read::run(&read::ReadOptions::new("a"), &cx).unwrap();
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.files[0].path, "a/b.txt");

    let p = patch::run(
        &patch::PatchOptions {
            path: "a/b.txt".into(),
            edits: vec!["hello=>bye".parse().unwrap()],
        },
        &cx,
    )
    .unwrap();
    assert_eq!(p.edits_applied, 1);

    cp::run(
        &cp::CpOptions {
            from: "a".into(),
            to: "c".into(),
            recursive: true,
        },
        &cx,
    )
    .unwrap();
    assert_eq!(read(dir.path(), "c/b.txt"), "bye\n");
    mkdir::run(&mkdir::MkdirOptions { path: "d".into() }, &cx).unwrap();
    let m = mv::run(
        &mv::MvOptions {
            from: "c/b.txt".into(),
            to: "d".into(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(m.to, "d/b.txt");
    rm::run(
        &rm::RmOptions {
            path: "c".into(),
            recursive: true,
        },
        &cx,
    )
    .unwrap();
    assert!(!dir.path().join("c").exists());
    let err = rm::run(
        &rm::RmOptions {
            path: "d".into(),
            recursive: false,
        },
        &cx,
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::InvalidOptions(_)));
}

#[test]
fn read_reports_missing_files() {
    let (_dir, cx) = setup(&[]);
    assert!(matches!(
        read::run(&read::ReadOptions::new("nope.txt"), &cx),
        Err(ToolError::NotFound(_))
    ));
}

#[test]
fn snapshot_then_restore() {
    let (dir, cx) = setup(&[("src/a.rs", "fn a() {}\n")]);
    let s = snapshot::run(
        &snapshot::SnapshotOptions {
            path: "src".into(),
            snapshot_id: "before".into(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(s.files, 1);
    fs::write(dir.path().join("src/a.rs"), "broken").unwrap();
    restore::run(
        &restore::RestoreOptions {
            snapshot_id: "before".into(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(read(dir.path(), "src/a.rs"), "fn a() {}\n");
    assert!(
        snapshot::run(
            &snapshot::SnapshotOptions {
                path: "src".into(),
                snapshot_id: "../x".into()
            },
            &cx
        )
        .is_err()
    );
}

#[test]
fn grep_respects_gitignore() {
    let (_dir, cx) = setup(&[
        (".gitignore", "ignored/\n"),
        ("src/a.rs", "// TODO one\nfn a() {}\n"),
        ("ignored/b.rs", "// TODO hidden\n"),
        ("target/c.rs", "// TODO build output\n"),
        (".env", "TODO=dotfiles are searched\n"),
    ]);
    let out = grep::run(&grep::GrepOptions::new("TODO", "."), &cx).unwrap();
    let paths: Vec<_> = out.matches.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, vec![".env", "src/a.rs"]);

    let mut no_ignore = cx.clone();
    no_ignore.respect_ignore = false;
    let all = grep::run(&grep::GrepOptions::new("TODO", "."), &no_ignore).unwrap();
    assert_eq!(all.matches.len(), 4);
}

#[test]
fn grep_options() {
    let (_dir, cx) = setup(&[("a.txt", "Foo foobar\nfoo\n"), ("b.md", "foo\n")]);
    let mut o = grep::GrepOptions::new("foo", ".");
    o.whole_word = true;
    o.ignore_case = true;
    o.ext = vec!["txt".into()];
    let out = grep::run(&o, &cx).unwrap();
    assert_eq!(
        out.matches.iter().map(|m| m.line).collect::<Vec<_>>(),
        vec![1, 2]
    );
    o.regex = true;
    o.pattern = "(".into();
    assert!(matches!(
        grep::run(&o, &cx),
        Err(ToolError::InvalidOptions(_))
    ));
}

#[test]
fn replace_and_refactor() {
    let (dir, cx) = setup(&[
        ("a.rs", "fn old_name() {}\nfn old_name_2() {}\n"),
        ("b.txt", "price: $5\n"),
    ]);
    let r = refactor::run(
        &refactor::RefactorOptions {
            symbol: "old_name".into(),
            rename_to: "new_name".into(),
            path: ".".into(),
            ext: vec![],
            dry_run: false,
            whole_word: true,
        },
        &cx,
    )
    .unwrap();
    assert_eq!(r.total_replacements, 1);
    assert_eq!(
        read(dir.path(), "a.rs"),
        "fn new_name() {}\nfn old_name_2() {}\n"
    );

    let out = replace::run(
        &replace::ReplaceOptions {
            pattern: "$5".into(),
            replacement: "$10".into(),
            path: ".".into(),
            ext: vec![],
            glob: Some("*.txt".into()),
            regex: false,
            ignore_case: false,
            whole_word: false,
            dry_run: false,
        },
        &cx,
    )
    .unwrap();
    assert_eq!((out.files_scanned, out.files_modified), (1, 1));
    assert_eq!(read(dir.path(), "b.txt"), "price: $10\n");
}

#[test]
fn scan_summarize_deps() {
    let (_dir, cx) = setup(&[
        ("src/main.rs", "use crate::a;\nmod a;\nfn main() {}\n"),
        (
            "web/index.ts",
            "import { x } from './x';\nexport const y = 1;\n",
        ),
        (
            "web/x.ts",
            "import { y } from './index';\nexport function x() {}\n",
        ),
    ]);
    let s = scan::run(
        &scan::ScanOptions {
            path: ".".into(),
            depth: 3,
            include: vec![],
            output: scan::ScanDetail::Full,
        },
        &cx,
    )
    .unwrap();
    assert_eq!(s.file_count, 3);
    assert!(s.entry_points.contains(&"src/main.rs".to_string()));
    assert_eq!(s.exports["web/index.ts"], vec!["y"]);

    let sum = summarize::run(
        &summarize::SummarizeOptions {
            path: "web/x.ts".into(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(sum.imports, vec!["./index"]);
    assert_eq!(sum.functions, vec!["x()"]);

    let d = deps::run(
        &deps::DepsOptions {
            path: ".".into(),
            depth: 3,
            include: vec![],
            focus: None,
        },
        &cx,
    )
    .unwrap();
    assert_eq!(d.graph["web/x.ts"], vec!["./index"]);
}

#[test]
fn extract_formats() {
    let (_dir, cx) = setup(&[
        ("a.json", r#"{"name":"x","version":"1"}"#),
        ("b.toml", "name = 'y'\n[deps]\nz = 1\n"),
        ("c.yaml", "name: w\n"),
        (".env", "# c\nKEY=\"v\"\n"),
    ]);
    let pick = |path: &str, keys: &[&str]| {
        extract::run(
            &extract::ExtractOptions {
                path: path.into(),
                pick: keys.iter().map(|k| k.to_string()).collect(),
            },
            &cx,
        )
        .unwrap()
        .data
    };
    assert_eq!(
        pick("a.json", &["name"]),
        serde_json::json!({ "name": "x" })
    );
    assert_eq!(pick("b.toml", &[])["deps"]["z"], 1);
    assert_eq!(pick("c.yaml", &[])["name"], "w");
    assert_eq!(pick(".env", &[])["KEY"], "v");
}

#[test]
fn diff_counts_changes() {
    let (_dir, cx) = setup(&[("a", "1\n2\n3\n"), ("b", "1\n3\n4\n")]);
    let d = diff::run(
        &diff::DiffOptions {
            a: "a".into(),
            b: "b".into(),
            format: diff::DiffFormat::Unified,
        },
        &cx,
    )
    .unwrap();
    assert_eq!((d.added, d.removed, d.is_identical), (1, 1, false));
    assert!(d.unified_diff.unwrap().contains("-2"));
}

#[test]
fn import_adds_removes_and_sorts() {
    let (dir, cx) = setup(&[(
        "a.ts",
        "import { b } from 'b';\nimport { a } from 'a';\n\nconsole.log(a, b);\n",
    )]);
    let out = import::run(
        &import::ImportOptions {
            path: "a.ts".into(),
            add: vec!["{ c } from './c';".into()],
            remove: vec!["'b'".into()],
            organize: true,
        },
        &cx,
    )
    .unwrap();
    assert_eq!(out.added, vec!["import { c } from './c';"]);
    assert_eq!(out.removed.len(), 1);
    assert_eq!(
        read(dir.path(), "a.ts"),
        "import { a } from 'a';\nimport { c } from './c';\n\nconsole.log(a, b);\n"
    );
}

#[test]
fn boilerplate_is_idempotent() {
    let (dir, cx) = setup(&[("run.sh", "echo hi\n")]);
    let o = boilerplate::BoilerplateOptions {
        path: "run.sh".into(),
        add_header: Some("Build script".into()),
        add_license: Some("mit".into()),
        add_shebang: Some("#!/usr/bin/env bash".into()),
    };
    let first = boilerplate::run(&o, &cx).unwrap();
    assert!(first.header_added && first.license_added && first.shebang_added);
    let expected = "#!/usr/bin/env bash\n# Build script\n# SPDX-License-Identifier: MIT\necho hi\n";
    assert_eq!(read(dir.path(), "run.sh"), expected);
    let second = boilerplate::run(&o, &cx).unwrap();
    assert!(!second.header_added && !second.license_added && !second.shebang_added);
    assert_eq!(read(dir.path(), "run.sh"), expected);
}

#[test]
fn dead_code_finds_unused() {
    let (_dir, cx) = setup(&[(
        "a.py",
        "import os\nimport sys\n\ndef used():\n    return sys.argv\n\ndef unused_helper():\n    pass\n\nused()\n",
    )]);
    let out = dead_code::run(
        &dead_code::DeadCodeOptions {
            path: ".".into(),
            include: vec![],
        },
        &cx,
    )
    .unwrap();
    let fns: Vec<_> = out
        .unused_functions
        .iter()
        .map(|i| i.symbol.as_str())
        .collect();
    assert_eq!(fns, vec!["unused_helper"]);
    let imports: Vec<_> = out
        .unused_imports
        .iter()
        .map(|i| i.symbol.as_str())
        .collect();
    assert_eq!(imports, vec!["os"]);
}

#[test]
fn loc_limits_and_overrides() {
    let big = "let x = 1;\n".repeat(12);
    let (_dir, cx) = setup(&[
        ("src/big.rs", &big),
        ("src/gen/huge.rs", &big),
        ("src/small.rs", "// c\nfn a() {}\n"),
        ("notes.md", &"line\n".repeat(50)),
    ]);
    let mut o = loc::LocOptions::new(".");
    o.max_lines = 10;
    o.check = true;
    let out = loc::run(&o, &cx).unwrap();
    assert_eq!(out.files.len(), 3, "markdown is not a source language");
    assert_eq!(out.totals.code, 25);
    assert_eq!(
        out.violations
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/big.rs", "src/gen/huge.rs"]
    );
    assert!(!loc::Loc::succeeded(&out));

    o.overrides = vec![loc::LocOverride {
        path: "src/gen/**".into(),
        max_lines: 0,
    }];
    o.exclude = vec!["src/big.rs".into()];
    let out = loc::run(&o, &cx).unwrap();
    assert!(out.violations.is_empty());
    assert!(loc::Loc::succeeded(&out));

    o.include = vec!["*.md".into()];
    o.exclude.clear();
    let md = loc::run(&o, &cx).unwrap();
    assert_eq!((md.files.len(), md.violations.len()), (1, 1));
}

#[test]
fn template_builtin_and_source() {
    let (dir, cx) = setup(&[("t.tera", "Hello {{ name | pascalcase }}!")]);
    let vars = vec![kv::KeyValue::new("name", "my widget")];
    template::run(
        &template::TemplateOptions {
            name: None,
            builtin: None,
            source: Some("t.tera".into()),
            output: "out.txt".into(),
            vars: vars.clone(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(read(dir.path(), "out.txt"), "Hello MyWidget!");
    template::run(
        &template::TemplateOptions {
            name: None,
            builtin: Some("rust-module".into()),
            source: None,
            output: "m.rs".into(),
            vars,
        },
        &cx,
    )
    .unwrap();
    assert!(read(dir.path(), "m.rs").contains("pub mod my widget"));
    let both = template::TemplateOptions {
        name: Some("a".into()),
        builtin: Some("b".into()),
        source: None,
        output: "x".into(),
        vars: vec![],
    };
    assert!(matches!(
        template::run(&both, &cx),
        Err(ToolError::InvalidOptions(_))
    ));
}

#[test]
fn bash_reports_exit_code() {
    let (_dir, cx) = setup(&[]);
    let ok = bash::run(
        &bash::BashOptions {
            cmd: "echo hi".into(),
        },
        &cx,
    )
    .unwrap();
    assert!(bash::Bash::succeeded(&ok));
    assert!(ok.stdout.contains("hi"));
    let fail = bash::run(
        &bash::BashOptions {
            cmd: "exit 3".into(),
        },
        &cx,
    )
    .unwrap();
    assert_eq!(fail.exit_code, 3);
    assert!(!bash::Bash::succeeded(&fail));
}

#[test]
fn git_status_in_a_fresh_repo() {
    let (dir, cx) = setup(&[]);
    if std::process::Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(dir.path())
        .status()
        .is_err()
    {
        return; // git not installed
    }
    let clean = git::run(
        &git::GitOptions {
            op: git::GitOp::Status,
            args: vec![],
        },
        &cx,
    )
    .unwrap();
    assert_eq!(clean.clean, Some(true));
    fs::write(dir.path().join("new.txt"), "x").unwrap();
    let dirty = git::run(
        &git::GitOptions {
            op: git::GitOp::Status,
            args: vec![],
        },
        &cx,
    )
    .unwrap();
    assert_eq!(dirty.lines, vec!["?? new.txt"]);
    let bad = git::run(
        &git::GitOptions {
            op: git::GitOp::Log,
            args: vec!["--not-a-flag".into()],
        },
        &cx,
    );
    assert!(matches!(bad, Err(ToolError::Command { .. })));
}

#[test]
fn registry_lists_every_tool_in_full() {
    let names: Vec<_> = registry().iter().map(|t| t.name).collect();
    assert_eq!(names, ALL_TOOLS);
    assert_eq!(find("dead_code").unwrap().name, "dead-code");
    assert_eq!(find("deadCode").unwrap().name, "dead-code");
    assert!(matches!(find("nope"), Err(ToolError::InvalidOptions(_))));
    let out = find("grep").unwrap().run_json(
        serde_json::json!({ "pattern": "x" }),
        &Context::current().unwrap(),
    );
    assert!(out.is_ok());
}
