#!/bin/sh
# Setup git hooks by symlinking from scripts/ to .git/hooks/
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOKS_DIR="$(git rev-parse --git-dir)/hooks"

for hook in pre-commit pre-push; do
    src="$SCRIPT_DIR/$hook"
    dst="$HOOKS_DIR/$hook"
    if [ -f "$src" ]; then
        ln -sf "$src" "$dst"
        echo "Installed $hook hook"
    fi
done

echo "Done! Git hooks are ready."
