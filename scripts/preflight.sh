#!/usr/bin/env bash
# Local mirror of CI. Light by default (touched crates only); pass --full for the whole workspace.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(pwd).noindex}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
run() { nice -n 15 "$@"; }

scripts/prose-lint.sh
run cargo fmt --all --check

if [[ "${1:-}" == "--full" ]]; then
  crates=$(cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json;print(' '.join(p['name'] for p in json.load(sys.stdin)['packages']))")
elif [[ $# -gt 0 ]]; then
  crates="$*"
else
  changed=$(git diff --name-only origin/main...HEAD 2>/dev/null; git diff --name-only; git diff --name-only --cached)
  crates=$(printf '%s\n' "$changed" | sed -nE 's#^(crates|spikes)/([^/]+)/.*#\2#p' | sort -u | tr '\n' ' ')
fi
[[ -z "${crates// }" ]] && { echo "preflight: no crates touched, prose and fmt ok"; exit 0; }
for c in $crates; do
  run cargo clippy -p "$c" --all-targets -- -D warnings
  # The GPUI board has no tests and is heavy; check only.
  [[ "$c" == "board" ]] || run cargo test -p "$c" -- --test-threads=4
done
echo "preflight: ok ($crates)"
