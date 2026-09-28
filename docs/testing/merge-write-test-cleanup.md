# merge の書き込みの中身のテスト整理の記録

merge の書き込みの中身の取り込みで加えた要件（REQ-merge-019 から REQ-merge-022）の根拠テストを整えた過程の記録。
変異テストの結果、各要件の根拠にしたテスト、整理後の見逃しの決着を残す。
取り込みの仕分けでこの範囲の既存テストは全て既存要件の根拠だったため、この整理ではテストを消していない。

## 整理前の変異テスト

整理を始める前のコミット bb37889（作業ツリーに変更のない状態）で、次のコマンドを一度だけ実行した。
三つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs
```

全体の集計は `mutants: caught=133 survived=13 timeout=0 unviable=17 equivalent=0`（163 件、実行時間は約 37 分）。
スクリプトの終了コードは 1 だった。これは kotowari mutants が見逃しを error として報告したためで、メモリ上限による停止ではない（cargo-mutants は完了し、結果が読まれている）。
ファイルごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge_flow.rs | 23 | 10 | 0 | 8 |
| src/cli/tolerant_io.rs | 8 | 0 | 0 | 0 |
| src/service/status.rs | 102 | 3 | 0 | 9 |

### 見逃し

位置は変異が入る行と列、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。src/service/merge_flow.rs の `execute_single_merge` の通常ファイルの経路と `copy_permissions`、src/cli/tolerant_io.rs の `fetch_contents_required`、src/service/status.rs の `verified_content_pairs`・`needs_explicit_file_compare`・`needs_merge_content_compare`・`refine_status_with_content` の見逃しを対象にする。
src/service/status.rs の他の関数、`fetch_contents_tolerant`、symlink の分岐・`check_source_exists`・`execute_deletions`・`validate_hunk_merge_target`・`execute_hunk_merge` の見逃しは記録だけする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 対象（読み込み元の権限が 0 のときに権限を付けるか） |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 対象（同上） |
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 記録だけ（変更のまとまりを選ぶマージの回で扱う） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 記録だけ（同上） |
| src/service/status.rs:143:37 | replace + with * in collect_all_file_paths | 記録だけ（status の規則。容量の見積もりだけに使う式） |
| src/service/status.rs:320:15 | replace && with \|\| in needs_merge_content_compare | 対象（--checksum のときに中身を読み比べる組の条件） |
| src/service/status.rs:323:61 | replace && with \|\| in needs_merge_content_compare | 対象（同上） |

`copy_permissions` の二件は、読み込み元がリモートの場合（FLAG-merge-002）、新規ファイルの権限（FLAG-merge-003）、権限の変更の失敗（FLAG-merge-004）のどれにも当たらない（読み込み元の権限の値が 0 のときだけ違いが出る）。
`fetch_contents_required` と `verified_content_pairs`・`needs_explicit_file_compare`・`refine_status_with_content` には見逃しがなかった。

### 変異と関係のないテストだけによる検知

検知した 133 件の変異ごとに、cargo-mutants の変異ごとのログから失敗したテストを集めた。
nextest は最初の失敗から少し進んで止まるため、集めた名前は検知したテストの全てではなく、先に失敗したものである。

負荷の下で落ちることのあるテスト（tui_merge や agent_ssh のテスト）だけで検知された変異は次の一件だった。

| 位置 | 変異 | 失敗したテスト | 扱い |
|---|---|---|---|
| src/service/status.rs:137:5 | replace path_is_within_unloaded_dir -> bool with true | tui_merge の test_hunk_merge_left_to_right_with_l、test_hunk_merge_right_to_left_with_h_key、test_merge_cancel_with_n、test_sensitive_file_merge_requires_confirmation | status の規則で決着の対象ではない。整理後の比較では、この変異を見逃しの候補に含めて比べる |

この一件は決着の対象の関数ではないため、見かけの検知かどうかは確かめていない。

## 要件ごとの根拠テスト

根拠テストは `tests/contract/merge_write.rs` にある。手本にした元のテスト（`tests/contract/merge_paths.rs`、`tests/cli_merge.rs`）は消さず、書き換えていない。
新しく書いたテストは、書いた時点の実装に対して通ることを最初の実行で確かめた。
一つの要件に複数の場合があるときは、場合ごとのテストに同じ要件の印を付け、印の付いたテストを合わせて要件の文を全て確かめる。

準備は `tests/contract/merge_support.rs` の `Fixture` を使い、書き込み先 develop を `RuntimeTargets::with_local` で一時ディレクトリに差し替えて `execute_merge` と `execute_sync` を関数呼び出しで呼ぶ。
結果は --format json と同じ `format_json` で JSON にして確かめる。バックアップは無効にした構成（`fixture`）を使う。
sync の呼び出しに足した `Fixture::sync_json` と `sync_args` は --force を付ける（--force のない sync は書き込む予定があると確認のプロンプトで標準入力を読むため）。
配下のディレクトリの作成（`Fixture::write` は親ディレクトリを作らない）、更新時刻の読み書き、読めないファイルの作成の補助も `Fixture` に足した。
読めないファイルは権限を 0o200 に落として作り、開けないことを先に確かめる（root で実行されると読めてしまい、前提が崩れたことが分かるようにする）。

### 読めなかった側と原因の示し方（REQ-merge-019）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-019（merge） | merge_reports_which_side_could_not_be_read | tests/contract/merge_paths.rs の explicit_merge_checks_readability_even_when_file_sizes_differ・unreadable_source_fails_one_file_without_blocking_the_other_merge・two_unreadable_sides_are_not_reported_as_identical_empty_files。元のテストは error が "read failed:" で始まることだけを見る。一回の merge で三つのファイルを明示し、読み込み元だけが読めない source-locked.txt の error が "read failed: left: " で始まり "right: " と "; " を含まないこと、書き込み先だけが読めない destination-locked.txt の error が "read failed: right: " で始まり "left: " と "; " を含まないこと、両側が読めない both-locked.txt の error が "read failed: left: " で始まりその後に "; right: " が続くことを確かめる。原因の文言は確かめない |
| REQ-merge-019（sync） | sync_reports_which_side_could_not_be_read | 同じ三件と tests/contract/merge_paths.rs の sync_does_not_overwrite_a_destination_it_cannot_read。同じ構成を --force の sync で確かめる |

- どのファイルも左右で中身の長さを変えた（ファイルを明示したときは長さによらず中身を読み比べる）。
- 書き込み先を読めないが中身を読み比べない場合の error（FLAG-merge-005）は構成に含めない。ファイルを明示した読み比べだけを使う。

### ディレクトリ指定の既定の比べ方（REQ-merge-020）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-020（merge） | directory_merge_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed | tests/contract/merge_paths.rs の directory_sync_without_checksum_uses_the_metadata_quick_check（sync で、サイズと更新時刻が同じで中身が違うファイルが書かれないことだけを見る）。--checksum のない folder の merge で、merged がちょうど folder/control.txt の一件で、書かれない二つのファイルの中身と更新時刻が変わらないことを確かめる |
| REQ-merge-020（sync） | directory_sync_without_checksum_writes_only_files_the_metadata_and_contents_show_as_changed | 同じ元のテスト。同じ構成を --force の sync で確かめる |

- 構成: folder の下に、サイズと更新時刻が同じで中身が違う same-metadata.txt（書き込み先の更新時刻を `File::set_modified` で読み込み元に揃える）、サイズと中身が同じで書き込み先の更新時刻を 120 秒前にずらした same-bytes.txt、左右で中身の長さが違う対照の control.txt を置く。呼ぶ前に、秒単位の更新時刻が same-metadata.txt では同じで same-bytes.txt では 60 秒以上違うことを確かめる。
- merge の結果が per-file の出力（`MergeCommandOutput::Files`）になることは `Fixture::merge_json` が確かめる（それ以外の出力では落ちる）。書き込むものがないときの出力は確かめない。
- 一時的な書き換え（コミットに含めない）で、`needs_merge_content_compare` が常に --checksum のときの組を読み比べるようにすると、この二件が落ちることを確かめた。REQ-merge-019 の二件は、読めなかった側をつなぐ "; " を別の文字に変える書き換えで落ちることを確かめた。書き換えは元に戻した。

### ディレクトリ指定と複数パスの書き込み（REQ-merge-021、REQ-merge-022）

| 要件 | 根拠テスト | 元にしたテスト |
|---|---|---|
| REQ-merge-021 | directory_merge_overwrites_changed_files_creates_source_only_files_and_skips_equal_files | tests/cli_merge.rs の test_merge_directory（配下の二つのファイルが上書きされることだけを見る）。folder の merge で、左右で中身の長さが違う changed.txt が上書きされ、読み込み元にだけある new.txt が書き込み先に作られ、中身が同じ same.txt が merged に出ず中身と更新時刻が変わらないこと、merged がちょうどこの二件で、failed が空で終了コードが 0 であることを確かめる |
| REQ-merge-022（二つのパス） | a_merge_of_two_paths_writes_each_of_them | tests/cli_merge.rs の test_merge_multiple_files。a.txt と b.txt を指定した merge で、merged がちょうどこの二件で、両方の中身が読み込み元に揃い、終了コードが 0 であることを確かめる |
| REQ-merge-022（同じパスの重ね指定） | a_path_given_twice_is_written_once | tests/cli_merge.rs の test_merge_duplicate_paths_deduplicated（標準出力の "Merged:" の数を見る）。a.txt を二度指定した merge で、merged にちょうど一度だけ出て、failed が空で、終了コードが 0 であることを確かめる。failed の error の文言は確かめない |

- REQ-merge-021 の構成には書き込み先にだけあるファイルを含めない（その扱いは symlink と削除の回の範囲）。

## 整理後の変異テスト

根拠テストを足し終えたコミット 48093a8 で、整理前と同じコマンドを一回実行した。実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh src/service/merge_flow.rs src/cli/tolerant_io.rs src/service/status.rs
```

全体の集計は `mutants: caught=134 survived=12 timeout=0 unviable=17 equivalent=0`（163 件、実行時間は約 40 分）。
スクリプトの終了コードは整理前と同じく 1（見逃しの error による。メモリ上限での停止ではない）。

| ファイル | caught | survived | timeout | unviable |
|---|---|---|---|---|
| src/service/merge_flow.rs | 24 | 9 | 0 | 8 |
| src/cli/tolerant_io.rs | 8 | 0 | 0 | 0 |
| src/service/status.rs | 102 | 3 | 0 | 9 |

### 整理前との比較

整理後の見逃し 12 件は全て整理前の見逃しに含まれる（整理前の 13 件から src/service/merge_flow.rs:413:33 を除いたもの）。
この計画はテストを消していないため、見逃しが増えないことは予想どおりだった。足したテストは決着の対象の見逃しのどれにも触れる構成を持たないため、見逃しは減っていない。

src/service/merge_flow.rs:413:33（replace || with && in execute_hunk_merge）は、整理前は見逃し、整理後は tui_merge の test_sensitive_file_merge_requires_confirmation だけに検知された。
この変異は変更のまとまりを選ぶマージの関数にあり、機密ファイルの確認のテストが通る経路ではないため、負荷の下での見かけの検知とみなし、見逃しとして扱う（決着の対象ではない）。
src/service/status.rs:137:5（replace path_is_within_unloaded_dir -> bool with true）は整理前と同じく tui_merge のテストだけに検知された。この変異は `.kotowari/mutants-equivalents.yaml` に status の整理で同等変異として登録済みで、見かけの検知である。
これ以外に、負荷の下で落ちるテストだけで検知された変異はなかった。

### 見逃しの決着

| 位置 | 変異 | 決着 |
|---|---|---|
| src/service/merge_flow.rs:256:14 | replace > with >= in copy_permissions | 未決着。下の「利用者の判断を待つ候補」の二件目 |
| src/service/merge_flow.rs:256:18 | replace && with \|\| in copy_permissions | 同上 |
| src/service/status.rs:320:15 | replace && with \|\| in needs_merge_content_compare | 未決着。下の「利用者の判断を待つ候補」の一件目 |
| src/service/status.rs:323:61 | replace && with \|\| in needs_merge_content_compare | 同上 |

決着の対象でない見逃しは次の 9 件（整理後の見逃し 8 件と、見かけの検知として見逃しに数える :413:33）で、記録だけする。

| 位置 | 変異 | 扱う回 |
|---|---|---|
| src/service/merge_flow.rs:413:33 | replace \|\| with && in execute_hunk_merge | 変更のまとまりを選ぶマージの回（整理後は見かけの検知） |
| src/service/merge_flow.rs:427:13 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:428:13 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:479:21 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:480:21 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:481:21 | delete field hunk_info from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:502:17 | delete field path from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/merge_flow.rs:503:17 | delete field status from struct MergeFileResult expression in execute_hunk_merge | 同上 |
| src/service/status.rs:143:37 | replace + with * in collect_all_file_paths | status の規則（容量の見積もりだけに使う式） |

### 利用者の判断を待つ候補

一件目: src/service/status.rs:320:15 と :323:61（--checksum のときに中身を読み比べる組の条件）

- 観測: どちらの変異も、--checksum のディレクトリ指定で、左右の片方だけが通常ファイルの組（または両方が通常ファイルでない組）を中身の読み比べに加える。コミットに含めない一時的なテストで、local の folder/x が通常ファイル、develop の folder/x がディレクトリの構成を --checksum 付きで merge と sync にかけると、今の実装は folder/x を skipped（reason "source and destination have different file types"）に出して終了コード 0 になる。変異では folder/x が failed（error "read failed: right: …Is a directory…"）になり終了コード 2 になる。どちらも書き込み先は変えない。
- 計画との関係: 計画は `needs_merge_content_compare` の見逃しを決着の対象にしているが、変異を落とす構成は通常ファイルとディレクトリ（または symlink）の種類の違いで、その扱いは REQ-merge-001（docs/ir/merge/symlink.md、種類の違う対象を理由付きでスキップする）の範囲にある。計画は symlink と種類の違いのテストを merge の後の回に回しており、S4 で足してよい既存要件の例にも REQ-merge-001 を挙げていない。どちらに読むかを計画が決めていないため、テストを足さずに返す。同等変異ではない（観測できる違いがある）。不具合の疑いではない（今の実装は REQ-merge-001 どおりスキップする）。
- 判断してほしいこと: (a) この回でテストを足す（`tests/contract/merge_write.rs` に、--checksum のディレクトリ指定の merge と sync で通常ファイルとディレクトリの組が failed に出ず、skipped に出て書き込み先が変わらないことを確かめるテストを、REQ-merge-001 と REQ-merge-006 の印で置く。reason の文言は確かめない）、(b) symlink と種類の違いの回で扱うものとして記録だけにする、のどちらにするか。推奨は (a)。symlink を使わない通常ファイルとディレクトリの組で落とせ、期待する挙動は既存の REQ-merge-001 が決めており、新しい判断を要しないため。

二件目: src/service/merge_flow.rs:256:14 と :256:18（読み込み元の権限の値が 0 のときに書き込み先の権限を変えるか）

- 観測: どちらの変異も、--with-permissions で読み込み元（ローカル）の権限の値が 0（mode 000）のときだけ、元のコードがしない `chmod(書き込み先, 0)` を行う。`copy_permissions` はどの経路でも読み込み元の中身を読んだ後に呼ばれるため、最初から mode 000 の読み込み元は読み取りで失敗し、ここまで届かない（root でない場合）。
- 同等変異として登録できるかの試み: 別の文脈のエージェントに、merge と sync の公開された入口からこの二つの変異を落とすテストを書かせた。エージェントは、書き込み先を FIFO（名前付きパイプ）にして merge の書き込み先の読み取りと書き込みを止め、読み込み元を読んだ後・権限を読む前に読み込み元を mode 000 にする競合を作るテスト（mkfifo と inotify を使う Linux 専用のテスト。merge が書き込み先を読む回数（2 回）に依存する）で、二つとも落とした。元のコードでは書き込み先の権限が 0o640 のまま残り、変異では 0 になる。この結果は、コミットに含めない一時的な書き換えでそれぞれの変異を入れて実行し、元のコードで通り、二つの変異で落ちることを確かめた。観測できる違いがあるため、同等変異としては登録しない。
- IR との関係: REQ-merge-014 は --with-permissions のとき読み込み元のファイル権限を書き込み先に反映するとする。読み込み元の権限が 0 のときに反映しない今の実装は、その文を字のとおり読むと食い違い、変異の挙動（0 を反映する）のほうが文に近い。一方で、この違いは読み込みと権限の読み取りの間に読み込み元の権限が変わる競合でしか起きない。FLAG-merge-002 から 004（リモートの読み込み元、新規ファイル、権限の変更の失敗）のどれにも当たらない。
- テストを足さなかった理由: エージェントのテストは、IR が決めていない挙動（権限 0 を反映しない）を根拠テストで決めてしまい、読み取りの回数という実装の詳細と FIFO の競合に依存して壊れやすいため、利用者の判断なしには足さない。
- 判断してほしいこと: (a) 新しい FLAG（読み込み元の権限が 0 のときの複製。kind は gap）として記録し、この二件をその範囲として決着させる、(b) 今の挙動（0 は反映しない）が意図どおりとして、エージェントのテストを REQ-merge-014 の印で足す、(c) 起きるのが競合の間だけで実際上は届かないとして、エージェントのテストで落とせたことを why に書いて同等変異に登録する（計画の登録の条件「別の文脈で書けなかった」は満たさない）、のどれにするか。推奨は (a)。IR の文と今の実装が食い違って見え、どちらに合わせるテストもその未決の点を決めてしまうため。

## 要件の verification の見直し

REQ-merge-019 から 022 の verification は全て unit で、いずれも具体的な場面の入力で結果が決まる挙動のため、要件の性質に合う（REQ-testing-009 の選び方）。見直しの候補はない。
property の要件はないため、REQ-testing-010（proptest で検査範囲に置く）に当たるテストはない。

| 要件 | verification | 合う理由 |
|---|---|---|
| REQ-merge-019 | unit | 読めない側（左・右・両方）の場面ごとに error の形が決まる |
| REQ-merge-020 | unit | サイズ・更新時刻・中身の組み合わせの場面ごとに書くか書かないかが決まる |
| REQ-merge-021 | unit | 配下のファイルの状態（中身が違う・読み込み元だけ・同じ）ごとに書くか書かないかが決まる |
| REQ-merge-022 | unit | 指定したパスの並び（別々・重複）に対して書き込みの回数が決まる |
