#!/usr/bin/env bash
# Coverage over the testable surface.
#
# src/main.rs and src/commands/ are excluded on purpose: they are Discord
# gateway glue that cannot run without a live connection, and every decision
# they make is delegated to src/ui.rs, which is covered. Excluding them keeps
# the number honest instead of diluting it with untestable I/O.
set -euo pipefail

exec cargo llvm-cov --lib \
    --ignore-filename-regex 'src/(main|commands)' \
    --fail-under-lines 95 \
    "$@"
