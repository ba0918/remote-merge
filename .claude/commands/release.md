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

### 6. 完了表示

```
Release v{version} tagged and pushed!
CI: https://github.com/ba0918/remote-merge/actions
```
