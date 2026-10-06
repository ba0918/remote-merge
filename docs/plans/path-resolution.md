# Plan: パスをどの経路でも同じ手順で辿る

## Goal

rollback・diff・merge・sync が使うパスの確認が、ローカル・SSH・エージェントのどの経路でも同じ手順で "/" から要素ごとに辿り、同じファイルシステムの状態に対して同じ結果か同じ "cannot resolve path: " の文言のエラーを返す。

## Specification

IR は `docs/ir/`。この計画が扱う要件と例:

- `docs/ir/paths/resolution.md#REQ-paths-001` から `#REQ-paths-007`、例 EX-paths-001 から EX-paths-018 と EX-paths-023
- `docs/ir/paths/routes.md#REQ-paths-008` から `#REQ-paths-011`、例 EX-paths-019 から EX-paths-022 と EX-paths-024
- 振る舞いを保つ既存の要件: `docs/ir/backup/rollback-path.md#REQ-backup-003`、`#REQ-backup-031`、`#REQ-backup-033`（例 EX-backup-027、EX-backup-029、EX-backup-030 とその三つの経路のテスト）、`docs/ir/cli/symlink-diff.md#REQ-cli-026`

判断の根拠は `docs/decision/records/2026-10-06-path-resolution.md` の A1〜A47（A15・A34・A35 は A43・A46・A44 に置き換わっている）。A40・A41 はこの計画で決めた（置き場所とテストの置き方）。

背景: このブランチ（path-validation）では、SSH の確認スクリプトにカーネル・realpath・`Path::parent()` の振る舞いを真似させる修正を三回重ねたが、そのたびに新しい経路差か退行が見つかった。findings ファイル `.agents/artifacts/reviews/path-validation.json`（作業ツリーの外の記録で、実装者は読まなくてよい）の 15〜17 がその残りで、次の形をしている。この計画の例がそれぞれを押さえる。

- 15: symlink の先の実ディレクトリが長く、そこに存在しない名前をつなぐと 4096 バイトを超える（EX-paths-008）
- 16: 4095 バイトの "/" で終わるリンク先（EX-paths-009）
- 17: 連なったリンクの先が "/" の付いた通常ファイルやループ（EX-paths-013、EX-paths-018）

## Approach and why

今の状態（HEAD 487e629 のコードで確かめた事実）:

- 確認の結果の型 `TargetPath` は `crates/remote-merge-engine/src/local_io.rs` にあり、Missing{real_parent}・File{real_path}・Symlink{link_target, real_path} の三つ。
  - ローカルは `inspect_local_path`（同ファイル）で、`std::fs::canonicalize` と `resolved_symlink`・`resolved_missing_parent` を使う。
  - エージェントは `crates/remote-merge-agent/src/agent/file_io.rs` の `inspect_path` で、同じ処理を書き写している。
  - SSH は `crates/remote-merge/src/runtime/remote_path.rs` の sh のスクリプトと、NUL 区切りの出力の読み取り（`build_inspect_path_command`・`parse_inspect_path_output`）。
- エージェントの通信の型は `crates/remote-merge-protocol/src/lib.rs` の `AgentPathInspection`（Missing・File・Symlink・Error）。通信の版は 4 で、交渉の結果は両者の小さい方になる。
- rollback は、確認がエラーになったとき `is_regular_file`（ローカルは metadata、SSH は `[ -f ]`、エージェントは StatFiles の mode のビット）で祖先を一つずつ問い合わせ、`crates/remote-merge-engine/src/service/rollback.rs` の `restore_path_after_failed_inspection` でスキップに読み替えている。
  - 呼び出しは `crates/remote-merge/src/runtime/side_io.rs` の `inspect_restore_path`。
  - 親が存在するかは、親をもう一度確認して調べている。
- 確認の結果を使う側は次のとおり。
  - diff: `crates/remote-merge/src/cli/diff.rs` の `real_root_dir`・`resolved_path_outside_root`・循環の検出・`link_target_for_diff`・root の外のリンクの判定（300 行目付近の `_ => false` は確認のエラーを握りつぶしている）
  - merge: 製品側の `crates/remote-merge/src/service/merge_flow.rs`（`save_symlink_update`・`target_path_exists`・`save_backup_if_exists`・`missing_parent_directories`・`is_terminal_symlink` の呼び出し）と、それを実装する `side_io.rs` の関数
  - バックアップの作成: `crates/remote-merge/src/runtime/core.rs` の `save_backup_if_exists`・`save_symlink_update`
  - rollback: `side_io.rs` の `restore_backup`・`inspect_recorded_path`
- crate の依存: エージェントは core・protocol・ssh に依存し、engine には依存しない。engine は core・config・protocol に依存する。core には libc の依存がない。
- SSH のスクリプトは root_dir と相対パスをつないだ一つのパスだけを受け取る（`remote_io.rs` の `build_inspect_path_command(path)` の呼び出し）。`runtime/mod.rs` の `mod remote_path;` は非公開。
- "cannot resolve path: " の接頭辞を付けているのは今は rollback だけで（`rollback.rs` の `restore_request_refusal` と `side_io.rs` の第二 pass）、確認の手順の外の失敗（SSH の切断など）にも付けている。merge の失敗はエラーの表示をそのまま出す（`cli/merge.rs`）。
- `crates/remote-merge/tests/contract.rs` では、SSH とエージェントの契約テストのモジュールに `#[cfg(feature = "test-utils")]` が付いており、`--all-features` を付けないと動かない。

方針:

- 辿る手順を remote-merge-core の新しいモジュールに、一つの関数として置く（A22・A40）。root_dir と相対パスを別々に受け取り、root_dir の要素と相対パスの要素を区別する（REQ-paths-002 の root_dir 自身より上の扱いと、REQ-paths-004 の root_dir のリンクの数え方に要る）。
  - 基本操作（lstat の種類、readlink、access(X_OK)）を外から渡せる形にする。
  - 標準ライブラリと libc で実装した基本操作も core に置き、ローカルとエージェントはどちらもその組み合わせを呼ぶ。こうするとローカルとエージェントは同じコードになり、経路差の余地は SSH のスクリプトだけに残る。
  - 結果の型も core に置き、REQ-paths-002 の四つの種類と原因の文言の一覧を型で表す。存在しないには親が存在するかの印、途中の祖先がディレクトリでないには実パスと通常ファイルかそれ以外かを持たせる。
  - 確認のエラーの型の表示は "cannot resolve path: <原因>" とする。rollback はこの型のエラーには接頭辞を足さず、確認の手順の外の失敗にだけ今までどおり接頭辞を足す（二重にしない）。
  - 検索権限は実効 uid の access（faccessat の AT_EACCESS）で判定する（A46）。UTF-8 でない部分は "non-UTF-8 path" で止める（A44）。
  - engine の `TargetPath` はこの型に置き換えるか、この型から作る。
- 手順は要素ごとに、REQ-paths-001 の順（名前の長さ、つないだ長さ、検索権限、存在と種類、ディレクトリか）で確かめる。
  - 上限（4096 バイト、255 バイト、40 回）は固定の定数とし、OS に届く前に自前で止める（REQ-paths-003・REQ-paths-004）。
  - 辿っている実パスは常に "/" から始まる symlink を含まない文字列として持ち、".." はその文字列の親に移る。
- SSH のスクリプトは同じ手順を POSIX sh で書き直す（A15）。
  - 使ってよいのは `[ -L ]`・`[ -d ]`・`[ -e ]`・`[ -x ]`・`[ -f ]`、組み込みの printf と command -v、オプションなしの readlink だけ（A43）で、readlink -f・getconf・今の kernel_form や rust_parent は使わない。
  - root_dir と相対パスを別々の引数で渡す。
  - 長さは `${#var}` によりバイトで数える。`LC_ALL=C` はスクリプトの中で代入せず、製品が `sh -c` を呼ぶときの環境として外から渡す（BusyBox ash で途中の代入が効くかは確かめていないため）。
  - 出力は今と同じく NUL 区切りとし、四つの種類とエラーの原因の文言を返す。製品側の読み取りは SSH の出力を信頼しない入力として扱い、一覧にない原因の文言、欠けた欄、0 でない終了コードはどれもエラーにする（握りつぶさない）。
- エージェントの通信の型を REQ-paths-002 に合わせて変え、通信の版を 5 に上げる（A7）。版が 5 未満ならパスの確認だけ SSH で行い、sudo を使うサーバではエラーにする（REQ-paths-010）。この振り分けは「交渉した版と sudo の有無から確認の経路を選ぶ」純粋関数として製品に置き、契約テストから呼べるようにする（版 4 のエージェントを試験で配置できないため）。
- 確認の結果を使う側を置き換える。
  - rollback は途中の祖先がディレクトリでない（通常ファイル）をスキップ（REQ-backup-031）、それ以外をエラー（REQ-backup-003）に読み、存在しないの親の印で親がないことを判定する。
  - これで `is_regular_file` と `restore_path_after_failed_inspection` と、`inspect_restore_path` の存在しないの分岐で親をもう一度確認する処理は要らなくなるので消す。
  - SymlinkUpdate の記録の確認（`inspect_recorded_path` の分岐と `core.rs` の `save_symlink_update`）は、リンク自身の親の実パスが要るので親の確認を残す。その分岐で symlink 以外をすべて "symlink changed after merge" に倒している let-else は、途中の祖先がディレクトリでない（通常ファイル以外）とエラーを REQ-backup-003 のエラーとして返すように直す。
  - rollback 以外は途中の祖先がディレクトリでないを "cannot resolve path: not a directory" のエラーにする（REQ-paths-011）。S1 ではこの変換を一か所（製品の `target_io.rs` の再公開の層）に置き、使う側の match が新しい種類を見ないようにする。
  - diff の root の外のリンクの判定で確認のエラーを `_ => false` で握りつぶしている箇所は、diff の既存のパスごとのエラーの扱いに渡す。
- 三つの経路の一致は、製品の crate の `tests/contract/` で確かめる（A14・A39・A41）。
  - 同じ一時ディレクトリに状態を作り、ローカルの関数、SSH のスクリプトをその場の `sh` で動かして製品の読み取りを通した結果、エージェントの Dispatcher に InspectPath を送り応答を通信の形式で往復させて製品の変換を通した結果を、種類と実パスのバイト列とエラーの文言で比べる。
  - 普通の状態（上限から遠く、"/" で終わらないリンク先）では、新しい手順の実パスが `std::fs::canonicalize` の結果とバイト列で一致することも確かめる（A13）。
  - EX-paths-001〜018 の状態の表に加え、連なった symlink・"."・"//"・".."・"/" で終わるリンク先・4096 バイト前後の長さを組み合わせて作る proptest を置く。
  - 今のモジュール内の比べるテスト（`remote_path.rs` の `script_and_local_inspection_agree_*`）と、その失敗入力 `proptest-regressions/src/runtime/remote_path.txt` は、S1 でローカルが変わると古いスクリプトと必ず食い違うので S1 で消し、S3 で契約テストとして置き直す。
  - 印は例の ID（`@kotowari[EX-paths-0nn]`）だけでは要件のテストに数えられないので、各要件に `@kotowari[REQ-paths-0nn]` を付けたテストを少なくとも一つ置く。REQ-paths-008 は proptest に付ける。
  - SSH のスクリプトは dash（このマシンの /bin/sh）と BusyBox ash（E2E のコンテナの /bin/sh）で確かめる。製品は `sh -c` で包むので、ログインシェルが fish でもスクリプトは sh で動く。

変更の照合（`.kotowari/config.yaml` の changes が有効）:

- 比較元はこのブランチの分岐点 `213a8def195d5026eef2f07e6930eafd6a104693`（path-validation の開始点）、対象は最終の HEAD の完全なコミット ID。
- 今の `.kotowari/changes/implementation.yaml` はコードと IR が変わるので無効になる。実装者が最後に同じ比較元で作り直す。
- 実装と別のレビュー担当の `.kotowari/changes/review.yaml` は、サイクルの最後のレビューで作る。
- 統合の条件は `kotowari check --format json` と `kotowari changes --base 213a8def195d5026eef2f07e6930eafd6a104693 --head <最終 HEAD> --phase review --format json` の両方が終了コード 0。

## Scope of change

- `crates/remote-merge-core/`
  - 新しいモジュール（辿る手順・結果の型・標準ライブラリと libc の基本操作）とその宣言、Unix 向けの libc の依存、そのモジュールの単体テスト
- `crates/remote-merge-engine/src/local_io.rs`
  - `TargetPath`・`inspect_local_path`・`resolved_symlink`・`resolved_missing_parent`・`is_regular_file` に当たる自由関数とそのテスト。読み書きの関数は触らない
- `crates/remote-merge-engine/src/service/rollback.rs`
  - `CurrentRestorePath` を作る判定、`restore_path_after_failed_inspection` の削除、そのテスト
- `crates/remote-merge-protocol/src/lib.rs`
  - `AgentPathInspection`、通信の版、そのテスト
- `crates/remote-merge-agent/src/agent/`
  - `file_io.rs` の `inspect_path` と書き写しの `resolved_*`、`dispatch.rs` の InspectPath の応答、`client.rs` の inspect_path、それらのテスト
- `crates/remote-merge/src/runtime/`
  - `remote_path.rs`（スクリプトと読み取りの書き直し、`[ -f ]` のコマンドの削除）
  - `remote_io.rs` の確認の呼び出し
  - `target_io.rs`（`TargetPath` の再公開、`is_regular_file` の削除、通信の版によるエージェントと SSH の振り分け）
  - `side_io.rs`（`try_agent_inspect_path`・`inspect_restore_path`・`inspect_recorded_path`・`missing_parent_directories`・`target_path_exists`・`is_terminal_symlink`・`try_agent_*` の祖先の問い合わせの削除、そのテスト）
  - `core.rs` のバックアップの作成で確認の結果を読む箇所
  - `mod.rs` の `remote_path` の公開（test-utils の機能のときだけ）
- `crates/remote-merge/src/service/merge_flow.rs`
  - 確認の結果を読む箇所だけ
- `crates/remote-merge/src/cli/diff.rs`
  - 確認の結果を読む箇所だけ
- `PROJECT.md`
  - core に置くモジュールと local_io のパス検査の記述だけ
- テスト
  - `crates/remote-merge/tests/contract/` の新しいファイル（例と三つの経路の比較）と `crates/remote-merge/tests/contract.rs` の mod 行（SSH のスクリプトかエージェントを使うファイルには `#[cfg(feature = "test-utils")]` を付ける）
  - 既存の `backup_rollback_path.rs`・`rollback_path_ssh.rs`・`rollback_path_agent.rs`・`ssh_server.rs`（試験 SSH サーバの模擬の応答）の、出力の形の変更に合わせる修正だけ
  - `proptest-regressions/` の古い失敗入力の削除と新しい失敗入力
- `.kotowari/changes/implementation.yaml`

## Step order and prerequisites

S1 が手順と結果の型を作り、S2（エージェント）と S3（SSH）はそれに合わせる。S3 の比べるテストはローカルとエージェントが同じ関数を呼ぶ S2 の後でないと三つの経路を比べられない。S4 は三つの経路が新しい結果を返すようになってから使う側を置き換える。S5 は全体の記録と検証で最後に置く。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-paths-001, REQ-paths-002, REQ-paths-003, REQ-paths-004, REQ-paths-005, REQ-paths-006, REQ-paths-007 | EX-paths-001 から EX-paths-018, EX-paths-023（ローカルの経路） |
| S2 | REQ-paths-010 | EX-paths-021, EX-paths-024 |
| S3 | REQ-paths-008, REQ-paths-009 | EX-paths-019, EX-paths-020（EX-paths-001 から EX-paths-018 を三つの経路で） |
| S4 | REQ-paths-011, REQ-backup-031, REQ-backup-003, REQ-backup-033 | EX-paths-022, EX-backup-027, EX-backup-029, EX-backup-030 |
| S5 | なし（記録と全体の検証） | なし |

## Left to the implementer

- core の新しいモジュールと型と関数の名前、基本操作を渡す形（trait か関数の組か）
- engine の `TargetPath` を core の型に置き換えるか、core の型から作るか
- SSH のスクリプトの関数の分け方と、出力の種類を表す文字列
- 契約テストのファイル名と、proptest の生成器の形と件数（数秒で終わる件数にする）

## Stop conditions

- 確認するパスとして、root_dir と組み合わせて "/" から辿れない値（絶対パスや ".." を含む値）が S1 の関数に届く経路が見つかった
- root_dir が絶対パスでない値のまま S1 の関数か SSH のスクリプトに届く経路が見つかった
- root でモード 000 のディレクトリを確認したとき、SSH の `[ -x ]`（dash か BusyBox ash）とローカルの実効 uid の access が食い違う（A46）
- 例の状態がその OS では作れないと分かった（例の書き直しは IR の変更なので戻す）
- POSIX sh の基本操作では、REQ-paths-001 の順（とくに検索権限と存在の区別）を A35 の既知の例外以外でローカルと一致させられないと分かった
- 通信の版を 5 に上げると、版 4 のエージェントとの交渉や配置の判定が壊れると分かった
- 普通のパス（上限に遠く、"/" で終わらないリンク先）で、新しい手順の実パスが今の canonicalize の結果とバイト列で違うと分かった（A13 の見込みが外れた）
- 既存のテストが、出力の形の変更以外の理由で失敗する
- diff の root の外のリンクの判定に、確認のエラーを渡せるパスごとのエラーの扱いがない

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

このマシンでは重いコマンドがメモリを使い切ると WSL ごと落ちるので、cargo と git commit は上のように systemd-run で囲む。`--all-features` がないと test-utils の付いた SSH とエージェントの契約テストがコンパイルされない（CI と pre-push も `--all-features` で動かしている）。

## Out of scope

- 走査（`find -L`、WalkDir）、エージェントの root_dir の検証（validate_path）、書き込みの境界（O_NOFOLLOW）の symlink の扱い（A2）
- diff でループする symlink を REQ-cli-022 のとおり示す変更（path-validation の findings 5、後の単位）
- 既存のバックアップの記録の移し替え（A13）
- 200 行を超えるファイルの分割（`remote_path.rs` は S3 で書き直すので、書き直した結果が 200 行を超えるならスクリプトと読み取りを分けてよい）

## Steps

### S1: core に辿る手順を置きローカルの確認をそれに替える

- Purpose: "/" から要素ごとに辿る手順と結果の型を core に置き、ローカルの確認をそれに替えて、EX-paths-001 から EX-paths-018 と EX-paths-023 をローカルの経路で満たす
- Specification: `docs/ir/paths/resolution.md#REQ-paths-001`, `docs/ir/paths/resolution.md#REQ-paths-002`, `docs/ir/paths/resolution.md#REQ-paths-003`, `docs/ir/paths/resolution.md#REQ-paths-004`, `docs/ir/paths/resolution.md#REQ-paths-005`, `docs/ir/paths/resolution.md#REQ-paths-006`, `docs/ir/paths/resolution.md#REQ-paths-007`
- Prerequisites: なし
- May change: `crates/remote-merge-core/`（新しいモジュールと lib.rs の宣言と Cargo.toml の libc）、`crates/remote-merge-engine/src/local_io.rs`、`crates/remote-merge/src/runtime/target_io.rs`（新しい種類を "cannot resolve path: not a directory" のエラーに変える一か所の変換）、確認の結果の型の変更でコンパイルが通らなくなる箇所（`crates/remote-merge/src/cli/diff.rs`、`crates/remote-merge/src/runtime/core.rs`、`crates/remote-merge/src/runtime/side_io.rs`、`crates/remote-merge/src/runtime/remote_path.rs`）の型の追従だけ、`remote_path.rs` のモジュール内の比べるテストと `proptest-regressions/src/runtime/remote_path.txt` の削除、`crates/remote-merge/tests/contract/` の新しいファイル、`crates/remote-merge/tests/contract.rs` の mod 行
- Done when: EX-paths-001 から EX-paths-018 と EX-paths-023 の各状態でローカルの確認が例のとおりの結果か文言を返し、REQ-paths-001 から REQ-paths-007 のそれぞれに印の付いたテストがあり、普通の状態で新しい手順の実パスが canonicalize の結果とバイト列で一致し、古い比べるテストを除いた既存のテストがすべて通る
- Shown by: test — EX-paths-001 から EX-paths-018 と EX-paths-023 を一つずつ `@kotowari[EX-paths-0nn]`、各要件の代表に `@kotowari[REQ-paths-0nn]` を付けた契約テストとして、ローカルの経路で RED から GREEN にする（EX-paths-015 は root で実行されたときは飛ばす）。canonicalize との一致は印のない表のテストで確かめる
- Left to the implementer: 一か所の変換を置く関数の形
- Stop and hand back if: Rust の標準ライブラリと libc で、lstat の種類・readlink・faccessat(AT_EACCESS) のどれかが取れない対象が見つかった、または普通の状態で canonicalize の結果とバイト列が違う

### S2: エージェントの確認を同じ手順に替え通信の版を 5 にする

- Purpose: エージェントの確認を S1 の関数に替え、通信の型を REQ-paths-002 の四つの種類に合わせて版を 5 に上げ、版が 5 未満なら確認を SSH で行い、sudo を使うサーバではエラーにする
- Specification: `docs/ir/paths/routes.md#REQ-paths-010`, `docs/ir/paths/resolution.md#REQ-paths-002`
- Prerequisites: S1
- May change: `crates/remote-merge-protocol/src/lib.rs`、`crates/remote-merge-agent/src/agent/`（file_io.rs の確認と書き写しの削除、dispatch.rs、client.rs）、`crates/remote-merge/src/runtime/side_io.rs` の `try_agent_inspect_path`、`crates/remote-merge/src/runtime/target_io.rs` の振り分けと、版と sudo の有無から確認の経路を選ぶ純粋関数、`crates/remote-merge/tests/contract/` の新しいファイル、`crates/remote-merge/tests/contract.rs` の mod 行
- Done when: エージェントの確認が S1 の関数を呼び、版 5 では四つの種類とエラーが通信を往復し、版 4 と交渉したときは確認が SSH の経路で行われ、sudo を使うサーバでは置き直しが要ることを示すエラーになる
- Shown by: test — EX-paths-021 と EX-paths-024 を、経路を選ぶ純粋関数を呼ぶ契約テストとして `@kotowari[EX-paths-021]`・`@kotowari[EX-paths-024]`・`@kotowari[REQ-paths-010]` を付けて RED から GREEN にし、四つの種類とエラーの往復を protocol の単体テストで確かめる
- Left to the implementer: 置き直しが要ることを示すエラーの英語の文言（"agent" と "redeploy" に当たる語を含める）
- Stop and hand back if: 版を 5 に上げると既存の配置の判定（ハッシュが一致しないエージェントの置き直し）か、版 4 のエージェントとの他の要求が壊れる

### S3: SSH の確認を同じ手順の sh で書き直し三つの経路を比べる

- Purpose: SSH の確認スクリプトを S1 と同じ手順の POSIX sh に書き直し、三つの経路が同じ結果を返すことを例の表と proptest で示す
- Specification: `docs/ir/paths/routes.md#REQ-paths-008`, `docs/ir/paths/routes.md#REQ-paths-009`, `docs/ir/paths/resolution.md#REQ-paths-001`, `docs/ir/paths/resolution.md#REQ-paths-002`
- Prerequisites: S2
- May change: `crates/remote-merge/src/runtime/remote_path.rs`、`crates/remote-merge/src/runtime/remote_io.rs` の確認の呼び出し（root_dir と相対パスを分けて渡し、`LC_ALL=C` を外から渡す）、`crates/remote-merge/src/runtime/mod.rs` の `remote_path` の公開（test-utils のときだけ）、`crates/remote-merge/tests/contract/` の新しいファイルと `ssh_server.rs` の模擬の応答、`crates/remote-merge/tests/contract.rs` の mod 行、`proptest-regressions/`
- Done when: EX-paths-001 から EX-paths-018 と EX-paths-023 の各状態と proptest の生成した状態で、ローカル・SSH のスクリプト（同じマシンの sh を通し製品の読み取りを通したもの）・エージェント（Dispatcher の応答を通信の形式で往復させ製品の変換を通したもの）の三つの結果が種類と実パスのバイト列とエラーの文言で一致し、readlink のない環境で EX-paths-020 のとおりになり、読み取りが一覧にない文言・欠けた欄・0 でない終了コードをエラーにする
- Shown by: test — EX-paths-019 と `@kotowari[REQ-paths-008]` を proptest と例の表の比較に、EX-paths-020 と `@kotowari[REQ-paths-009]` を PATH から readlink だけを外した sh で動かすテストに付けて RED から GREEN にし、findings 15〜17 の形（EX-paths-008・EX-paths-009・EX-paths-013・EX-paths-018）が比較に含まれることを確かめる
- Left to the implementer: スクリプトを BusyBox ash でも動かして確かめる方法（E2E のコンテナを使うか一度だけの確かめでよい。コミットに含めない確かめは報告にコマンドと結果を書く）
- Stop and hand back if: `[ -x ]`・`[ -e ]`・`[ -L ]`・`[ -f ]` の組み合わせで、検索権限がないことと存在しないことを A35 の例外以外で見分けられない状態が見つかった、または BusyBox ash で `${#var}` がバイトで数えられない

### S4: 確認の結果を使う側を新しい結果に替える

- Purpose: rollback を新しい結果で判定するように替えて別の問い合わせを消し、rollback 以外は途中の祖先がディレクトリでないを確認のエラーとして扱う
- Specification: `docs/ir/paths/routes.md#REQ-paths-011`, `docs/ir/backup/rollback-path.md#REQ-backup-031`, `docs/ir/backup/rollback-path.md#REQ-backup-003`, `docs/ir/backup/rollback-path.md#REQ-backup-033`
- Prerequisites: S3
- May change: `crates/remote-merge-engine/src/service/rollback.rs`、`crates/remote-merge/src/service/merge_flow.rs` の確認の結果を読む箇所、`crates/remote-merge-engine/src/local_io.rs` の `is_regular_file` の削除、`crates/remote-merge/src/runtime/side_io.rs`、`crates/remote-merge/src/runtime/target_io.rs`、`crates/remote-merge/src/runtime/remote_path.rs` の `[ -f ]` のコマンドの削除、`crates/remote-merge/src/runtime/core.rs`、`crates/remote-merge/src/cli/diff.rs`、`crates/remote-merge/tests/contract/` の新しいファイルと既存の rollback の三つの経路のテストの出力の形への追従、`crates/remote-merge/tests/contract.rs` の mod 行
- Done when: EX-backup-027・EX-backup-029・EX-backup-030 が三つの経路で今と同じスキップ理由になり、EX-paths-022 で merge が "cannot resolve path: not a directory" のエラーを返し、rollback の確認のエラーに接頭辞が二重に付かず、SymlinkUpdate の記録で途中の祖先がディレクトリでない（通常ファイル以外）ときとエラーのときに REQ-backup-003 のエラーになり、diff の root の外のリンクの判定が確認のエラーを握りつぶさず、`is_regular_file` と `restore_path_after_failed_inspection` と祖先を問い合わせる処理が残っていない
- Shown by: test — EX-paths-022 と `@kotowari[REQ-paths-011]` を merge の契約テストに付けて RED から GREEN にし、EX-backup-027・EX-backup-029・EX-backup-030 の既存の三つの経路のテストが通り続けることを確かめる
- Left to the implementer: rollback の判定を純粋関数に残す形
- Stop and hand back if: 既存のバックアップの記録の実パスと新しい確認の実パスが普通のパスで違い比較が変わる、または diff の該当箇所にパスごとのエラーを渡す既存の仕組みがない

### S5: 変更の記録を作り直し全体を確かめる

- Purpose: ブランチ全体の実装者の変更の記録を作り直し、全体の検査と Docker の E2E を通す
- Specification: `docs/ir/paths/routes.md#REQ-paths-008`
- Prerequisites: S4
- May change: `.kotowari/changes/implementation.yaml`、`PROJECT.md` の core のモジュールと local_io のパス検査の記述
- Done when: 実装者の記録が比較元 213a8def195d5026eef2f07e6930eafd6a104693 と最終 HEAD のブランチ全体を覆い、全テストと E2E が通り、この計画が扱う要件と例のどれにも印の付いたテストがある
- Shown by: check — `systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`、`scripts/run-container-e2e.sh`（mise の cargo に musl のターゲットがないときは `~/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin` を PATH の先頭に置いて実行）、`kotowari query` で REQ-paths-001 から REQ-paths-011 と EX-paths-001 から EX-paths-024 の tests が空でないこと、`kotowari check --format json` でこのブランチが変えたファイルとこの計画の ID にエラーがないこと、`kotowari changes --base 213a8def195d5026eef2f07e6930eafd6a104693 --head <最終 HEAD> --phase implementation --format json` が終了コード 0
- Left to the implementer: 記録の組の分け方と ID
- Stop and hand back if: E2E の実際の OpenSSH のサーバ（BusyBox の sh）で、スクリプトの結果がこのマシンの sh で動かしたときと違う
