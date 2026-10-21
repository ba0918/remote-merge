#!/bin/bash
# scripts/check-version-declarations.sh を、版の宣言をそろえた・そろえていない小さな作業ツリーに
# 当てて、通すべきものを通し、止めるべきものを止めることを確かめる。CI の Format & Lint が呼ぶ。
set -euo pipefail

check="$(cd "$(dirname "$0")" && pwd)/check-version-declarations.sh"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

failures=0

# 版 0.3.3 をそろえて宣言した作業ツリーを $work/<名前> に作る
consistent_tree() {
  local root="$work/$1"
  mkdir -p "$root/crates/remote-merge"
  cat > "$root/crates/remote-merge/Cargo.toml" <<'EOF'
[package]
name = "remote-merge"
version = "0.3.3"
EOF
  cat > "$root/CHANGELOG.md" <<'EOF'
# Changelog

## [0.3.3] - 2026-10-22

### 修正

- something

## [0.3.2] - 2026-10-16

[0.3.3]: https://github.com/ba0918/remote-merge/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/ba0918/remote-merge/compare/v0.3.1...v0.3.2
EOF
  cat > "$root/README.md" <<'EOF'
mise use --global github:ba0918/remote-merge@0.3.3
別の版を使う場合は、`@0.3.3` をその版に置き換えてください。
curl -fsSL https://example.invalid/install.sh | VERSION=v0.3.3 INSTALL_DIR="$HOME/.local/bin" sh
EOF
  printf '%s\n' "$root"
}

expect_pass() {
  local name=$1 root=$2
  if "$check" "$root" >/dev/null 2>&1; then
    echo "ok: $name"
  else
    echo "FAILED: $name (expected the check to pass)"
    failures=$((failures + 1))
  fi
}

# 止まり、理由に $3 を含むこと。別の理由（スクリプトがないなど）で止まったのを通さないため
expect_fail() {
  local name=$1 root=$2 reason=$3 stderr
  if stderr=$("$check" "$root" 2>&1 >/dev/null); then
    echo "FAILED: $name (expected the check to stop)"
    failures=$((failures + 1))
  elif [[ "$stderr" != *"$reason"* ]]; then
    echo "FAILED: $name (stopped, but the reason lacks \"$reason\": $stderr)"
    failures=$((failures + 1))
  else
    echo "ok: $name"
  fi
}

root=$(consistent_tree consistent)
expect_pass "declarations that all name the canonical version pass" "$root"

root=$(consistent_tree unreleased-heading)
sed -i 's/^## \[0\.3\.3\] - 2026-10-22$/## [Unreleased]/' "$root/CHANGELOG.md"
expect_fail "a changelog still headed Unreleased for the canonical version stops" "$root" "no heading"

root=$(consistent_tree missing-link)
sed -i '/^\[0\.3\.3\]: /d' "$root/CHANGELOG.md"
expect_fail "a changelog without the comparison link for the canonical version stops" "$root" "no comparison link"

root=$(consistent_tree stale-link)
sed -i 's|^\[0\.3\.3\]: .*$|[0.3.3]: https://github.com/ba0918/remote-merge/compare/v0.3.2...HEAD|' "$root/CHANGELOG.md"
expect_fail "a comparison link that does not end at the canonical version's tag stops" "$root" "no comparison link"

root=$(consistent_tree stale-mise-example)
sed -i 's/remote-merge@0\.3\.3/remote-merge@0.3.2/' "$root/README.md"
expect_fail "a README mise example naming another version stops" "$root" "names version 0.3.2"

root=$(consistent_tree stale-script-example)
sed -i 's/VERSION=v0\.3\.3/VERSION=v0.3.2/' "$root/README.md"
expect_fail "a README install script example naming another version stops" "$root" "names version 0.3.2"

root=$(consistent_tree no-readme-version)
printf 'no version here\n' > "$root/README.md"
expect_fail "a README that names no install version stops" "$root" "names no install version"

if [ "$failures" -ne 0 ]; then
  echo "$failures case(s) failed" >&2
  exit 1
fi
