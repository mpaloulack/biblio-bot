#!/usr/bin/env bash
# Coverage over the testable surface.
#
# src/main.rs, src/commands/, src/watcher.rs and src/download_watcher.rs are
# excluded on purpose: they are Discord gateway glue that cannot run without a
# live connection, and every decision they make is delegated to src/ui.rs,
# src/watchlist.rs and src/downloads.rs, which are covered. Excluding them
# keeps the number honest instead of diluting it with untestable I/O.
set -euo pipefail

exec cargo llvm-cov --lib \
    --ignore-filename-regex 'src/(main|commands|watcher|download_watcher)' \
    --fail-under-lines 95 \
    "$@"
