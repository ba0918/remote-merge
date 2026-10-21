#!/bin/bash
# タグと、版の正本（scripts/canonical-version.sh）の一致を確かめる。release のワークフローが、
# タグの名前を RELEASE_TAG で受け取って呼ぶ。一致しなければ終了コード 1 で止め、リリースを
# 作らせない。
set -euo pipefail

tag=${RELEASE_TAG:?RELEASE_TAG is required}

version=$("$(dirname "$0")/canonical-version.sh")

expected="v$version"
if [ "$tag" != "$expected" ]; then
  echo "the tag $tag does not match the canonical version $version (expected $expected)" >&2
  exit 1
fi

echo "the tag $tag matches the canonical version $version"
