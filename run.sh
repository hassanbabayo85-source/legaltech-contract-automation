#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -f .env ]; then
    echo "❌ .env ba ya nan."
    exit 1
fi
cmd="${1:-run}"
case "$cmd" in
    run) echo "🚀 Fara LexHack backend…"; exec cargo run ;;
    migrate) echo "📦 Migrations…"; exec cargo run -- --migrate ;;
    check) exec cargo check --workspace --all-targets ;;
    test) exec cargo test --workspace --all-targets ;;
    *) echo "Amfani: $0 [run|migrate|check|test]"; exit 1 ;;
esac
