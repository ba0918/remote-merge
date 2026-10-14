# 開発時の自動チェック

## Context

テスト環境を再現可能にする作業では、整形・静的検査・テスト・仕様検査を開発時にも実行したい。
現状の CI は整形・Clippy・テストを実行するが kotowari check は実行しない。
利用者のローカル Git フックはコミット時に全テストを走らせ、Claude Code 専用フックはコードを自動修正して追跡済みファイルを再ステージする。
これらと衝突しない共有チェック設定を作り、通常の開発操作と CI の保証範囲を決める。

## Agreements

- A1 Lefthook の pre-commit は cargo fmt --all --check と kotowari check を実行し、pre-push は cargo clippy --all-targets --all-features -- -D warnings と Docker 不要の通常テストを実行する。
  - why: 短い検査をコミット時に行い、時間のかかる検査は push 前へ移して、開発中に繰り返し実行できるようにするため。
  - decided_by: user (took the recommendation)
- A2 CI は整形・Clippy・通常テスト・kotowari check に加え、独立した必須ジョブで実 OpenSSH コンテナ検証を実行する。
  - why: ローカルのフックが未導入または無効でも、検証の不足を CI で検出するため。
  - decided_by: user (took the recommendation)
- A3 この作業は既存の未追跡ローカル Git フックを自動で上書きせず、Lefthook の設定と移行手順を提供する。
  - why: ローカルに置かれた利用者のフックと暗黙に入れ替えないため。
  - decided_by: user (took the recommendation)
- A4 Claude Code 専用の自動整形・Clippy 修正・追跡済みファイルの再ステージは廃止し、共有の読み取り専用チェックに統一する。
  - why: 利用者が選んだコミット内容をフックが勝手に書き換えたり広げたりしないため。
  - decided_by: user (took the recommendation)
- A5 CI の kotowari は公開 Git リポジトリの固定コミットから取得し、バージョンが変わっても暗黙に別の仕様検査を実行しない。
  - why: 開発者のローカルにだけ存在するツールに CI を依存させず、実行するチェッカーの内容を再現できるようにするため。
  - decided_by: user (took the recommendation)
- A6 pre-commit の fmt --check は作業ツリーを検査し、ステージ内容との差による検査漏れは CI の整形検査で検出する。
  - why: ローカルフックに一時 worktree や Git index 専用の検査処理を導入せず、共有の標準コマンドで検証するため。
  - decided_by: user (took the recommendation)

## Reuse decisions

- ローカルチェック: adopt (installed tool) — インストール済みの Lefthook を共有設定の実行役として使い、Git フック起動機構を自作しない。
- Rust の検証: adopt (codebase/toolchain) — PROJECT.md と既存 CI の cargo fmt・clippy・test をそのままチェック内容に使う。
- 仕様検査: adopt (platform/Git) — kotowari の公開 Git リポジトリの固定コミットから CI で導入し、内部の仕様解析を作り直さない。
- CI: adopt (codebase) — .github/workflows/ci.yml の既存ジョブに kotowari 検証を加え、OS 固有ジョブは再現可能なテスト環境の仕様と合わせる。
