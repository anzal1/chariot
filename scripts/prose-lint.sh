#!/usr/bin/env bash
# Fails on em dashes and stock AI phrases in user-facing text and the last commit message.
set -euo pipefail
cd "$(dirname "$0")/.."
phrases='delve|tapestry|seamlessly|game-changer|in today.s fast-paced|it.s worth noting|unlock the power|elevate your|navigate the complexities|robust and scalable'
files=$(git ls-files '*.md' '*.html' 'crates/**/*.rs' 'spikes/**/*.rs' | grep -v '^experiments/' || true)
fail=0
if grep -nH $'—' $files 2>/dev/null; then echo "prose-lint: em dash found"; fail=1; fi
if grep -niHE "$phrases" $files 2>/dev/null; then echo "prose-lint: stock phrase found"; fail=1; fi
msg=$(git log -1 --pretty=%B 2>/dev/null || true)
if printf '%s' "$msg" | grep -q $'—'; then echo "prose-lint: em dash in last commit message"; fail=1; fi
exit $fail
