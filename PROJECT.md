# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust の CLI ツール。非対話で動き、JSON 出力に対応する。正規化した仕様は `docs/ir/` に置く。旧仕様は `docs/archive/` に移行元として保存しており、内容を現行の動作と同一視しない。

## Stack and layout

- Rust。主な依存は tokio、russh、clap、similar、serde。
- ルートは Cargo ワークスペース。members と default-members は `crates/remote-merge/`、`crates/remote-merge-core/`、`crates/remote-merge-protocol/`、`crates/remote-merge-settings/`、`crates/remote-merge-ssh/`、`crates/remote-merge-config/`、`crates/remote-merge-agent/`、`crates/remote-merge-engine/`。`Cargo.lock`、`.cargo/`、release profile と `target/` はルートに置く。
- `crates/remote-merge-core/src/`: tree・diff・error・filter とその単体テスト。製品は同じ型を `remote_merge::{tree, diff, error, filter}` から再公開する。
- `crates/remote-merge-core/src/rel_path.rs`・`operations.rs`・`listing.rs`: root_dir からの相対パスの検査（`check_rel_path`。engine の `rel_path` は同じ関数を再公開する）と、このマシンでの削除・権限の変更・子の一覧。ローカルとエージェントは同じ関数を呼ぶ。SSH の子の一覧は `remote-merge-ssh` の `child_listing` が同じ三つの結果（`ChildListing`）を返す。
- `crates/remote-merge-core/src/entry_link.rs`: パスの項目そのもの（最後の要素のリンクは辿らない）が symlink ならそのリンク先を、親を一覧せずにこのマシンで読む。SSH の経路は `remote-merge-ssh` の `entry_link` が find の `-printf` だけで同じ結果を返す。エージェントの経路は、その項目だけを読む要求がないため、親の子の一覧から取り出す。merge で比べたときに symlink だった書き込み先の確認に使う。
- `crates/remote-merge-core/src/path_resolution/`: パスを "/" から要素ごとに辿る手順（`resolve_path`）、確認の結果の四つの種類と原因の文言の一覧、このマシンの基本操作（`os`。Unix では標準ライブラリと libc、Unix 以外では標準ライブラリだけを使う）。ローカルとエージェントは同じ `os::resolve_on_this_machine` を呼ぶ。SSH は製品の `runtime/remote_path/`（スクリプトの `script` と出力の読み取りの `output`）で同じ手順を POSIX sh で辿る。`runtime::remote_path` と、確認の経路を選ぶ `runtime::path_route` は test-utils の機能のときだけ公開する。
- `crates/remote-merge-protocol/src/lib.rs`: Agentの通信型・シリアライズ・ハンドシェイクとその単体テスト。製品は `remote_merge::agent::protocol` から同じ型・定数・関数を再公開する。
- `crates/remote-merge-protocol/src/framing.rs`: 長さプレフィクス付きフレームの読み書きとその単体テスト。製品は既存の `remote_merge::agent::framing` から再公開する。
- `crates/remote-merge-settings/src/lib.rs`: 標準ライブラリだけに依存する接続設定の5型と既存のDebug・Default実装。configと製品は同じ型を再公開する。
- `crates/remote-merge-config/src/lib.rs`: 設定の読み込み・認証の解決・パースと既存89単体テスト。製品の `src/config.rs` は既存公開パスを明示的に再公開する。expand_tildeは下位のconfigに所属し、SSHから参照する。
- `crates/remote-merge-ssh/src/`: 全SSH実装と既存132単体テスト。製品は `remote_merge::ssh::{client, batch_read, hint, host_key_verifier, passphrase_provider}` を公開のまま再公開し、`ssh::tree_parser` と `ssh::batch_hash` は製品crate内だけの再公開を保つ。known_hosts・known_hosts_io・preferredはSSH crate内だけに置く。設定型はsettings、ホーム展開と既定の上限はconfig、error・filter・treeはcoreの同じ項目を使う。strict実行のinherent methodはcrate内に保ち、製品の三つの呼び出しは内部再公開した委譲関数を使う。製品のtest-utils featureはSSHの同名featureへ転送する。
- `crates/remote-merge-agent/src/agent/`: Agentの実行処理・SSH転送と配置処理、既存191単体テスト。製品は既存 `remote_merge::agent` の同じ公開パスから再公開する。通信型とframingはprotocol、filter/treeはcore、shell escapeは通常依存のSSHを使う。配置の四つの版依存関数は製品側の旧signatureのwrapperから製品CLI_VERSIONを受け取る。版に関わる既存88テストとtest-only SHA helperは製品側に残す。両crateのbuild.rsはCargoのTARGETをそのままcompile時定数へ渡す。
- `crates/remote-merge-engine/src/`: local・backup・backup_store・merge/executor、共有Side、service/types・max_files・path_resolver・fast_path・diff・status・status_ref_badge・status_unreadable・content_reads・merge・merge_reference・merge_reference_reads・merge_flow・sync・rollbackと既存388単体テスト。content_readsはパスごとに中身か失敗の理由を持つ読み込みの結果の型、merge_referenceは参照先を使う merge の三者の扱いの純粋な表、merge_reference_readsはツリーと読み込みの結果からその表の入力を求める処理、status_ref_badgeは参照先との違いの印、status_unreadableは status で読めなかったファイルの取り出しを持つ。root_dir からの相対パスとして受け取る値の検査（絶対パスと ".." を拒む）は `rel_path` の `check_rel_path` が担い、diff・merge・sync の引数の検査（path_resolver）、パスの組み立て（executor の validate_remote_path）、SSH の経路の子の一覧が使う。走査の部分パス（fetch_tree_for_subpath）と path_resolver の check_path_traversal には同じ規則の別の実装が残っている。Sideの正本は `side` に置き、製品の `side` は同じ型・関数を再公開する。serviceのモジュールも製品の同じパスから同じ項目を再公開する。three_wayは三者の存在・内容比較だけを持つ。executorは既存公開項目を明示的に再公開する。validate_path_within_rootの利用はengine内にあり、製品側に未使用のprivate再公開を置かない。backup_storeは製品runtimeからcrate内だけに再公開する。coreとconfigの同じ型・定数を参照する。serdeの既存JSON表現は維持し、serde_jsonは保存レコードの読み書きにも使う通常依存に置く。
- `crates/remote-merge-engine/src/local_io.rs`: ローカルの読み書き・バッチ・ハッシュ・mtime・権限・削除・symlink・ツリー走査とパス検査。パス検査は core の path_resolution の確認の結果をそのまま返す。protocolの同じFileHashResultとcoreの同じFileNode/FileTreeを使う。製品runtimeはhelpersをcrate内だけに再公開し、rollback 以外が使う三つの種類の TargetPath は製品の target_io に置く。LocalTargetIoの検査と十五操作を具体的な自由関数に委譲する。製品側の利用が混在テストだけになった五helperはtest-only再公開に保つ。製品側にTargetIoのdispatch、LocalTargetIoの構築と接続no-op、RemoteTargetIo、CoreRuntimeとSideIOの混在する既存116テストを保つ。
- `crates/remote-merge-engine/src/service/merge_flow.rs`: ソース存在検査、比べたときに symlink だった書き込み先が同じリンク先のままかの検査と、エージェントの経路でそれに使う親の子の一覧からのリンク先の取り出し、hunk対象検査とバイト列からのhunk準備。製品service/merge_flowは三つの検査を同じ公開パスから再公開し、両側read・dry-run・backup・writeと既存十八テストを保つ。
- `crates/remote-merge-engine/src/service/rollback.rs`: 復元パス判定に加え、検査失敗または既存の二つの拒否理由を要求全体の結果へ展開する純粋なrestore_request_refusalを持つ。backup_storeは保存レコードを同じBackupPathRecordへ変換するFrom実装を持つ。製品SideIOはprivate変換wrapperと既存116混在テストを保ち、第一passの逐次read・検査、session予約、第二passのread・dry-run・pre-backup・write、最後のfinishを担当する。
- `crates/remote-merge/src/cli/` と `crates/remote-merge/src/service/`: CLI と共通処理。`crates/remote-merge/src/runtime/`: CLI の実行時処理。`crates/remote-merge/src/telemetry/`: 診断ログの保存と閲覧。`crates/remote-merge/src/ssh/` と `crates/remote-merge/src/agent/`: リモート接続と転送。
- CLIの出力とruntimeのI/Oは製品側に残す。
- 通常テストは `crates/remote-merge/tests/`。Docker E2E は `tests/container-e2e/` の独立ワークスペースに残す。proptest の失敗入力はルートの `proptest-regressions/` に保存する。
- 製品バージョンの正本は `crates/remote-merge/Cargo.toml` の `package.version`。
- core の内部バージョンは `crates/remote-merge-core/Cargo.toml` の `package.version`。core は `publish = false` とし、製品のバージョンとは独立に扱う。
- protocol の内部バージョンは `crates/remote-merge-protocol/Cargo.toml` の `package.version`。protocol は `publish = false`。通信版の正本は `define_protocol_version` の単一リテラルで、製品側の `CLI_VERSION` は製品版と通信版を組み合わせた定数とする。
- settings の内部バージョンは `crates/remote-merge-settings/Cargo.toml` の `package.version`。settings は版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- ssh の内部バージョンは `crates/remote-merge-ssh/Cargo.toml` の `package.version`。ssh は版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- config の内部バージョンは `crates/remote-merge-config/Cargo.toml` の `package.version`。configは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- agent の内部バージョンは `crates/remote-merge-agent/Cargo.toml` の `package.version`。agentは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- engine の内部バージョンは `crates/remote-merge-engine/Cargo.toml` の `package.version`。engineは版0.1.0・`publish = false` とし、製品版とは独立に扱う。
- 設定はグローバルの `~/.config/remote-merge/config.toml` と、プロジェクトの `.remote-merge.toml`。後者が優先し、`[filter]` は和集合でマージする。

## Commands

| Purpose | Command |
|---|---|
| Build | `cargo build` |
| Test | `cargo nextest run`（未導入なら `cargo test`） |
| Check | `cargo fmt --all --check` / `cargo clippy --all-targets --all-features -- -D warnings` |
| Run | `cargo run -- status --left local --right <server>`（サブコマンドなしは使い方を出して終了コード 2） |
| Spec check | `kotowari check` |
| Real OpenSSH/sudo tests | `scripts/run-container-e2e.sh`（Docker 必須、通常テストと別パッケージ） |

## Conventions specific to this project

- ユーザー向けの文言と CLI ヘルプは英語。コード内のコメントは日本語でもよい。
- コミットメッセージは Conventional Commits 形式で、件名・本文は日本語。
- `lefthook.yml` は fmt・仕様検査を pre-commit、Clippy・通常テストを pre-push で読み取り専用で実行する。既存の個人フックを置き換えないため `lefthook install` は自動実行しない。既存フックの所有者が内容を確認し、必要なら手動で統合する。未ステージの変更も手動検査する場合は `lefthook run pre-commit --force --no-auto-install` / `lefthook run pre-push --force --no-auto-install` を使う。
- 旧 OpenSSH の手動負荷試行は `bench/legacy-ssh/setup.sh` から開始する。試行中のシェルを終了すると専用コンテナ・イメージ・鍵・known_hosts・データが破棄され、個人の SSH 設定は変更されない。CI の保証範囲には含めない。

## Constraints

- 読み取り中の SSH 接続エラーは一度だけ再試行する。書き込み前には対象の内容も再確認する（現行の更新時刻だけの検査は IR に対する実装課題）。
- シンボリックリンクは参照先のパスで比較し、バイナリファイルは SHA-256 で比較する。
- リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
