# 書き込み直前の更新確認

差分表示後に別の操作が書き込み先を変えたときのマージ結果と、そのときの書き込み・symlink の作成・削除が書き込み先の外や種類の違う対象を変えずに失敗する規則。

## Requirements

### REQ-merge-011: 他者の更新を上書きしない
- kind: prohibition
- source: docs/decision/records/2026-09-25-spec-migration.md#A35, docs/decision/records/2026-09-25-spec-migration.md#A38, docs/decision/records/2026-10-12-foundation-4.md#E9, docs/decision/records/2026-10-12-foundation-3.md#D46
- verification: unit

差分確認からマージまでに書き込み先の中身が変わったとき、サイズと更新時刻が元と同じでも、古い状態を前提としたマージは、compare/reading.md の REQ-compare-015 の変更の直前の判定で書き込みを止め、変更を報告する。--hunks の merge と --delete の削除も同じである。
変わったかは書き込みの直前に一度だけ確かめ、確かめてから書くまでの短い間に変わったものは追わない（ふつうの並行する作業でも、この間に書き込み先を書き換えれば上書きしうる）。

### REQ-merge-033: 更新を検知したファイルだけを止める
- kind: event_driven
- source: docs/decision/records/2026-10-04-merge-backup-flag-resolution.md#A9, docs/decision/records/2026-10-11-symlink-merge-check.md#A1, docs/decision/records/2026-10-11-symlink-merge-check-consistency.md#A1, docs/decision/records/2026-10-11-agent-operations.md#A38, docs/decision/records/2026-10-11-agent-operations.md#A20, docs/decision/records/2026-10-11-agent-operations.md#A25, docs/decision/records/2026-10-12-foundation-4.md#E9
- verification: unit

merge と sync は、REQ-compare-015 のとおり、比べたときから書き込み先の種類・リンク文字列・中身が変わったファイルと、比べたときになかった書き込み先が現れたファイルを書かずに（消さずに）failed に出し、他のファイルの処理は続ける。
比べたときに symlink だった書き込み先のリンク先が UTF-8 として読めないバイトか置換文字 U+FFFD を含むときは、変わっていないことを確かめられないので、変わったファイルと同じく書かずに failed に出す。
`接続断` のときは、そのファイルを含めて REQ-ssh-022・REQ-ssh-003・REQ-ssh-023 による。

### REQ-merge-044: 処理の途中で root_dir ごと消えたら、その側への処理を止める
- kind: event_driven
- source: docs/decision/records/2026-10-12-foundation-3.md#D47, docs/decision/records/2026-10-12-foundation-3.md#D48, docs/decision/records/2026-10-12-foundation-3.md#D49
- verification: unit

どのコマンドでも、どの側（読み込み元・書き込み先）でも、どの経路でも、処理の途中でその側の root_dir がなくなった（ディレクトリでなくなったもの、祖先がディレクトリでなくなったものを含む）と分かったときは、その側への処理を止め、終了コード 2 を返す。
merge・sync・rollback では、そのファイルを root_dir がないことと root_dir（リモートは "サーバ名:パス"）が分かる error で failed に出し、その側を使うまだ処理していない残りのファイル（読み込み元の側が止まったときは、それを読む書き込み先のファイルも）を変更せずに、手を付けていないことが分かる error で failed に出す。それまでに済んだファイルはそのまま結果に出し、root_dir もその親も作り直さない。sync の他の書き込み先は REQ-merge-015 のとおり処理を続ける。
読むだけのコマンド（diff・status）では、走査の root_dir の誤り（REQ-scan-017）と同じく、root_dir がないことと root_dir が分かる誤りで止める。
root_dir がなくなったと分かるのは、その側への操作が、ない・ディレクトリでないと答えた後に root_dir を確かめ直したとき、root_dir 直下のパスの親（root_dir そのもの）がないか途中がディレクトリでないと答えた後に確かめ直したとき、親を作る前に root_dir を確かめたときである。深いパスの親がないというふつうの答えでは確かめ直さない。
root_dir の上を辿れない（権限がない）ときは root_dir がないとせず、権限がない失敗にして処理を続ける。root_dir がディレクトリでないときの理由を見分ける道具がリモートにないときは、その道具がない失敗にする。
root_dir がない文言は一度だけ出す。

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

### REQ-merge-043: 新しいファイルを作る書き込みは、現れたものを書き換えない
- kind: prohibition
- source: docs/decision/records/2026-10-12-foundation-3.md#D37, docs/decision/records/2026-10-12-foundation-3.md#D38, docs/decision/records/2026-10-12-foundation-3.md#D41, docs/decision/records/2026-10-12-foundation-3.md#D42, docs/decision/records/2026-10-12-foundation-3.md#D45
- verification: review
- how_to_verify: ローカルとエージェントは "crates/remote-merge-core/src/machine_ops/new_file_tests.rs" の a_hard_link_placed_after_the_check_for_a_new_file_is_not_written（書き込み先がないと確かめた後・開く前に、別のファイルへのハードリンクを置く）と a_destination_gone_after_the_check_is_made_as_a_new_file、SSH は "crates/remote-merge-ssh/src/change_scripts_race_tests.rs" の a_hard_link_placed_after_the_check_for_a_new_file_is_not_written_like_this_machine と a_link_to_a_device_placed_after_the_check_for_a_new_file_is_file_exists_like_this_machine（dd が "oflag=nofollow" と "conv=excl" を受け付けるときと受け付けないときに、親を作った直後にハードリンクか /dev/null への symlink を置く mkdir を PATH に置いて、生成したコマンドを sh で実行する）と a_new_file_is_opened_by_a_dd_with_excl_and_nofollow_when_the_dd_accepts_them、確かめたときにあった書き込み先が消えたときは "crates/remote-merge-ssh/src/change_scripts_gone_tests.rs" の a_destination_gone_with_its_parent_after_the_check_is_made_again_from_the_parent と a_destination_gone_after_the_check_is_made_as_a_new_file_like_this_machine（最初の呼び出しで書き込み先か親を消す dd を PATH に置く）、エージェントが分けて送られた書き込みの後の分を最初の分で開いたファイルに書くことは "crates/remote-merge-agent/src/agent/dispatch/tests/write_races.rs" の later_parts_go_to_the_file_opened_for_the_first_part_even_if_the_path_is_swapped を読み、どの経路でも書き込みが "File exists" の I/O の失敗になり、ハードリンク先のファイルが変わらないことを人が確かめる。これらの単体テストは kotowari が印を数えるテストファイルの外にある。書き込み先がないと確かめた後に同じパスに項目が現れることは、その競合でだけ起きるため、契約テストでは作らない。確かめたときにあった書き込み先が親ごと消えたときに三つの経路が同じに作り直すことは "crates/remote-merge/tests/contract/write_gone_parent_agree.rs" の契約テストで比べる。

書き込み先がないときの書き込みは、新しいファイルとして作り、ないと確かめた後に同じパスに何か（並行する別の作業が作った通常ファイル・どこを指すかによらない symlink・別のファイルへのハードリンク）が現れていれば、それを開かず、書き換えず、所有者も変えずに、どの経路でも I/O の失敗（理由は "File exists"）にする。
確かめたときにあった書き込み先が開く前に消えていたときも（親ごと消えていたときは、ない親を作り直して）、どの経路でも新しいファイルとして作る（REQ-merge-042 のとおりログインした利用者の持ち物になる）。
エージェントが分けて送られた中身を書くときは、後の分も最初の分で開いたファイルに書き、書いている間に書き込み先のパスが別のファイルに差し替えられても、差し替えたファイルに書かない。
既存の書き込み先の書き換えは REQ-merge-039 による。

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

@id=EX-merge-065 @about=REQ-merge-043 @source=docs/decision/records/2026-10-12-foundation-3.md#D37
Scenario: 新しいファイルを作る間に並行する作業が同じファイルを作る
Given sudo=true のサーバで、書き込み先 "new.txt" がないと確かめた後、作る前に、並行する別のデプロイが "new.txt" を作った
When "new.txt" を merge する
Then "new.txt" は failed に出て、別のデプロイが作った "new.txt" の中身と所有者は変わらない

@id=EX-merge-066 @about=REQ-merge-044 @source=docs/decision/records/2026-10-12-foundation-3.md#D47
Scenario: 三つのファイルの merge の二つ目の前に root_dir ごと消える
Given "a.txt"・"b.txt"・"c.txt" を merge し、"a.txt" を書いた後、"b.txt" を書く前に、書き込み先の root_dir が中身ごと消えた
When merge の結果を見る
Then "a.txt" は書き込まれ、"b.txt" は root_dir がない失敗、"c.txt" は手を付けていない失敗として failed に出て、root_dir は作り直されず、終了コード 2 を返す
```
