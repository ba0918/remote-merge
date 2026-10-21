# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust の CLI ツール。非対話で動き、JSON 出力に対応する。正規化した仕様は `docs/ir/` に置く。文書の入口は `docs/README.md`。

## Stack and layout

- Rust。主な依存は tokio、russh、clap、similar、serde。
- ルートは Cargo ワークスペース。members と default-members は `crates/remote-merge/`、`crates/remote-merge-core/`、`crates/remote-merge-protocol/`、`crates/remote-merge-settings/`、`crates/remote-merge-ssh/`、`crates/remote-merge-config/`、`crates/remote-merge-agent/`、`crates/remote-merge-engine/`。`Cargo.lock`、`.cargo/`、release profile と `target/` はルートに置く。
- `crates/remote-merge-core/src/`: diff・error・filter とその単体テスト。製品は同じ型を `remote_merge::{diff, error, filter}` から再公開する。
- `crates/remote-merge-core/src/content/` は内容指定、厳密なデコード、BOM と元バイトの行範囲の唯一の構築処理を持つ。diff の PairScript は改行を含む行から変更列を作り、表示、競合、元バイトの選択へ投影する。config の content は二層の内容設定と接続先名、相対パス、CLI 指定の解決を持つ。engine の treat/content は PathComparison と実際の Side から一つのパスの解釈を作り、diff と部分マージが共有する。raw の比較と I/O と保存形式はこの解釈を使わない。
- `crates/remote-merge-core/src/relative_path/`: root_dir からの相対パスの型 `RelPath`（入力の形の検査 `RelPath::parse` を通ったときだけ作れる。コマンドの入口・エージェントの受け取り口・バックアップの記録の読み込みで使う）と、root_dir の下の一つの項目を表す `EntryPath`（root_dir 全体を表せない型）。経路の操作（製品の `TargetIo`、engine の `local_io`、エージェントの受け取り口とクライアント）は、項目の操作（読み書き・削除・権限の変更・symlink の作成・stat・ハッシュ・大きさ・項目のリンク先）では `EntryPath` を、範囲の操作（走査・子の一覧・パスの確認）では `RelPath` を受け取り、自分では入力の形も root_dir の外かどうかも検査しない。
- `crates/remote-merge-core/src/op_failure/`: 操作の失敗の一つの型 `OpFailure`（ない・ディレクトリでない・権限がない・上限を超える・I/O・接続断・root_dir がないの七つ）、OS の誤りからの訳し方（`from_io_error`）、大きさの上限の判定（`check_size`）、利用者に示す文言を一か所で作る `OpError`。どの経路も操作の結果をこの型で返し、扱い（止めるか続けるか）は経路の外で決める。
- `crates/remote-merge-core/src/machine_ops/`: このマシンの一つの項目への操作（読み込み・大きさ・ハッシュ・stat・項目のリンク先・その場で書き換える書き込み・削除・権限の変更・symlink の作成）。ローカルとエージェントは同じ関数を呼ぶ。中身の読み込みとハッシュは、辿った先が通常ファイルでなければ開いて待たずに "not a regular file" の I/O の失敗にする（REQ-paths-021）。`crates/remote-merge-core/src/route_choice/`: エージェントで操作するか、使えずに読み書きの前に止めるかをサーバにつなぐときに一度だけ決める純粋な関数（TBL-ssh-001）。
- `crates/remote-merge-core/src/sha256.rs`: 中身の SHA-256 を小文字の16進で表す処理（バイト列から・読み手から少しずつ）。製品のコードは項目のハッシュ、バイナリの比較、変更の直前の判定、バックアップの置き場所の名前、エージェントの配置と手元のバイナリの照合でこれを使う。
- `crates/remote-merge-core/src/compare/`: 比べる流れ（docs/ir/compare/）の規則を純粋な関数で一つずつ置く。状態の組の判定（`judge`。TBL-compare-001）、中身のハッシュからの判定と読み込みの失敗の扱い（`contents`。TBL-compare-002・004）、コマンドと指定からの比べ方と二つの symlink の組の合わせ方（`method`。TBL-compare-003・REQ-compare-008）、変更の直前の判定（`recheck`。REQ-compare-015）、判定の値と判定できない理由の文（`verdict`。REQ-compare-014）。流れの組み立てのうち事実を受けて進める段も純粋な関数で、側ごとの一覧から状態の表を作り（`table`）、組ごとに判定して中身を求める側を決め（`pairs`）、読んだハッシュと中身で決め直す（`settle`）。コマンドが判定の扱いに使うハッシュと中身は `Uses` で告げる。
- `crates/remote-merge-core/src/scan/`: 走査の規則（exclude・include・件数・UTF-8 でない名前・循環・辿るか・読めないときに止めるか）を一か所に置いた純粋な関数群と、ローカルとエージェントが共有するこのマシンの集め方（`this_machine`）。子の一覧は走査の範囲の一つ（`ScanScope::Children`、`children`）で、集め方への問いを深さ一つにして同じ規則で作る。経路は生の項目を問いに答えて返すだけで、エージェントは `tree_listing` で同じ規則を動かす。子の一覧の結果はローカルとエージェントのどちらの経路も engine の `child_listing` で同じ形にし、子は走査と同じ件数の上限（max_scan_entries）で数えて、超えたら読めた子を持つ `ChildrenTruncated` の誤りにする。一覧の項目の種類は通常ファイル・ディレクトリ・symlink（リンク先がディレクトリか・ディレクトリでないか・ないか・辿れないか）・その他の種類（FIFO・ソケット・デバイス）で、比べる流れはその他の種類を判定できない（REQ-compare-009）とする。子の一覧は、`接続断` の後に権限の変更を確かめるときに親の子の一覧から権限を読むのに使う。起点そのものだけを集める起点（`Start::itself`）は、比べる流れが名前で指定したパスと変更の直前の書き込み先の状態を集めるのに使う。製品の走査は `runtime/scan_entry.rs` の一つの入口（include の値の確認と起点の決め方）を通る。起点そのものだけを集める起点には exclude を当てない（名前で指定したパスにフィルタを当てない。REQ-cli-081）。
- 項目そのもの（最後の要素のリンクは辿らない）のリンク先は、ローカルとエージェント（ReadEntryLink の要求）とも `machine_ops::entry_link` を使う。merge で比べたときに symlink だった書き込み先の確認に使う。
- `crates/remote-merge-core/src/path_resolution/`: パスを "/" から要素ごとに辿る手順（`resolve_path`）、確認の結果の四つの種類と原因の文言の一覧、このマシンの基本操作（`os`。Unix では標準ライブラリと libc、Unix 以外では標準ライブラリだけを使う）。ローカルとエージェントは同じ `os::resolve_on_this_machine` を呼ぶ。
- `crates/remote-merge-protocol/src/lib.rs`: Agentの通信型・シリアライズ・ハンドシェイクとその単体テスト。製品は `remote_merge::agent::protocol` から同じ型・定数・関数を再公開する。
- `crates/remote-merge-protocol/src/framing.rs`: 長さプレフィクス付きフレームの読み書きとその単体テスト。製品は既存の `remote_merge::agent::framing` から再公開する。
- `crates/remote-merge-settings/src/lib.rs`: 標準ライブラリだけに依存する接続設定の5型と既存のDebug・Default実装。configと製品は同じ型を再公開する。
- `crates/remote-merge-config/src/`: TOML の形（`schema`）、設定元の探索・読み込みと root_dir の解決（`loading`）、優先規則と合成（`merge`）、設定型と値の検証・変換（`value`）を非公開モジュールに分ける。`lib.rs` と製品の `src/config.rs` は既存公開パスを明示的に再公開する。expand_tilde は下位の config に所属し、SSH から参照する。
- `crates/remote-merge-ssh/src/`: SSH の接続（client・ホスト鍵の確認・パスワードの解決・パスフレーズ・接続の失敗の手がかり）と、エージェントを置いて起動するコマンドの引数の引用（`quote`）と単体テスト。リモートのファイルの操作はエージェントだけで行うので、ファイルを読み書きするシェルのスクリプトは持たない（docs/decision/records/2026-10-12-foundation.md の A12）。製品は `remote_merge::ssh::{client, hint, host_key_verifier, passphrase_provider}` を公開のまま再公開する。known_hosts・known_hosts_io・password・preferred は SSH crate 内だけに置く。設定型はsettings、ホーム展開と既定の上限はconfig、errorはcoreの同じ項目を使う。strict実行のinherent methodはcrate内に保ち、製品の呼び出しは内部再公開した委譲関数を使う。製品のtest-utils featureはSSHの同名featureへ転送する。
- `crates/remote-merge-agent/src/agent/`: Agentの実行処理・SSH転送と配置処理、既存191単体テスト。製品は既存 `remote_merge::agent` の同じ公開パスから再公開する。通信型とframingはprotocol、filterはcore、shell escapeは通常依存のSSHを使う。配置の四つの版依存関数は製品側の旧signatureのwrapperから製品CLI_VERSIONを受け取る。版に関わる既存88テストとtest-only SHA helperは製品側に残す。両crateのbuild.rsはCargoのTARGETをそのままcompile時定数へ渡す。
- `crates/remote-merge-engine/src/`: local_io・backup・backup_store・scan_result・child_listing、共有 Side（`side`。製品の `side` は同じ型・関数を再公開する）、出力の型（`service/types`）とコマンドの入口のパスの検査（`service/path_resolver` の `parse_path_arguments`）と、その単体テスト。製品の `service` は同じ項目を同じパスから再公開し、出力の書き出し（`service/output`）は製品に置く。backup_store は製品の runtime から crate 内だけに再公開する。core と config の同じ型・定数を参照する。serde の既存 JSON 表現は維持し、serde_json は保存レコードの読み書きにも使う通常依存に置く。
- `crates/remote-merge-engine/src/local_io.rs`: このマシン（ローカルの経路）の範囲の操作：パスの確認（core の path_resolution の確認の結果をそのまま返す）、走査、子の一覧。一つの項目への操作は core の `machine_ops` にあり、製品の `runtime/local_ops.rs`・`runtime/local_target.rs` がそれを `TargetIo` につなぐ。`crates/remote-merge-engine/src/scan_result.rs` と `child_listing.rs` は、ローカルとエージェントの経路の走査の結果を一覧・子の一覧にする一つの関数を持つ。
- `crates/remote-merge-engine/src/treat/`: 比べる流れの判定を各コマンドの扱いに訳す純粋な関数。status の一覧と参照先の印（`status`・`status_listing`）、diff の項目・参照先との差・競合・errors・summary（`diff`・`diff_*`）、merge と sync の扱う範囲・扱いの並び・参照先の確認・結果（`merge_scope`・`merge_plan`・`merge_reference`・`merge_result`）と --hunks で当てる中身（`hunk_merge`）、sync の書き込み先の状態（`sync`）、rollback の書き戻し・スキップ・失敗（`rollback`）とセッションの選び方と終了コード（`rollback_session`）、比べる側の決め方（`sides`）、コマンドが比べる流れに告げるハッシュと中身（`uses`・`merge_uses`）、不完全な比較の誤り（`incomplete`）。経路は呼ばない。
- `crates/remote-merge/src/runtime/merge_apply.rs`・`merge_inspect.rs`・`merge_change.rs`・`merge_permissions.rs`: merge と sync の書き込み先への変更。書き込み先のパスと祖先をまとめて確かめ、バックアップする中身をまとめて読み、パスごとに変更の直前の判定（`recheck_before_change`）・バックアップ・変更・権限の順に行う。`rollback_facts.rs`・`rollback_apply.rs`・`rollback_restore_one.rs`: rollback の書き戻し。戻す先のパスとその親の確認をまとめて尋ね、確かめられたパスの状態を名前で指定したパスの走査で集め、treat の `rollback` で扱いを決めて書き戻す。書き戻しの最中の `接続断` の確かめは `change_check`・`confirm`・`restore_before`・`restore_saved`。 パスをまとめて確かめる問いは、どれも `root_checked.rs` で root_dir そのものを先頭に足して尋ね、その答えで root_dir がないことを決める（変更の前のまとめた問いで分かったときの、ファイルごとの error の割り当ても同じ場所）。
- `crates/remote-merge/src/cli/` と `crates/remote-merge/src/service/`: CLI と共通処理。`crates/remote-merge/src/runtime/`: CLI の実行時処理。`crates/remote-merge/src/telemetry/`: 診断ログの保存と閲覧。`crates/remote-merge/src/ssh/` と `crates/remote-merge/src/agent/`: リモート接続と転送。
- `crates/remote-merge/src/runtime/compare_flow.rs`・`compare_reads.rs`・`compare_checks.rs`: 比べる流れの組み立て。一つ目の段（`collect_compare_states`）で左右と参照先を同じ範囲の走査（`side_facts.rs` の側ごとの走査の一覧）で集め、残りの段（`compare_collected`）で判定が求めたハッシュとコマンドが告げたハッシュ・中身だけを側ごとにまとめて取る（`side_facts.rs` の `hash_contents`・`read_contents`。ハッシュは symlink も辿った先の中身で、大きさの上限を当てない）。`compare_starts.rs`・`compare_start_entries.rs` は起点ごとの事実を集める：ディレクトリの下の起点そのものは器でなく起点そのものを集め直して状態にし、途中がディレクトリでない名前の起点はその側にないとして外し（起点がなくなっても root_dir は確かめる）、途中が root_dir の外へ解決される起点の下は集めない。root_dir の状態は、どの範囲でも走査の初めの問いで一度だけ決める（このマシンの集め方は root_dir に入れるかも確かめる。E42）。外へのリンクを辿らないときの起点の途中と一覧の symlink の事実は、走査の後に `TargetIo::inspect_paths` で経路にまとめて一度に尋ねる（エージェントは `InspectPaths`）。判定の規則は core の `compare/starts.rs`。`recheck_before_change` は変更の直前に書き込み先のそのパスだけを集め直して判定する。diff・status・merge・sync はこの流れで比べ、rollback はこの流れの集め方（`collect_side` と状態の表）で戻す先の状態を集める。
- `crates/remote-merge/src/runtime/` の経路: リモートはエージェントだけで操作し、つなぐときに一度だけエージェントを使えるかを決めて `route_state` に記録し（使えなければ読み書きの前に止める。決め直さない）、`route_retry` が接続断のときの扱い（読み取りはつなぎ直して一度だけやり直す、変更はやり直さずに止める）を一か所で決める。エージェントの経路は `agent_route/`、ローカルは `local_ops`・`local_target`。
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
