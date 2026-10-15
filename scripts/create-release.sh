#!/bin/bash
# CHANGELOG.md の、このタグの版の節を GitHub リリースの本文にして作る。release のワークフローが、
# タグの名前を RELEASE_TAG で受け取って呼ぶ（本文は生成に任せず、利用者が読む節をそのまま出す）。
# 節がなければ終了コード 1 で止める。
set -euo pipefail

tag=${RELEASE_TAG:?RELEASE_TAG is required}
version=${tag#v}

notes=$(mktemp)
trap 'rm -f -- "$notes"' EXIT

awk -v heading="## [$version]" '
  index($0, heading) == 1 { found = 1; next }
  found && /^## \[/ { exit }
  found { print }
' CHANGELOG.md > "$notes"

if [ ! -s "$notes" ]; then
  echo "CHANGELOG.md has no section for $version" >&2
  exit 1
fi

gh release create "$tag" --title "$tag" --notes-file "$notes" artifacts/*
