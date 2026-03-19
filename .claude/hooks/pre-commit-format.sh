#!/bin/bash
# Pre-commit hook: auto-format and lint-fix before git commit
# git commit を検知したら cargo fmt + cargo clippy --fix を自動実行する

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command')

# git commit コマンドかチェック
if echo "$COMMAND" | grep -qE '^\s*git\s+commit'; then
  echo "Running cargo fmt..." >&2
  cargo fmt --all
  if [ $? -ne 0 ]; then
    echo "cargo fmt failed!" >&2
    exit 2
  fi

  echo "Running cargo clippy --fix..." >&2
  cargo clippy --fix --allow-dirty --allow-staged 2>&1 | tail -5 >&2
  if [ $? -ne 0 ]; then
    echo "cargo clippy --fix failed!" >&2
    exit 2
  fi

  # fmt/clippy で変更されたファイルを自動ステージ
  git add -u >&2

  echo "Auto-format complete. Proceeding with commit." >&2
fi

exit 0
