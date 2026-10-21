#!/bin/bash
# 版の正本（scripts/canonical-version.sh）と、ほかの場所に書いた版がそろっているかを確かめる。
# CHANGELOG.md にその版の見出しと比較リンクがあり、README.md のインストール例がその版を名指しして
# いること。CI の Format & Lint が呼び、そろっていなければ終了コード 1 で止める。リリース準備の
# 書き換え漏れを、タグを打つ前の PR で見つけるため（PROJECT.md の Release）。
# 引数に作業ツリーの根を取る（省略時はカレントディレクトリ）。
set -euo pipefail

root=${1:-.}
version=$("$(dirname "$0")/canonical-version.sh" "$root")
escaped=${version//./\\.}
problems=0

if ! grep -Eq "^## \[$escaped\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" "$root/CHANGELOG.md"; then
  echo "CHANGELOG.md has no heading \"## [$version] - <date>\" for the canonical version" >&2
  problems=$((problems + 1))
fi

if ! grep -Eq "^\[$escaped\]: https://github\.com/ba0918/remote-merge/.*v$escaped$" "$root/CHANGELOG.md"; then
  echo "CHANGELOG.md has no comparison link \"[$version]: ...v$version\" for the canonical version" >&2
  problems=$((problems + 1))
fi

# インストール例の版は、mise の "remote-merge@X.Y.Z" と "`@X.Y.Z`"、インストールスクリプトの
# "VERSION=vX.Y.Z"。一つも見つからないときも止める。例の書き方が変わって読めなくなったのを、
# そろっているとみなさないため
readme_versions=$(grep -oE '(@|VERSION=v)[0-9]+\.[0-9]+\.[0-9]+' "$root/README.md" \
  | sed -E 's/^(@|VERSION=v)//' || true)
if [ -z "$readme_versions" ]; then
  echo "README.md names no install version; update this check if the install examples changed" >&2
  problems=$((problems + 1))
fi
for found in $readme_versions; do
  if [ "$found" != "$version" ]; then
    echo "README.md names version $found where the canonical version is $version" >&2
    problems=$((problems + 1))
  fi
done

if [ "$problems" -ne 0 ]; then
  exit 1
fi

echo "every version declaration names the canonical version $version"
