# パスの辿り方

root_dir の下のパスを確認するときにパスを辿る規則、辿れないときの止め方、確認の結果の種類。
ローカル・SSH・エージェントのどの経路でも同じ規則で辿る。

## Requirements

### REQ-paths-001: パスを "/" から要素ごとに辿る
- kind: ubiquitous
- source: docs/decision/records/2026-10-06-path-resolution.md#A1, docs/decision/records/2026-10-06-path-resolution.md#A5, docs/decision/records/2026-10-06-path-resolution.md#A6, docs/decision/records/2026-10-06-path-resolution.md#A10, docs/decision/records/2026-10-06-path-resolution.md#A19, docs/decision/records/2026-10-06-path-resolution.md#A24, docs/decision/records/2026-10-06-path-resolution.md#A33, docs/decision/records/2026-10-06-path-resolution.md#A38
- verification: unit

パスの確認は、root_dir と確認するパスをつないだ絶対パスを "/" から要素ごとに辿り、末尾以外で出会った symlink はリンク先に置き換えて辿り続ける。空の要素と "." の要素は無視し、リンク先の ".." は辿り終えた実ディレクトリの親へ移り、"/" の親は "/" とする。一つの要素では名前の長さ、つないだ長さ、検索権限、存在と種類、ディレクトリかの順に確かめ、最初に当たった条件で止め、まだ辿っていない部分は見ない。空のパスの確認は、root_dir が symlink でも辿って存在するという結果を返す。

### REQ-paths-002: 確認の結果は四つの種類のどれかになる
- kind: ubiquitous
- source: docs/decision/records/2026-10-06-path-resolution.md#A7, docs/decision/records/2026-10-06-path-resolution.md#A8, docs/decision/records/2026-10-06-path-resolution.md#A9, docs/decision/records/2026-10-06-path-resolution.md#A12, docs/decision/records/2026-10-06-path-resolution.md#A24, docs/decision/records/2026-10-06-path-resolution.md#A27, docs/decision/records/2026-10-06-path-resolution.md#A28, docs/decision/records/2026-10-06-path-resolution.md#A35, docs/decision/records/2026-10-06-path-resolution.md#A37, docs/decision/records/2026-10-06-path-resolution.md#A44
- verification: unit

パスの確認の結果は次の四つのどれかか、"cannot resolve path: " に "path too long"・"too many levels of symbolic links"・"not a directory"・"permission denied"・"readlink not available"・"dangling symbolic link"・"parent reference in missing part"・"non-UTF-8 path"・"cannot read" のどれかを続けたエラーである。実パスかリンク文字列に UTF-8 でない部分があれば "cannot resolve path: non-UTF-8 path" で止める。
存在しないは、存在する最も近い祖先の実パスに存在しない途中の要素をつないだ親の実パス（末尾の要素を含めない）と、親が存在するかの印を持つ。
存在するは、symlink でないもの（ディレクトリを含む）の実パスを持つ。
末尾の symlink は、リンク文字列そのものと、リンク先を同じ規則で辿った実パスを持ち、リンク先が存在しなければ、リンク先の側の辿れた親の実パスにリンク先の最後の名前をつないだものを実パスとする。
途中の祖先がディレクトリでないは、確認するパスの要素のうちディレクトリでなかったものの実パスと、それが symlink なら辿った先を含めて通常ファイルかそれ以外かを持ち、root_dir 自身かそれより上の要素がディレクトリでないときはこの結果にせず "cannot resolve path: not a directory" で止める。

### REQ-paths-003: 長すぎるパスは辿らずに止める
- kind: prohibition
- source: docs/decision/records/2026-10-06-path-resolution.md#A3, docs/decision/records/2026-10-06-path-resolution.md#A21, docs/decision/records/2026-10-06-path-resolution.md#A32, docs/decision/records/2026-10-06-path-resolution.md#A35
- verification: unit

パスの確認は、次の要素をつないだ直後の実パス（リンク切れで組み立てる親と名前をつないだものを含み、末尾の "/" は除く）が 4096 バイト以上になるとき、または辿る名前の要素が 255 バイトを超えるとき、その先を辿らずに "cannot resolve path: path too long" で止める。入力の文字列とリンク文字列そのものの長さでは止めず、この上限はマシンごとの設定で変えない。

### REQ-paths-004: symlink は 40 回までしか辿らない
- kind: prohibition
- source: docs/decision/records/2026-10-06-path-resolution.md#A4, docs/decision/records/2026-10-06-path-resolution.md#A31, docs/decision/records/2026-10-06-path-resolution.md#A35
- verification: unit

パスの確認は、リンク文字列をパスに差し込んだ回数を、末尾のリンクと root_dir 自身のリンクを含めて数え、41 回目を差し込もうとした時点で "cannot resolve path: too many levels of symbolic links" で止める。

### REQ-paths-005: 最後の要素の "/" はディレクトリを求める
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A5, docs/decision/records/2026-10-06-path-resolution.md#A26, docs/decision/records/2026-10-06-path-resolution.md#A30, docs/decision/records/2026-10-06-path-resolution.md#A35
- verification: unit

確認全体の最後の要素の後に "/" があり、その要素を辿った先が存在してディレクトリでないとき、パスの確認は "cannot resolve path: not a directory" で止める。途中の要素の後の "/" と、後ろにまだ確認する要素が続くリンク先の "/" は、何も足さない。

### REQ-paths-006: 検索できないディレクトリの中は見ない
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A11, docs/decision/records/2026-10-06-path-resolution.md#A34, docs/decision/records/2026-10-06-path-resolution.md#A35, docs/decision/records/2026-10-06-path-resolution.md#A46, docs/decision/records/2026-10-06-path-resolution.md#A48, docs/decision/records/2026-10-06-path-resolution.md#A54
- verification: unit

辿る途中のディレクトリに、実行する利用者が実効 uid で検索する権限を持たないとき、パスの確認はその中の名前を見ずに "cannot resolve path: permission denied" で止める。Unix 以外の環境では、OS が権限のエラーを返したときだけこの理由で止める。

### REQ-paths-007: 辿れないリンク先では止める
- kind: state_driven
- source: docs/decision/records/2026-10-06-path-resolution.md#A6, docs/decision/records/2026-10-06-path-resolution.md#A25, docs/decision/records/2026-10-06-path-resolution.md#A29, docs/decision/records/2026-10-06-path-resolution.md#A35
- verification: unit

途中の祖先の symlink のリンク先が存在しないとき、パスの確認は "cannot resolve path: dangling symbolic link" で止める。リンク先の存在しない部分に ".." があるときは "cannot resolve path: parent reference in missing part" で止める。末尾の symlink のリンク先を辿る途中で止まったときは、確認の結果をそのエラーとし、リンク先が途中の祖先がディレクトリでないところで終わるときは "cannot resolve path: not a directory" とする。

## Examples

```gherkin
@id=EX-paths-001 @about=REQ-paths-001 @source=docs/decision/records/2026-10-06-path-resolution.md#A5,docs/decision/records/2026-10-06-path-resolution.md#A12
Scenario: "." と重なった "/" を含むリンク先
Given root_dir の下に "dir/file.txt" があり、"link" が "dir/./file.txt" を指し、"link2" が "dir//file.txt" を指す
When "link" と "link2" を確認する
Then どちらも末尾の symlink で、実パスは root_dir の実パスに "/dir/file.txt" をつないだものになる

@id=EX-paths-002 @about=REQ-paths-001 @source=docs/decision/records/2026-10-06-path-resolution.md#A38
Scenario: root_dir 自身が symlink
Given root_dir がディレクトリ "real" を指す symlink である
When 空のパスを確認する
Then 結果は存在するで、実パスは "real" の実パスになる

@id=EX-paths-003 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A7,docs/decision/records/2026-10-06-path-resolution.md#A27
Scenario: 祖先が通常ファイルに変わった
Given root_dir の下の "dir" が通常ファイルになっている
When "dir/a.txt" を確認する
Then 結果は途中の祖先がディレクトリでないで、その要素の実パスは root_dir の実パスに "/dir" をつないだもの、種類は通常ファイルになる

@id=EX-paths-004 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A26,docs/decision/records/2026-10-06-path-resolution.md#A27
Scenario: "/" で終わるリンク先の祖先が通常ファイルを指す
Given root_dir の下で "l" が "file.txt/" を指し、"file.txt" は通常ファイルである
When "l/a.txt" を確認する
Then 結果は途中の祖先がディレクトリでないで、種類は通常ファイルになる

@id=EX-paths-005 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A27
Scenario: root_dir 自身が通常ファイル
Given root_dir が通常ファイルになっている
When "a.txt" を確認する
Then 確認は "cannot resolve path: not a directory" で止まる

@id=EX-paths-006 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A28
Scenario: 親も存在しない
Given root_dir の下に "missing" が存在しない
When "a.txt" と "missing/a.txt" を確認する
Then どちらも結果は存在しないで、"a.txt" は親が存在する印を持ち、"missing/a.txt" は親の実パスが root_dir の実パスに "/missing" をつないだもので親が存在しない印を持つ

@id=EX-paths-007 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A37
Scenario: リンク切れの末尾の symlink
Given root_dir の下で "a/l" が "../b/missing" を指し、"b" はディレクトリで "b/missing" は存在しない
When "a/l" を確認する
Then 結果は末尾の symlink で、実パスは root_dir の実パスに "/b/missing" をつないだものになる

@id=EX-paths-008 @about=REQ-paths-003 @source=docs/decision/records/2026-10-06-path-resolution.md#A3,docs/decision/records/2026-10-06-path-resolution.md#A32,docs/decision/records/2026-10-06-path-resolution.md#A35
Scenario: symlink の先で実パスが長くなる
Given 実パスが 4060 バイトのディレクトリを root_dir の下の "deep" が指し、"link" が "deep/" に 40 バイトの存在しない名前をつないだものを指す
When "link" を確認する
Then 確認は "cannot resolve path: path too long" で止まる

@id=EX-paths-009 @about=REQ-paths-003 @source=docs/decision/records/2026-10-06-path-resolution.md#A21,docs/decision/records/2026-10-06-path-resolution.md#A12
Scenario: 最後の "/" は長さに数えない
Given "link" のリンク先は "/" で終わり、その "/" を除いて辿った実パスがちょうど 4095 バイトのディレクトリになる
When "link" を確認する
Then 結果は末尾の symlink で、実パスはそのディレクトリになる

@id=EX-paths-010 @about=REQ-paths-003 @source=docs/decision/records/2026-10-06-path-resolution.md#A32,docs/decision/records/2026-10-06-path-resolution.md#A42,docs/decision/records/2026-10-06-path-resolution.md#A12,docs/decision/records/2026-10-06-path-resolution.md#A24
Scenario: 入力の文字列が長くても実パスが短ければ辿る
Given root_dir の下の "link" が "./" を 2040 回繰り返した後に "file.txt" を続けたものを指し、root_dir の実パスとつなぐと 4096 バイトを超え、"file.txt" は通常ファイルである
When "link" を確認する
Then 結果は末尾の symlink で、実パスは root_dir の実パスに "/file.txt" をつないだものになる

@id=EX-paths-011 @about=REQ-paths-004 @source=docs/decision/records/2026-10-06-path-resolution.md#A31
Scenario: ちょうど上限の回数
Given root_dir の下で "l1" から "l40" までの 40 個のリンクが順に次を指し、"l40" が "file.txt" を指す
When "l1" を確認する
Then 結果は末尾の symlink になる

@id=EX-paths-012 @about=REQ-paths-004 @source=docs/decision/records/2026-10-06-path-resolution.md#A31,docs/decision/records/2026-10-06-path-resolution.md#A35
Scenario: 上限を一回超える
Given root_dir の下で "l1" から "l41" までの 41 個のリンクが順に次を指し、"l41" が "file.txt" を指す
When "l1" を確認する
Then 確認は "cannot resolve path: too many levels of symbolic links" で止まる

@id=EX-paths-013 @about=REQ-paths-005 @source=docs/decision/records/2026-10-06-path-resolution.md#A26,docs/decision/records/2026-10-06-path-resolution.md#A35,docs/decision/records/2026-10-06-path-resolution.md#A29,docs/decision/records/2026-10-06-path-resolution.md#A5
Scenario: 連なったリンクの先が "/" の付いた通常ファイル
Given root_dir の下で "link" が "inner" を指し、"inner" が "file.txt/" を指し、"file.txt" は通常ファイルである
When "link" を確認する
Then 確認は "cannot resolve path: not a directory" で止まる

@id=EX-paths-014 @about=REQ-paths-005 @source=docs/decision/records/2026-10-06-path-resolution.md#A30,docs/decision/records/2026-10-06-path-resolution.md#A37
Scenario: "/" で終わるリンク先が存在しない
Given root_dir の下の "l" が "newdir/" を指し、"newdir" は存在しない
When "l" を確認する
Then 結果は末尾の symlink で、実パスは root_dir の実パスに "/newdir" をつないだものになる

@id=EX-paths-015 @about=REQ-paths-006 @source=docs/decision/records/2026-10-06-path-resolution.md#A11,docs/decision/records/2026-10-06-path-resolution.md#A34
Scenario: 検索できないディレクトリの中
Given root_dir の下の "locked" に、確認する root でない利用者の検索の権限がない
When "locked/a.txt" を確認する
Then 確認は "cannot resolve path: permission denied" で止まる

@id=EX-paths-016 @about=REQ-paths-007 @source=docs/decision/records/2026-10-06-path-resolution.md#A25
Scenario: 途中の祖先がリンク切れの symlink
Given root_dir の下の "dir" が存在しない "gone" を指す
When "dir/a.txt" を確認する
Then 確認は "cannot resolve path: dangling symbolic link" で止まる

@id=EX-paths-017 @about=REQ-paths-007 @source=docs/decision/records/2026-10-06-path-resolution.md#A6,docs/decision/records/2026-10-06-path-resolution.md#A35
Scenario: リンク先の存在しない部分の ".."
Given root_dir の下の "link" が "missing/../file.txt" を指し、"missing" は存在しない
When "link" を確認する
Then 確認は "cannot resolve path: parent reference in missing part" で止まる

@id=EX-paths-018 @about=REQ-paths-007 @source=docs/decision/records/2026-10-06-path-resolution.md#A29,docs/decision/records/2026-10-06-path-resolution.md#A4,docs/decision/records/2026-10-06-path-resolution.md#A35
Scenario: 自分を指す末尾の symlink
Given root_dir の下の "loop" が自分自身を指す
When "loop" を確認する
Then 確認は "cannot resolve path: too many levels of symbolic links" で止まる

@id=EX-paths-023 @about=REQ-paths-002 @source=docs/decision/records/2026-10-06-path-resolution.md#A44
Scenario: UTF-8 でない名前
Given root_dir の下の "link" が UTF-8 でないバイトを含む名前を指す
When "link" を確認する
Then 確認は "cannot resolve path: non-UTF-8 path" で止まる
```
