#!/bin/bash
# 版の正本（crates/remote-merge/Cargo.toml の package.version。PROJECT.md が定める）を出力する。
# 引数に作業ツリーの根を取る（省略時はカレントディレクトリ）。見つからなければ終了コード 1 で止める。
set -euo pipefail

root=${1:-.}
manifest="$root/crates/remote-merge/Cargo.toml"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$manifest" | head -1)
if [ -z "$version" ]; then
  echo "the canonical version is not found in $manifest" >&2
  exit 1
fi

printf '%s\n' "$version"
