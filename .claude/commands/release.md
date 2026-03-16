---
description: Cargo.toml バージョンバンプ → コミット → push → タグで GitHub Actions リリースを発行。"/release 0.3.0" でバージョン指定、引数なしでパッチ自動提案。
argument-hint: "[version]"
allowed-tools: Read, Edit, Bash, Grep, AskUserQuestion
---

# Release

Cargo.toml のバージョンをバンプし、コミット・push・タグ作成で GitHub Actions リリースを発行する。

## パラメータ

- `$ARGUMENTS`: バージョン文字列（例: `0.2.3`, `v0.3.0`）。`v` プレフィックスは自動補完する

## 手順

### 1. バージョン決定

- 引数がなければ Cargo.toml の現在のバージョンを読み取り、パッチバージョンを +1 した値を提案して AskUserQuestion で確認する
- 引数があればそれを使用（`v` プレフィックスは strip して Cargo.toml 用にする）

### 2. リリース内容の収集

前回のタグから HEAD までのコミットを取得:

```bash
git log $(git describe --tags --abbrev=0 2>/dev/null || echo "HEAD~10")..HEAD --oneline
```

コミット一覧を Conventional Commits の type 別に分類:
- **feat**: 新機能
- **fix**: バグ修正
- **refactor/perf**: 改善
- **その他**: chore, docs, test, style

### 3. Cargo.toml 更新

`Cargo.toml` の `version = "X.Y.Z"` を新バージョンに更新する。

### 4. コミット

```
chore: v{version} リリース準備

{type 別に分類したコミットサマリー（日本語、箇条書き）}
```

- `git commit` には `timeout: 600000` を設定（pre-commit hook が全テストを走らせるため）

### 5. Push & Tag

```bash
git push origin main
git tag v{version}
git push origin v{version}
```

- `git push` にも `timeout: 600000` を設定（pre-push hook が全テストを走らせるため）
- タグ push で GitHub Actions release.yml が自動起動する

### 6. リリースノート生成 & GitHub Release 更新

タグ push 後、コミットログから Conventional Commits の type 別に分類したリリースノートを生成し、GitHub Release に反映する。

#### 6-1. コミットログの取得

```bash
# 前のタグを取得（初回リリース時のフォールバックあり）
prev_tag=$(git describe --tags --abbrev=0 v{version}^ 2>/dev/null || echo "")

# prev_tag が空なら全コミットを対象にする
if [ -n "$prev_tag" ]; then
  git log ${prev_tag}..v{version} --oneline --no-merges
else
  git log v{version} --oneline --no-merges
fi
```

#### 6-2. type 別にセクション化した Markdown を生成

各コミットを `^(feat|fix|refactor|perf|test|docs|chore|style):` の正規表現で分類し、以下のマッピングでセクション化する:

| type | セクション名 |
|------|-------------|
| `feat` | New Features |
| `fix` | Bug Fixes |
| `refactor`, `perf` | Improvements |
| `test` | Tests |
| `docs` | Documentation |
| `chore`, `style`, その他 | Other |

フォーマット:

```markdown
## What's Changed

### New Features
- コミットメッセージ (short_hash)

### Bug Fixes
- コミットメッセージ (short_hash)

### Improvements
- コミットメッセージ (short_hash)

### Tests
- コミットメッセージ (short_hash)

### Documentation
- コミットメッセージ (short_hash)

### Other
- コミットメッセージ (short_hash)

**Full Changelog**: prev_tag...v{version}
```

- 空のセクションは省略する
- 各エントリは `- {summary} ({short_hash})` 形式（type プレフィックスは除去）
- prev_tag が空の場合は Full Changelog 行を省略する

#### 6-3. GitHub Release の更新

```bash
# Release の存在を確認
gh release view v{version} --json tagName 2>/dev/null
```

- **存在する場合**: 生成したノートを一時ファイルに書き出し、`gh release edit v{version} --notes-file /tmp/release-notes.md` で更新（シェル特殊文字の問題を回避）
- **存在しない場合**（Actions がまだ作成していない）: 警告を表示し、手動で `gh release edit` するためのコマンドを出力

### 7. 完了表示

```
Release v{version} tagged and pushed!
Release notes updated on GitHub.
CI: https://github.com/ba0918/remote-merge/actions
```
