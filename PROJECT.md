# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust の CLI ツール。非対話で動き、JSON 出力に対応する。正規化した仕様は `docs/ir/` に置く。文書の入口は `docs/README.md`。

## Stack and layout

- Rust の Cargo ワークスペース。edition・MSRV・lint はルートの `Cargo.toml` で一度だけ決め、各 crate が受け継ぐ。主な依存は tokio、russh、clap、similar、serde。`Cargo.lock`、`.cargo/`、profile、`target/` はルートに置く。
- crate と依存の向き（下から上）。上向きと横向きの依存は足さない。
  - `crates/remote-merge-settings/`: 接続設定の型とホーム展開（`expand_tilde`）。依存は標準ライブラリと dirs だけ。
  - `crates/remote-merge-core/`: 入出力を持たない規則と、このマシンの一つの項目への操作。
  - `crates/remote-merge-protocol/`（→ core）: エージェントとの通信の型・直列化・ハンドシェイク・フレーム。
  - `crates/remote-merge-config/`（→ settings・core）: TOML の形、設定元の探索と読み込み、合成、値の検査、include の値の正規化。
  - `crates/remote-merge-ssh/`（→ settings・core）: SSH の接続、ホスト鍵、パスワードとパスフレーズ、接続の失敗の手がかり。端末の入出力も、ファイルを読み書きするシェルのスクリプトも持たない（docs/decision/records/2026-10-12-foundation.md の A12）。
  - `crates/remote-merge-engine/`（→ core・config）: 比べた結果を各コマンドの扱いに訳す純粋な関数（`treat/`）、出力の型（`service/types`）、ローカルの範囲の操作（`local_io`）、バックアップの保存（`backup`・`backup_store`）。
  - `crates/remote-merge-agent/`（→ core・protocol）: サーバで動くエージェントの受け取り口と、手元のクライアント・SSH の上の転送・配置のコマンドの組み立て。
  - `crates/remote-merge/`（製品）: CLI（`cli/`）、出力の書き出し（`service/output`）、実行時の入出力（`runtime/`）、診断ログ（`telemetry/`）、端末につなぐホスト鍵の確認とパスフレーズの問い合わせ（`ssh/`）。下の crate の一部を `remote_merge::…` から再公開する。
- 規則の置き場所: 純粋な規則は core（`compare/`・`scan/`・`change_check`・`content` など）と engine の `treat/` に置き、製品の `runtime/` と `cli/` は事実を集めて渡し、結果を書き出すだけにする。比べる流れの規則は core の `compare/`、走査の規則（exclude・include・件数・循環・UTF-8 でない名前）は core の `scan/` に一か所ずつ置き、ローカルとエージェントは同じ関数を動かす。「同じパスかその下か」は core の `relative_path` の `is_within`・`is_below` を使う。
- パスの型: `RelPath` は root_dir からの相対パスで、`RelPath::parse` を通ったときだけ作れる。`EntryPath` は root_dir の下の一つの項目で、root_dir 全体は表せない。入力の形は、コマンドの入口・エージェントの受け取り口・バックアップの記録の読み込みで一度だけ検査する。経路の操作は、一つの項目への操作では `EntryPath` を、範囲の操作（走査・子の一覧・パスの確認）では `RelPath` を受け取り、自分では入力の形も root_dir の外かどうかも検査しない。
- 操作の失敗: どの経路も操作の結果を core の `OpFailure` 一つの型で返し、止めるか続けるかは経路の外で決める。利用者に示す文言は `OpError` で一か所で作る。
- 経路: リモートの操作はエージェントだけで行う。つなぐときに一度だけエージェントを使えるかを決め（core の `route_choice`、製品の `runtime/route_state`）、使えなければ読み書きの前に止めて決め直さない。接続断の扱いは `runtime/route_retry` が一か所で決める（読み取りはつなぎ直して一度だけやり直し、変更はやり直さずに止める）。ローカルとエージェントは、一つの項目への操作に core の `machine_ops` を、パスの確認に core の `path_resolution` を、同じ関数で使う。
- 比べる流れ（製品の `runtime/compare_*`）は二段: まず左右と参照先を同じ範囲の走査で集め、次に判定が求めたハッシュとコマンドが告げた中身だけを側ごとにまとめて取る。merge と sync の書き込みは、書き込み先と祖先をまとめて確かめ、パスごとに変更の直前の判定・バックアップ・変更・権限の順に行う。パスをまとめて確かめる問いは、どれも `runtime/root_checked.rs` で root_dir そのものを先頭に足して尋ね、その答えで root_dir がないことを決める。
- 内容の解釈（文字コード・BOM・行）は core の `content` が作り、一つのパスの解釈は engine の `treat/content` で作って diff と部分マージが共有する。raw の比較・入出力・保存形式はこの解釈を使わない。
- 通常テストは `crates/remote-merge/tests/`（仕様の印を持つ契約テストは `crates/remote-merge/tests/contract/`）。Docker の E2E は `tests/container-e2e/` の独立したワークスペースに置く。proptest の失敗入力はルートの `proptest-regressions/` に保存する。
- 製品バージョンの正本は `crates/remote-merge/Cargo.toml` の `package.version`。
- 製品以外の crate（core・protocol・settings・ssh・config・agent・engine）は `publish = false` で、版は各 crate の `Cargo.toml` の `package.version` に製品版とは独立に持つ。通信版の正本は protocol の `define_protocol_version` の単一リテラルで、製品側の `CLI_VERSION` は製品版と通信版を組み合わせた定数とする。
- 設定はグローバルの `~/.config/remote-merge/config.toml` と、プロジェクトの `.remote-merge.toml`。後者が優先し、`[filter]` は和集合でマージする。

## Commands

| Purpose | Command |
|---|---|
| Build | `cargo build` |
| Test | `cargo nextest run --workspace --all-features`（未導入なら `cargo test --workspace --all-features`。CI と pre-push はこちら。`--all-features` がないと ssh_integration・agent_ssh_transport と契約テストの多くが走らない） |
| Check | `cargo fmt --all --check` / `cargo clippy --all-targets --all-features -- -D warnings` |
| Run | `cargo run -- status --left local --right <server>`（サブコマンドなしは使い方を出して終了コード 2） |
| Spec check | `kotowari check` |
| Real OpenSSH/sudo tests | `scripts/run-container-e2e.sh`（Docker 必須、通常テストと別パッケージ） |
| CentOS 5 kernel check | `scripts/kernel-2.6.18/reproduce.sh <作業ディレクトリ>`（2.6.18-419.el5 を qemu で起動し、musl のエージェントが手元と同じ答えを返し、エージェント・core・protocol の単体テストが root と一般の利用者で通るかを確かめる。カーネルの rpm と静的な BusyBox を作業ディレクトリに置く。前提はスクリプトの先頭。エージェントの通信か、エージェントが使うシステムコールを変えたとき） |

## Conventions specific to this project

- ユーザー向けの文言と CLI ヘルプは英語。コード内のコメントは日本語でもよい。
- コミットメッセージは Conventional Commits 形式で、件名・本文は日本語。
- `lefthook.yml` は fmt・仕様検査を pre-commit、Clippy・通常テストを pre-push で読み取り専用で実行する。既存の個人フックを置き換えないため `lefthook install` は自動実行しない。既存フックの所有者が内容を確認し、必要なら手動で統合する。未ステージの変更も手動検査する場合は `lefthook run pre-commit --force --no-auto-install` / `lefthook run pre-push --force --no-auto-install` を使う。
- 旧 OpenSSH の手動負荷試行は `bench/legacy-ssh/setup.sh` から開始する。試行中のシェルを終了すると専用コンテナ・イメージ・鍵・known_hosts・データが破棄され、個人の SSH 設定は変更されない。CI の保証範囲には含めない。

## Release

版の正本・CHANGELOG・タグの決まりは ba0918-release に従う。このリポジトリでの手順は次のとおり。

1. ブランチで、次を一つのコミット（`chore: v<版> リリース準備`）にまとめる。
   - `crates/remote-merge/Cargo.toml` の `package.version` と、それに合わせた `Cargo.lock`。
   - `CHANGELOG.md` の `## [Unreleased]` を `## [<版>] - <日付>` にし、末尾の `[Unreleased]` のリンクを `[<版>]: https://github.com/ba0918/remote-merge/compare/v<前の版>...v<版>` に書き換える。次に利用者に見える変更を入れるときに、`## [Unreleased]` と `compare/v<版>...HEAD` のリンクを置き直す。
   - `README.md` のインストール例が明示している版（mise の `@<版>` と、インストールスクリプトの `VERSION=v<版>`）。

   CHANGELOG と README が版の正本とそろっているかは、CI の Format & Lint が `scripts/check-version-declarations.sh` で確かめる（版の正本は `scripts/canonical-version.sh` で読む）。
2. PR を作り、CI が全部通ったら main に入れる。
3. main の CI がそのコミットで通ってから、そのコミットに `v<版>` のタグを打って push する。
4. タグの push で `.github/workflows/release.yml` が動く。タグと版の正本の一致を `scripts/check-release-version.sh` で確かめ、配布物を作り、CHANGELOG のその版の節を本文にして GitHub Release を作る（`scripts/create-release.sh`）。その節がないと Release は作られず、タグだけが公開されて残る。Release の本文は CHANGELOG の節のまま保ち、あとから書き換えない。

## Security boundary

レビューや実装で security の指摘を直すかを判断するときは、この節に照らす。決定は [foundation A11](docs/decision/records/2026-10-12-foundation.md#A11)。

- 信頼するもの: 操作する利用者、接続先のサーバ、配置先に書ける組織内のアカウント。配置先は組織内の誰でも書ける共有ディレクトリが普通である。
- 直さないもの: その中の誰かが、ツールの確かめと変更の間にすり替える（ハードリンク・symlink・rename の差し替え）攻撃。
- 直すもの: 同時に動く別の作業（並行するデプロイ・ほかの利用者のふつうの編集）で、ツールが壊す・誤った結果を返す・黙って失敗する正しさの問題。信頼しない入力（ネットワーク越しの応答の解釈、パスの形）からのコマンド注入と、root_dir の外への書き込み。

## Constraints

- 読み取り中の SSH 接続エラーは一度だけ再試行する。書き込み前には対象の内容も再確認する（現行の更新時刻だけの検査は IR に対する実装課題）。
- シンボリックリンクは参照先のパスで比較し、バイナリファイルは SHA-256 で比較する。
- リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
