# 書き込み直前の更新確認

差分表示後に別の操作が書き込み先を変えたときのマージ結果と、そのときの書き込み・symlink の作成・削除が書き込み先の外や種類の違う対象を変えずに失敗する規則。

## Requirements

### REQ-merge-011: 他者の更新を上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A35, docs/decision/records/2026-09-25-spec-migration.md#A38
- verification: unit

差分確認からマージまでに書き込み先の中身が変わったとき、サイズと更新時刻が元と同じでも、古い状態を前提としたマージは書き込みを止め、変更を報告する。

### REQ-merge-033: 更新を検知したファイルだけを止める
- kind: event_driven
- source: docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A9, docs/decision/records/2026-10-11-symlink-merge-check.md#A1, docs/decision/records/2026-10-11-symlink-merge-check-consistency.md#A1, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25
- verification: unit

merge と sync は、比べたときから書き込み先の中身が変わったファイルと、比べたときになかった書き込み先が現れたファイルを書かずに failed に出し、他のファイルの処理は続ける。
比べたときに symlink だった書き込み先のリンク先が UTF-8 として読めないバイトか置換文字 U+FFFD を含むときは、変わっていないことを確かめられないので、変わったファイルと同じく書かずに failed に出す。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-merge-038: 最後の要素が symlink のパスへ通常ファイルを書かない
- kind: prohibition
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A1, docs/decision/records/2026-10-05-ssh-strict-exec.md#A3, docs/decision/records/2026-10-05-ssh-strict-exec.md#A11, docs/decision/records/2026-10-05-ssh-strict-exec.md#A14, docs/decision/records/2026-10-05-harden-security.md#A6, docs/decision/records/2026-10-12-foundation-3.md#D12
- verification: review
- how_to_verify: ローカルは "crates/remote-merge-core/src/machine_ops/tests.rs" の a_write_onto_a_symlink_or_a_directory_fails_and_changes_nothing、SSH は "crates/remote-merge-ssh/src/tree_parser.rs" の binary_write_refuses_a_symlink_destination・text_write_refuses_a_symlink_destination・a_write_onto_a_dangling_link_fails_without_creating_its_target・without_dd_nofollow_a_link_destination_is_refused_and_a_file_is_written・a_dd_without_status_none_still_opens_the_destination_without_following_a_link（生成したコマンドを各ログインシェルから sh で実行する）、エージェントはローカルと同じ "crates/remote-merge-core/src/machine_ops/tests.rs" の関数で書く（"crates/remote-merge-agent/src/agent/dispatch/writes.rs"）ことを読み、最後の要素が symlink の書き込み先への書き込みが失敗しリンクとリンク先が変わらないことを人が確かめる。これらの単体テストは kotowari が印を数えるテストファイルの外にある。比べた後に書き込み先が symlink に替わる競合でだけ起きるため、契約テストでは作らない。

通常ファイルの中身の書き込みは、書き込み先のパスの最後の要素が既存の symlink ならそのパスへ書かずに失敗し、リンクもリンク先も変えない。
書き込み先の途中の symlink は REQ-merge-004 のとおり辿る。
リモートの dd が "oflag=nofollow" を受け付けないときに残る競合は FLAG-merge-021 に残す。

### REQ-merge-039: 書き込みは書き込み先をその場で書き換える
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A11, docs/decision/records/2026-10-05-ssh-strict-exec.md#A14, docs/decision/records/2026-10-11-agent-operations.md#A6, docs/decision/records/2026-10-12-foundation-3.md#D18
- verification: unit
- how_to_verify: "crates/remote-merge-ssh/src/tree_parser.rs" の a_write_keeps_owner_mode_and_hard_links_and_truncates_old_content・writes_create_a_missing_file_and_leave_no_other_entry・a_link_swapped_in_during_a_write_keeps_its_target_unchanged・a_write_onto_a_directory_fails_without_change・without_dd_nofollow_a_link_destination_is_refused_and_a_file_is_written・a_decoder_that_fails_after_writing_part_fails_the_write・a_binary_write_without_openssl_fails_and_keeps_the_destination を読み、生成した書き込みのコマンドを各ログインシェルから sh で実行して、書き込み先と同じディレクトリに他の項目が現れないこと、既存の書き込み先の権限とハードリンクが残り古い中身が切り詰められること、書き込みの途中で書き込み先を symlink に替えてもリンク先が変わらないこと、中身を途中まで書いた後でデコードが失敗すると書き込みが失敗すること、openssl のない PATH ではバイナリの書き込みが失敗し書き込み先が変わらないことを人が確かめる。これらの単体テストは kotowari が印を数えるテストファイルの外にある。 三つの経路の書き込みがその場で書き、inode・所有者・ハードリンクを保つことは、EX-merge-062 の契約テスト（crates/remote-merge/tests/contract/merge_hard_links.rs）で確かめる。

通常ファイルの書き込みは、一時ファイルを作らず、書き込み先のファイルをその場で開いて古い中身を切り詰めてから書く。
書き込み先のファイルそのものに書くので、書き込み先の inode は変わらず、REQ-merge-013 と REQ-backup-029 のとおり所有者と権限を変えず、ハードリンクも保つ。
書き込みを始めた後で書き込み先が symlink に替わっても、リンク先は変えない。
書き込み先が symlink でない既存のディレクトリなら書かずに失敗する。
書き込む中身を作る処理（リモートの道具で base64 を復号するものを含む）が失敗したときは、書き込み先に書けていても書き込みを失敗にする。
その処理に要る道具がリモートにないときは、書き込み先を開かずに書き込みを失敗にし、書き込み先を変えない（ssh/compatibility.md の TBL-ssh-002）。
書き込みの途中で失敗したときは、途中までの中身が書き込み先に残る。

### REQ-merge-040: symlink の作成先にある項目を置き換えない
- kind: prohibition
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A7, docs/decision/records/2026-10-05-ssh-strict-exec.md#A10, docs/decision/records/2026-10-05-ssh-strict-exec.md#A14, docs/decision/records/2026-10-12-foundation-3.md#D12
- verification: review
- how_to_verify: ローカルは "crates/remote-merge-core/src/machine_ops/tests.rs" の creating_a_symlink_where_something_exists_fails_without_change、SSH は "crates/remote-merge-ssh/src/tree_parser.rs" の creating_a_link_where_a_link_or_a_file_exists_fails_without_change と creating_a_link_where_a_directory_or_a_file_parent_is_fails_without_change（生成したコマンドを sh で実行する）、エージェントはローカルと同じ machine_ops の関数で作る（"crates/remote-merge-agent/src/agent/dispatch/writes.rs"）ことを読み、作成先に通常ファイル・ディレクトリ・symlink があると作成が失敗しそれらが変わらないことを人が確かめる。これらの単体テストは kotowari が印を数えるテストファイルの外にある。比べた後に作成先に項目が現れる競合でだけ起きるため、契約テストでは作らない。

symlink の作成は、作成先に通常ファイル・ディレクトリ・symlink のどれかが既にあれば置き換えずに失敗し、何も変えない。
merge・sync・rollback の symlink の更新は、比べたときにあった書き込み先を消してから作るので、REQ-merge-003 のとおりリンク先を更新する。

### REQ-merge-041: 比べた後に消えた削除の対象は失敗にする
- kind: event_driven
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A4, docs/decision/records/2026-10-05-ssh-strict-exec.md#A9, docs/decision/records/2026-10-05-ssh-strict-exec.md#A14, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-12-foundation-3.md#D12
- verification: review
- how_to_verify: SSH は "crates/remote-merge-ssh/src/tree_parser.rs" の removing_deletes_a_file_or_a_dangling_link_and_fails_on_a_missing_path（生成した削除のコマンドを sh で実行する）、ローカルは "crates/remote-merge-core/src/machine_ops/tests.rs" の removing_a_link_keeps_its_target_and_a_directory_or_missing_path_fails を読み、ないパスの削除が失敗することを人が確かめる。三つの経路でないパスの削除がないの失敗になることは、EX-paths-044 の契約テスト（crates/remote-merge/tests/contract/operation_failures_agree.rs）でも確かめる。これらの単体テストは kotowari が印を数えるテストファイルの外にある。比べた後に書き込み先が消える競合でだけ起きるため、契約テストでは作らない。

--delete の merge と sync は、消そうとした書き込み先が比べた後に消えていたとき、そのファイルをないの失敗（paths/operations.md の REQ-paths-020）として failed に出し、他のファイルの処理は続ける。
リモートの削除を使う symlink の更新でも、消えていた書き込み先は failed に出す。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

## Examples

```gherkin
@id=EX-merge-021 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 差分確認後に書き込み先が変わる
Given 差分を確認した後で別の操作が書き込み先を更新した
When 元の差分を使ってマージする
Then 後から行われた更新は上書きされず変更が報告される

@id=EX-merge-022 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A35
Scenario: 書き込み先は変わらない
Given 差分確認後も書き込み先は同じである
When 元の差分を使ってマージする
Then 書き込みが行われる

@id=EX-merge-023 @about=REQ-merge-011 @source=docs/decision/records/2026-09-25-spec-migration.md#A38
Scenario: 更新時刻を戻した外部変更
Given 差分確認後に書き込み先の中身が変わりサイズと更新時刻は以前と同じである
When 元の差分を使ってマージする
Then 書き込み先は上書きされず内容の変更が報告される

@id=EX-merge-045 @about=REQ-merge-038 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A1
Scenario: 比べた後に書き込み先が symlink に替わる
Given 比べたときに通常ファイルだった書き込み先が、書き込みの前に別のファイルを指す symlink に替わった
When そのパスをマージする
Then そのファイルは失敗し、リンクとリンク先のファイルは変わらない

@id=EX-merge-046 @about=REQ-merge-039 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A11
Scenario: SSH の書き込みの途中で書き込み先が symlink に替わる
Given SSH の経路で書き込み先への中身の送信の途中に、書き込み先が別のファイルを指す symlink に替わった
When 送信が終わり書き込みが完了する
Then リンク先のファイルは変わらない

@id=EX-merge-050 @about=REQ-merge-039 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A11
Scenario: SSH の書き込みが既存のファイルの権限とハードリンクを保つ
Given エージェントを使わないリモートの書き込み先が、既定と違う権限を持ち、別の名前のハードリンクもある既存のファイルである
When 元より短い中身でそのファイルをマージする
Then 書き込み先の権限は変わらず、ハードリンクの別の名前からも新しい中身だけが読め、書き込み先のディレクトリに他の項目は現れない

@id=EX-merge-047 @about=REQ-merge-040 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A7
Scenario: 比べた後に symlink の作成先にファイルが現れる
Given 比べたときになかった symlink の作成先に、作成の前に通常ファイルが現れた
When そのパスをマージする
Then そのファイルは失敗し、現れた通常ファイルは変わらない

@id=EX-merge-048 @about=REQ-merge-041 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A4,docs/decision/records/2026-10-05-ssh-strict-exec.md#A9
Scenario: 比べた後に削除の対象が消える
Given --delete で消す書き込み先だけのファイルが、削除の前に別の操作で消された
When --delete でマージする
Then そのファイルは failed に出て、他のファイルの処理は続く

@id=EX-merge-053 @about=REQ-merge-033 @source=docs/decision/records/2026-10-11-symlink-merge-check.md#A1
Scenario: 比べた後に書き込み先の symlink が通常ファイルに替わる
Given 読み込み元も書き込み先も symlink で、比べた後、merge の前に書き込み先の symlink が通常ファイルに替わった
When バックアップを無効にしてそのパスを merge する
Then そのファイルは failed に出て、現れた通常ファイルの中身は変わらない

@id=EX-merge-054 @about=REQ-merge-033 @source=docs/decision/records/2026-10-11-symlink-merge-check.md#A1
Scenario: 比べた後に書き込み先の symlink のリンク先が変わる
Given 読み込み元も書き込み先も symlink で、比べた後、merge の前に書き込み先の symlink が別のリンク先を指すように作り直された
When そのパスを merge する
Then そのファイルは failed に出て、書き込み先の symlink は作り直されたリンク先を指したままである

@id=EX-merge-055 @about=REQ-merge-033 @source=docs/decision/records/2026-10-11-symlink-merge-check-consistency.md#A1
Scenario: 比べた後に書き込み先の symlink が UTF-8 でないバイトだけが違うリンク先に作り直される
Given 読み込み元も書き込み先も symlink で、書き込み先のリンク先は UTF-8 として読めないバイトを含み、比べた後、merge の前にそのバイトだけが違うリンク先を指すように作り直された
When バックアップを無効にしてそのパスを merge する
Then そのファイルは failed に出て、書き込み先の symlink は作り直されたリンク先を指したままである

@id=EX-merge-062 @about=REQ-merge-039 @source=docs/decision/records/2026-10-12-foundation-3.md#D18
Scenario: 三つの経路で既存のファイルに書く
Given ローカル・SSH・エージェントの書き込み先に、別の名前のハードリンクもある既存の "a.txt" がある
When それぞれの経路で "a.txt" に新しい中身を merge する
Then どの経路でも "a.txt" の inode と所有者は書く前と同じで、ハードリンクの別の名前からも新しい中身が読める
```
