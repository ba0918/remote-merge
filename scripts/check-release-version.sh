#!/bin/bash
# タグと、版の正本（crates/remote-merge/Cargo.toml の package.version。PROJECT.md が定める）の
# 一致を確かめる。release のワークフローが、タグの名前を RELEASE_TAG で受け取って呼ぶ。
# 一致しなければ終了コード 1 で止め、リリースを作らせない。
set -euo pipefail

tag=${RELEASE_TAG:?RELEASE_TAG is required}

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/remote-merge/Cargo.toml | head -1)
if [ -z "$version" ]; then
  echo "the canonical version is not found in crates/remote-merge/Cargo.toml" >&2
  exit 1
fi

expected="v$version"
if [ "$tag" != "$expected" ]; then
  echo "the tag $tag does not match the canonical version $version (expected $expected)" >&2
  exit 1
fi

echo "the tag $tag matches the canonical version $version"
