# 比べる流れ

diff・status・merge・sync・rollback が二つか三つの側のパスを比べるときに通る一つの流れの段の順と、その流れの判定がコマンド・経路・パスの指定の仕方によらないこと、パスがないことの決め方、参照先と --dry-run の扱い。
判定の規則は judgement.md に、読むものと読めないときの扱いと変更の直前の判定は reading.md に置く。

## Requirements

### REQ-compare-001: 状態・判定・読み込み・もう一度の判定の順に比べる
- kind: ubiquitous
- source: docs/decision/records/2026-10-12-foundation-4.md#E1, docs/decision/records/2026-10-12-foundation.md#A7
- verification: unit

diff・status・merge・sync・rollback は、パスを比べるとき、まず比べる側（左右と、--ref の参照先）のそのパスの状態（ない・通常ファイル・ディレクトリ・symlink・その他の種類と、大きさ・更新時刻・リンク文字列）を走査かパスの確認で集め、集めた状態の組を judgement.md の TBL-compare-001 で判定する。
判定が中身を求めたパスと、コマンドがその判定の扱いに使う中身だけを、reading.md の REQ-compare-010 と REQ-compare-011 のとおり読み、読んだ結果で TBL-compare-002 のとおりもう一度判定する。
コマンドは判定をそのコマンドの扱い（表示・書き込み・スキップ・失敗）に訳すだけで、判定を変えない。

### REQ-compare-002: 判定はコマンド・経路・パスの指定の仕方によらない
- kind: invariant
- source: docs/decision/records/2026-10-12-foundation-4.md#E2, docs/decision/records/2026-10-12-foundation-4.md#E17
- verification: unit

同じ状態の組と同じ比べ方（judgement.md の TBL-compare-003）に対して、判定は、どのコマンドでも、ローカル・SSH・エージェントのどの経路でも、パスを名前で指定したか・ディレクトリの展開か・root_dir 全体か・指定したパスの数・走査の範囲によらず、同じ値を返す。
コマンドごとに違ってよいのは、判定の扱いと、TBL-compare-003 の比べ方だけである。

### REQ-compare-003: パスがないことは集めた状態だけから決める
- kind: prohibition
- source: docs/decision/records/2026-10-12-foundation-4.md#E3, docs/decision/records/2026-10-12-foundation.md#A2, docs/decision/records/2026-10-12-foundation-4.md#E33
- verification: unit

比べる流れは、パスがないことを集めた状態だけから決め、読み込みの失敗から決めない。
名前で指定したパスとディレクトリ（走査の起点）の途中の要素がディレクトリでない側では、その起点のパスはないとし、ほかの起点の状態を集め続ける。
状態ではあったパスの中身かハッシュの読み込みが、ないの失敗（paths/operations.md の REQ-paths-020）になったときは、状態を集めた後にそのパスが変わったものとして reading.md の REQ-compare-013 の判定できないとし、片側だけとも両側にないとも扱わない。
参照先の同じパスがないこと（cli/unreadable.md の REQ-cli-090）と、rollback の戻す先にファイルがないこと（backup/rollback-path.md の REQ-backup-027）も、その側の状態から決める。

### REQ-compare-004: 参照先も同じ状態の集め方と判定で比べる
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E15, docs/decision/records/2026-10-12-foundation-4.md#E7
- verification: unit

--ref を指定したコマンドは、参照先の側のパスの状態も左右と同じ集め方で集め、root_dir の外へ解決されるかも左右と同じに判定し（cli/symlink-diff.md の REQ-cli-026）、左と参照先、右と参照先の組を左右の組と同じ TBL-compare-001 と TBL-compare-002 で判定する。
--follow-external-links のない diff は、参照先の同じパスの途中が root_dir の外を指すときも、その外の中身を参照先との差に出さない。
判定できないのは組ごととし、参照先との組だけが判定できないときは、左右の組の判定を残す。

### REQ-compare-005: --dry-run も実行時と同じ流れと判定を通る
- kind: state_driven
- source: docs/decision/records/2026-10-12-foundation-4.md#E10
- verification: unit

merge と sync の --dry-run は、書き込まないことのほかは、実行時と同じ状態の集め方・判定・確認を通る。
--ref を使う merge の --dry-run は、cli/merge.md の REQ-cli-051 と TBL-cli-009 の確認もし、実行時に書かないファイルを書く予定（status "would merge"）に出さずに、実行時と同じ error で failed に出して終了コード 2 を返す。

## Examples

```gherkin
@id=EX-compare-001 @about=REQ-compare-002 @source=docs/decision/records/2026-10-12-foundation-4.md#E2
Scenario: 同じ組をパスの指定の仕方によらず同じに判定する
Given 左右の "dir/a.txt" はサイズが同じで中身が違う
When "dir/a.txt" を名前で指定した diff、"dir" を指定した diff、パスを指定しない diff を実行する
Then どの diff も "dir/a.txt" を中身の違うファイルとして示す

@id=EX-compare-002 @about=REQ-compare-002 @source=docs/decision/records/2026-10-12-foundation-4.md#E2
Scenario: ディレクトリとディレクトリへの symlink の組をどのコマンドでも種類の違いと判定する
Given 左の "linked" はディレクトリで、右の "linked" はディレクトリへの symlink である
When 左右の状態の組を判定する
Then status・diff・merge のどれから比べても、判定は種類の違いになる

@id=EX-compare-003 @about=REQ-compare-003 @source=docs/decision/records/2026-10-12-foundation-4.md#E3
Scenario: 比べる前に消えたファイルを片側だけとしない
Given 左右の状態では "a.txt" は両側にある通常ファイルで、中身を読む前に右の "a.txt" が消える
When "a.txt" を比べる
Then "a.txt" は左だけにあるファイルとも差分のあるファイルともされず、判定できないパスとして理由とともに示される

@id=EX-compare-004 @about=REQ-compare-004 @source=docs/decision/records/2026-10-12-foundation-4.md#E15
Scenario: 参照先の root_dir の外の中身を出さない
Given 参照先の "shared" は root_dir の外のディレクトリを指す symlink で、左右と参照先に "shared/a.txt" があり中身が違う
When --follow-external-links を付けずに --ref を指定して diff --format json を実行する
Then "shared/a.txt" の参照先との差に root_dir の外の中身は出ない

@id=EX-compare-005 @about=REQ-compare-005 @source=docs/decision/records/2026-10-12-foundation-4.md#E10
Scenario: --dry-run でも参照先から変わった書き込み先を書く予定に出さない
Given 読み込み元は参照先と同じで、書き込み先だけが参照先から変わったファイル "a.txt" がある
When --ref と --dry-run を指定して "a.txt" を merge する
Then "a.txt" は "would merge" で出ず、failed に error "destination changed since reference" で出て、終了コード 2 を返す
```
