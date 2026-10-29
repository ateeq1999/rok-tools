# rok-tools

A library of self-contained developer tools (search, edit, scan, refactor, …) that other crates import piecemeal, and that the `rok-tools` and `rok tools` commands expose on the command line.

## Language

**Tool**:
One named capability (e.g. grep, replace, scan, loc), usable as a Rust function or as a command-line subcommand.
_Avoid_: Step (when meaning the capability), command, plugin

**Step**:
One invocation of a Tool inside a Payload, with its arguments.
_Avoid_: Task, action

**Payload**:
A JSON document listing Steps (and control flow over them) to execute in order.
_Avoid_: Script, task file

**Runner**:
The executor that carries out a Payload's Steps.
_Avoid_: Engine, executor

**Snapshot**:
A saved copy of chosen files that a Tool can later restore.
_Avoid_: Backup, checkpoint

**Checkpoint**:
The Runner's saved progress through a Payload, so a failed run can resume.
_Avoid_: Snapshot, savepoint

**Tool group**:
A named set of related Tools that can be enabled together (e.g. fs, search, code, vcs, net, template).
_Avoid_: Category, bundle

**Line limit**:
The maximum number of lines of code a single source file may contain before it is reported as a violation; 600 unless configured otherwise.
_Avoid_: Max file size, LOC cap
