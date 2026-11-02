#!/usr/bin/env bash
# Builds rok-tools with every Tool feature on its own (with and without `cli`),
# each Tool group, and the non-tool features, so a missing `dep:` in a feature
# list or an unguarded shared helper fails CI.
set -euo pipefail

tools=(read write patch mv cp rm mkdir snapshot restore grep replace scan summarize
       extract diff lint import refactor deps boilerplate dead-code loc template git bash http)
groups=(fs search code template vcs full)
others=(runner config cli "runner,config,cli")

check() {
  echo "== rok-tools --features $1"
  RUSTFLAGS="-D warnings" cargo check -q -p rok-tools --no-default-features --features "$1"
}

check_none() {
  echo "== rok-tools (no features)"
  RUSTFLAGS="-D warnings" cargo check -q -p rok-tools --no-default-features
}

check_none
for f in "${tools[@]}"; do check "$f"; check "$f,cli"; done
for f in "${groups[@]}" "${others[@]}"; do check "$f"; done

echo "== rok-tools-cli --no-default-features --features loc"
RUSTFLAGS="-D warnings" cargo check -q -p rok-tools-cli --no-default-features --features loc
echo "== rok-tools-cli --no-default-features"
RUSTFLAGS="-D warnings" cargo check -q -p rok-tools-cli --no-default-features
echo "all feature combinations build"
