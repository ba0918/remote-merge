# Project Context

## What this is

SSH 経由でローカルと複数のリモートサーバのファイルを比較・マージする Rust の CLI ツール。非対話で動き、JSON 出力に対応する。正規化した仕様は `docs/ir/` に置く。旧仕様は `docs/archive/` に移行元として保存しており、内容を現行の動作と同一視しない。

## Stack and layout

- Rust。主な依存は tokio、russh、clap、similar、serde。
- ルートは Cargo ワークスペース。members と default-members は `crates/remote-merge/`、`crates/remote-merge-core/`、`crates/remote-merge-protocol/`、`crates/remote-merge-settings/`、`crates/remote-merge-ssh/`、`crates/remote-merge-config/`、`crates/remote-merge-agent/`、`crates/remote-merge-engine/`。`Cargo.lock`、`.cargo/`、release profile と `target/` はルートに置く。
- `crates/remote-merge-core/src/`: tree・diff・error・filter とその単体テスト。製品は同じ型を `remote_merge::{tree, diff, error, filter}` から再公開する。
- `crates/remote-merge-core/src/relative_path/`: root_dir からの相対パスの型 `RelPath`（入力の形の検査 `RelPath::parse` を通ったときだけ作れる。コマンドの入口・エージェントの受け取り口・バックアップの記録の読み込みで使う）と、root_dir の下の一つの項目を表す `EntryPath`（root_dir 全体を表せない型）。経路の操作（製品の `TargetIo`、engine の `local_io`、エージェントの受け取り口とクライアント、SSH の事実の集め方）は、項目の操作（読み書き・削除・権限の変更・symlink の作成・stat・ハッシュ・大きさ・項目のリンク先）では `EntryPath` を、範囲の操作（走査・子の一覧・パスの確認）では `RelPath` を受け取り、自分では入力の形も root_dir の外かどうかも検査しない。
- `crates/remote-merge-core/src/op_failure/`: 操作の失敗の一つの型 `OpFailure`（ない・ディレクトリでない・権限がない・上限を超える・道具がない・I/O・接続断・root_dir がないの八つ）、OS の誤りとリモートの道具の文言の終わりからの訳し方（`from_io_error`・`from_tool_message`）、大きさの上限の判定（`check_size`）、利用者に示す文言を一か所で作る `OpError`。どの経路も操作の結果をこの型で返し、扱い（止めるか続けるか）は経路の外で決める。
- `crates/remote-merge-core/src/machine_ops/`: このマシンの一つの項目への操作（読み込み・大きさ・ハッシュ・stat・項目のリンク先・その場で書き換える書き込み・削除・権限の変更・symlink の作成）。ローカルとエージェントは同じ関数を呼ぶ。中身の読み込みとハッシュは、辿った先が通常ファイルでなければ開いて待たずに "not a regular file" の I/O の失敗にする（REQ-paths-021）。`crates/remote-merge-core/src/route_choice/`: エージェントか SSH かをサーバにつなぐときに一度だけ選ぶ純粋な関数（TBL-ssh-001）。
- `crates/remote-merge-core/src/compare/`: 比べる流れ（docs/ir/compare/）の規則を純粋な関数で一つずつ置く。状態の組の判定（`judge`。TBL-compare-001）、中身のハッシュからの判定と読み込みの失敗の扱い（`contents`。TBL-compare-002・004）、コマンドと指定からの比べ方と二つの symlink の組の合わせ方（`method`。TBL-compare-003・REQ-compare-008）、変更の直前の判定（`recheck`。REQ-compare-015）、判定の値と判定できない理由の文（`verdict`。REQ-compare-014）。流れの組み立てのうち事実を受けて進める段も純粋な関数で、側ごとの一覧から状態の表を作り（`table`）、組ごとに判定して中身を求める側を決め（`pairs`）、読んだハッシュと中身で決め直す（`settle`）。コマンドが判定の扱いに使うハッシュと中身は `Uses` で告げる。
- `crates/remote-merge-core/src/scan/`: 走査の規則（exclude・include・件数・UTF-8 でない名前・循環・辿るか・読めないときに止めるか）を一か所に置いた純粋な関数群と、ローカルとエージェントが共有するこのマシンの集め方（`this_machine`）。子の一覧は走査の範囲の一つ（`ScanScope::Children`、`children`）で、集め方への問いを深さ一つにして同じ規則で作る。経路は生の項目を問いに答えて返すだけで、SSH の集め方は `remote-merge-ssh` の `scan_script`・`scan_output`・`scan_collect`（find にリンク先を辿らせない POSIX sh のスクリプト。初めに find が -printf と -exec … {} + を受け付けるかを確かめる）、エージェントは `tree_listing` で同じ規則を動かす。子の一覧の結果は三つの経路とも engine の `child_listing` で同じ形にし、子は走査と同じ件数の上限（max_scan_entries）で数えて、超えたら読めた子を持つ `ChildrenTruncated` の誤りにする。一覧の項目の種類は通常ファイル・ディレクトリ・symlink（リンク先がディレクトリか・ディレクトリでないか・ないか）・その他の種類（FIFO・ソケット・デバイス）で、その他の種類は段 5 まで古い木のノードにせずパスだけを `other_kinds` に残し、古い status・diff・merge・sync は engine の `service/other_kinds` でどの側の木からもそのパスを下ごと外して、判定できない（errors・failed、終了コード 2）とする。起点そのものだけを集める起点（`Start::itself`）は、比べる流れが名前で指定したパスと変更の直前の書き込み先の状態を集めるのに使う。製品の走査は `runtime/scan_entry.rs` の一つの入口（include の値の確認と起点の決め方）を通り、diff の名前で指定したディレクトリの展開も `cli/diff_expansion.rs` から同じ入口を使う。
- 項目そのもの（最後の要素のリンクは辿らない）のリンク先は、ローカルとエージェント（ReadEntryLink の要求）は `machine_ops::entry_link` を、SSH の経路は `remote-merge-ssh` の `entry_links`（シェルのテストと readlink、ないか入れない理由は `ls -d`）を使う。merge で比べたときに symlink だった書き込み先の確認に使う。SSH の書き込みと symlink の作成のコマンドは `change_scripts`（親の作成を含む一つのコマンド。失敗の種類は標準エラーの文言の終わりで決める）、ハッシュを取る道具のないサーバの中身のハッシュは `contents_stream` に置く。
- `crates/remote-merge-core/src/path_resolution/`: パスを "/" から要素ごとに辿る手順（`resolve_path`）、確認の結果の四つの種類と原因の文言の一覧、このマシンの基本操作（`os`。Unix では標準ライブラリと libc、Unix 以外では標準ライブラリだけを使う）、集めた要素の事実から次に尋ねる要素を決める手順（`facts`）。ローカルとエージェントは同じ `os::resolve_on_this_machine` を呼ぶ。SSH は `remote-merge-ssh` の `path_facts`（スクリプトの `path_facts_script` と出力の読み取りの `path_facts_output`）でまだ辿っていない要素の事実をまとめて尋ね、`facts` の手順に渡す。製品の入口は `runtime/ssh_inspect.rs`。
- `crates/remote-merge-protocol/src/lib.rs`: Agentの通信型・シリアライズ・ハンドシェイクとその単体テスト。製品は `remote_merge::agent::protocol` から同じ型・定数・関数を再公開する。
- `crates/remote-merge-protocol/src/framing.rs`: 長さプレフィクス付きフレームの読み書きとその単体テスト。製品は既存の `remote_merge::agent::framing` から再公開する。
- `crates/remote-merge-settings/src/lib.rs`: 標準ライブラリだけに依存する接続設定の5型と既存のDebug・Default実装。configと製品は同じ型を再公開する。
- `crates/remote-merge-config/src/lib.rs`: 設定の読み込み・認証の解決・パースと既存89単体テスト。製品の `src/config.rs` は既存公開パスを明示的に再公開する。expand_tildeは下位のconfigに所属し、SSHから参照する。
- `crates/remote-merge-ssh/src/`: 全SSH実装と単体テスト。製品は `remote_merge::ssh::{client, hint, host_key_verifier, passphrase_provider}` を公開のまま再公開し、`ssh::tree_parser`（書き込み・削除・symlink の作成のコマンドの組み立て）は製品crate内だけの再公開を保つ。SSH の事実の集め方は、パスの確認の `path_facts*`、大きさ・中身・ハッシュをまとめて尋ねる `contents*`、走査と子の一覧の `scan_*` に置き、一回のコマンド行の長さは `command_limit` で分ける。答えの目印が欠けるか問いと合わないときは接続断にする。known_hosts・known_hosts_io・preferredはSSH crate内だけに置く。設定型はsettings、ホーム展開と既定の上限はconfig、errorはcoreの同じ項目を使う。strict実行のinherent methodはcrate内に保ち、製品の呼び出しは内部再公開した委譲関数を使う。製品のtest-utils featureはSSHの同名featureへ転送する。
- `crates/remote-merge-agent/src/agent/`: Agentの実行処理・SSH転送と配置処理、既存191単体テスト。製品は既存 `remote_merge::agent` の同じ公開パスから再公開する。通信型とframingはprotocol、filter/treeはcore、shell escapeは通常依存のSSHを使う。配置の四つの版依存関数は製品側の旧signatureのwrapperから製品CLI_VERSIONを受け取る。版に関わる既存88テストとtest-only SHA helperは製品側に残す。両crateのbuild.rsはCargoのTARGETをそのままcompile時定数へ渡す。
- `crates/remote-merge-engine/src/`: local・backup・backup_store・merge/executor、共有Side、service/types・max_files・path_resolver・kind_skip・fast_path・diff・status・status_ref_badge・status_unreadable・content_reads・merge・merge_reference・merge_reference_reads・merge_flow・sync・rollbackと既存388単体テスト。content_readsはパスごとに中身か失敗の理由を持つ読み込みの結果の型、merge_referenceは参照先を使う merge の三者の扱いの純粋な表、merge_reference_readsはツリーと読み込みの結果からその表の入力を求める処理、status_ref_badgeは参照先との違いの印、status_unreadableは status で読めなかったファイルの取り出しを持つ。kind_skipは merge と sync の対象から、種類の違うパスを一件のスキップにしてその配下を外し、ディレクトリとディレクトリへの symlink の組は組のパスだけを外して中を比べさせる判定を持つ。diff・merge・sync の引数は path_resolver の `parse_path_arguments` で `RelPath` にする。Sideの正本は `side` に置き、製品の `side` は同じ型・関数を再公開する。serviceのモジュールも製品の同じパスから同じ項目を再公開する。three_wayは三者の存在・内容比較だけを持つ。executorは既存公開項目を明示的に再公開する。backup_storeは製品runtimeからcrate内だけに再公開する。coreとconfigの同じ型・定数を参照する。serdeの既存JSON表現は維持し、serde_jsonは保存レコードの読み書きにも使う通常依存に置く。
- `crates/remote-merge-engine/src/local_io.rs`: このマシン（ローカルの経路）の範囲の操作：パスの確認（core の path_resolution の確認の結果をそのまま返す）、走査、子の一覧。一つの項目への操作は core の `machine_ops` にあり、製品の `runtime/local_ops.rs`・`runtime/local_target.rs` がそれを `TargetIo` につなぐ。`crates/remote-merge-engine/src/scan_result.rs` と `child_listing.rs` は、三つの経路の走査の結果を一覧・ツリー・子の一覧にする一つの関数を持つ。
- `crates/remote-merge-engine/src/service/merge_flow.rs`: ソース存在検査、比べたときに symlink だった書き込み先が同じリンク先のままかの検査、hunk対象検査とバイト列からのhunk準備。製品service/merge_flowは三つの検査を同じ公開パスから再公開し、両側read・dry-run・backup・writeと既存十八テストを保つ。
- `crates/remote-merge-engine/src/service/rollback.rs`: 復元パス判定に加え、検査失敗または既存の二つの拒否理由を要求全体の結果へ展開する純粋なrestore_request_refusalを持つ。backup_storeは保存レコードを同じBackupPathRecordへ変換するFrom実装を持つ。製品SideIOはprivate変換wrapperと既存116混在テストを保ち、第一passの逐次read・検査、session予約、第二passのread・dry-run・pre-backup・write、最後のfinishを担当する。
- `crates/remote-merge/src/cli/` と `crates/remote-merge/src/service/`: CLI と共通処理。`crates/remote-merge/src/runtime/`: CLI の実行時処理。`crates/remote-merge/src/telemetry/`: 診断ログの保存と閲覧。`crates/remote-merge/src/ssh/` と `crates/remote-merge/src/agent/`: リモート接続と転送。
- `crates/remote-merge/src/runtime/compare_flow.rs`・`compare_reads.rs`・`compare_checks.rs`: 比べる流れの組み立て。一つ目の段（`collect_compare_states`）で左右と参照先を同じ範囲の走査（`side_facts.rs` の側ごとの走査の一覧）で集め、残りの段（`compare_collected`）で判定が求めたハッシュとコマンドが告げたハッシュ・中身だけを側ごとにまとめて取る（`side_facts.rs` の `hash_contents`・`read_contents`。ハッシュは symlink も辿った先の中身で、大きさの上限を当てない）。`compare_starts.rs` は起点ごとの事実を集める：ディレクトリの下の起点そのものは器でなく起点そのものを集め直して状態にし、途中がディレクトリでない名前の起点はその側にないとして外し（起点がなくなっても root_dir は確かめる）、途中が root_dir の外へ解決される起点の下は集めない。起点の途中と一覧の symlink の事実は `TargetIo::inspect_paths` で経路にまとめて一度に尋ねる（SSH は段 3 のまとめて尋ねるパスの確認、エージェントは `InspectPaths`）。判定の規則は core の `compare/starts.rs`。`recheck_before_change` は変更の直前に書き込み先のそのパスだけを集め直して判定する。段 5 で diff・status・merge・sync・rollback をこの流れに載せ替えるまで、古いコマンドは使わない。
- `crates/remote-merge/src/runtime/` の経路: サーバごとに選んだ経路を `route_state` に記録し（選び直さない）、`route_retry` が接続断のときの扱い（読み取りは同じ経路でつなぎ直して一度だけやり直す、変更はやり直さずに止める）を一か所で決める。エージェントの経路は `agent_route/`、SSH の経路は `ssh_inspect`・`ssh_contents`・`ssh_scan`・`ssh_changes`・`ssh_entry_links`、ローカルは `local_ops`・`local_target`。
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
| Old remote tools | `REMOTE_MERGE_TEST_SCAN_IMAGE=centos:5 cargo nextest run -p remote-merge-ssh`（Docker 必須。centos:5〜7・busybox:1.37.0 などのコンテナの sh と道具でリモートのスクリプトを動かす。イメージを指定しないと、コンテナでしか確かめられない試験は飛ばす。`REMOTE_MERGE_TEST_REQUIRE_SCAN_IMAGE=1` を置くと、飛ばさずに失敗にする。`REMOTE_MERGE_TEST_REQUIRE_FISH=1` は fish がないときに同じく失敗にする） |

## Conventions specific to this project

- ユーザー向けの文言と CLI ヘルプは英語。コード内のコメントは日本語でもよい。
- コミットメッセージは Conventional Commits 形式で、件名・本文は日本語。
- `lefthook.yml` は fmt・仕様検査を pre-commit、Clippy・通常テストを pre-push で読み取り専用で実行する。既存の個人フックを置き換えないため `lefthook install` は自動実行しない。既存フックの所有者が内容を確認し、必要なら手動で統合する。未ステージの変更も手動検査する場合は `lefthook run pre-commit --force --no-auto-install` / `lefthook run pre-push --force --no-auto-install` を使う。
- 旧 OpenSSH の手動負荷試行は `bench/legacy-ssh/setup.sh` から開始する。試行中のシェルを終了すると専用コンテナ・イメージ・鍵・known_hosts・データが破棄され、個人の SSH 設定は変更されない。CI の保証範囲には含めない。

## Security boundary

レビューや実装で security の指摘を直すかを判断するときは、この節に照らす。決定は [foundation A11](docs/decision/records/2026-10-12-foundation.md#A11)。

- 信頼するもの: 操作する利用者、接続先のサーバ、配置先に書ける組織内のアカウント。配置先は組織内の誰でも書ける共有ディレクトリが普通である。
- 直さないもの: その中の誰かが、ツールの確かめと変更の間にすり替える（ハードリンク・symlink・rename の差し替え）攻撃。
- 直すもの: 同時に動く別の作業（並行するデプロイ・ほかの利用者のふつうの編集）で、ツールが壊す・誤った結果を返す・黙って失敗する正しさの問題。信頼しない入力（ネットワーク越しの応答の解釈、パスの形）からのコマンド注入と、root_dir の外への書き込み。

## Constraints

- 読み取り中の SSH 接続エラーは一度だけ再試行する。書き込み前には対象の内容も再確認する（現行の更新時刻だけの検査は IR に対する実装課題）。
- シンボリックリンクは参照先のパスで比較し、バイナリファイルは SHA-256 で比較する。
- リモート間 merge には確認が必要。CLI の比較先指定は `--left` / `--right`。
