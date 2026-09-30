# Flags

## Flags

### FLAG-cli-001: agent の出力の形と出す条件
- kind: contradiction
- related: REQ-cli-032
- source: docs/decision/records/2026-09-27-adopt-status.md#A15

旧総合仕様の JSON 出力スキーマは agent を {"status": "connected"} の形で「該当する場合のみ」出すとするが、実装は "agent": "connected"（または "fallback"）の文字列で出し、-v を指定して右がリモートのときだけ出す。テキストでも同じ条件のときだけ "Agent: connected" または "Agent: fallback (SSH exec)" の行を出す。

### FLAG-cli-002: 参照先との違いの印の値
- kind: contradiction
- related: REQ-cli-035
- source: docs/decision/records/2026-09-27-adopt-status.md#A16

旧総合仕様は ref_badge の値を "differs"・"exists_only_in_ref"・"missing_in_ref" とするが、実装は三つすべてで中身が同じファイルと、片方だけにあって参照先にもないファイルに "all_equal" を付ける（テキストでは表示しない）。一覧が左右の和集合のため "exists_only_in_ref" は status では出ず、ref_only は常に 0 になる。

### FLAG-cli-003: --checksum での symlink の比べ方
- kind: ambiguity
- related: REQ-cli-008, REQ-cli-028
- source: docs/decision/records/2026-09-27-adopt-status.md#A17

旧個別仕様 symlink-merge の 3.8 は --checksum でも片方でも symlink のペアは中身を読み比べないとするが、実装は --checksum で symlink のペアも比較対象に含める。右がリモートで --ref がないときはハッシュの経路、それ以外は中身を読む経路で比べるため、経路によって symlink の組の判定が旧資料どおりになるかがコードから確かめきれない。

### FLAG-cli-004: 中身を読めなかったファイルの判定
- kind: gap
- related: REQ-cli-027
- source: docs/decision/records/2026-09-27-adopt-status.md#A18

status が中身を読み比べる対象のファイルを読めなかったとき（読み取りの失敗やサイズ上限の超過）、実装は理由を示さずにメタデータでの判定（"modified"）のまま一覧に出す。旧資料に記述がなくテストもない。

### FLAG-cli-005: 三者比較での機密ファイルの判定
- kind: gap
- related: REQ-cli-027, REQ-cli-036
- source: docs/decision/records/2026-09-27-adopt-status.md#A19

--ref を指定した status は機密ファイルの中身を読まないため、サイズが同じで更新時刻が違い中身が同じ機密ファイルを "modified" のまま出し、--ref なしなら "equal" に直す。旧資料に記述がなくテストもない。

### FLAG-cli-006: パスを省いた sync
- kind: contradiction
- related: REQ-cli-038
- source: docs/decision/records/2026-09-27-adopt-sync.md#A9

旧総合仕様のマルチサーバ同期の節はパスを省いた "sync --left local --right server1 server2 server3" で全体を同期する例を示すが、実装はパスを一つ以上必須とし、全体は "." で指定する。同じ旧資料の使い方の例 "sync . --left local --right server1 server2" とは一致する。

### FLAG-cli-007: 確認を断ったときの JSON
- kind: ambiguity
- related: REQ-cli-018, REQ-cli-040
- source: docs/decision/records/2026-09-27-adopt-sync.md#A10

--format json の sync で確認を断ると、実装は標準出力に何も出さずに終了コード 0 で終わる。JSON 指定なら成功とエラーのどちらでも JSON を返すという REQ-cli-018 に取り消しが含まれるかの読みが分かれる。

### FLAG-cli-008: 接続に失敗した書き込み先の並び
- kind: gap
- related: REQ-merge-015, REQ-cli-039
- source: docs/decision/records/2026-09-27-adopt-sync.md#A11

接続またはツリーの取得に失敗した書き込み先を、実装は --right の指定順によらず結果の末尾に並べる。旧資料に記述がなくテストもない。

### FLAG-cli-009: 読み込み元に接続できない sync
- kind: gap
- related: REQ-merge-015
- source: docs/decision/records/2026-09-27-adopt-sync.md#A12

読み込み元への接続またはツリーの取得に失敗したとき、実装は書き込み先ごとの結果を出さずに sync 全体をエラーで止める。旧資料に記述がなくテストもない。

### FLAG-cli-010: 種類の違いによるスキップと書き込み先の状態
- kind: contradiction
- related: REQ-cli-041, REQ-cli-044, REQ-merge-001
- source: docs/decision/records/2026-09-27-adopt-sync.md#A13

旧個別仕様 symlink-merge の 3.3 は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある書き込み先の status を "partial" にし全体の終了コードを 2 にするとするが、実装は status の判定に skipped を数えないため、そのスキップだけの書き込み先は "success" になり終了コードは 0 になる。

### FLAG-cli-011: 削除の成否と書き込み先の状態
- kind: gap
- related: REQ-cli-041
- source: docs/decision/records/2026-09-27-adopt-sync.md#A14

実装は書き込み先の status の判定に削除の成否を数えないため、書き込んだファイルがなく削除だけがある書き込み先で一部の削除に失敗すると、削除できたファイルがあっても "partial" ではなく "failed" になる。旧資料に記述がなくテストもない。

### FLAG-cli-012: sync のテキスト出力の形
- kind: contradiction
- related: REQ-cli-041, REQ-cli-042
- source: docs/decision/records/2026-09-27-adopt-sync.md#A15

旧総合仕様のマルチサーバ同期の節は dry-run の出力を書き込み先ごとの "[server1] 3 files to merge (2 modified, 1 added)" と "M"・"+" の記号付きのファイルの行、最後の "Total: 7 merge operations across 3 servers" で示す。実装のテキスト出力は "Sync: 元 → 先1, 先2" の見出し、書き込み先ごとの "[先] success" の行、"plan"・"ok"・"D"・"skip"・"FAILED" を付けたファイルの行、最後の "Summary: N/M servers successful, N files merged" の行を出し、書き込む予定がなく失敗もないときは先頭に "No files to sync." を出す。

### FLAG-cli-013: dry-run の削除予定の表示
- kind: gap
- related: REQ-cli-045, REQ-cli-004
- source: docs/decision/records/2026-09-27-adopt-sync.md#A16

--dry-run の sync で書き込む予定がなく削除予定だけがある書き込み先を、実装はテキストで "(deleted)" と削除済みの形で出し、JSON でも deleted の status を実際に削除したときと同じ "ok" にするため、予定と実行済みを区別できない。旧資料に記述がなくテストもない。

### FLAG-cli-014: dry-run の終了コード
- kind: contradiction
- related: REQ-cli-044, REQ-cli-004
- source: docs/decision/records/2026-09-27-adopt-sync.md#A17

旧個別仕様 symlink-merge の 3.3 は --dry-run の終了コードを 0 とするが、実装は --dry-run の sync でも中身を読み比べるファイルを読めなかった書き込み先を "failed" にし、終了コード 2 を返す。

### FLAG-cli-015: 確認の行で件数が 0 の部分を省くか
- kind: ambiguity
- related: REQ-cli-040
- source: docs/decision/records/2026-09-27-sync-mutant-flags.md#A1

sync の書き込む前の確認で、実装は書き込むファイルだけがある書き込み先を "[先] N files to merge"、削除予定だけがある書き込み先を "[先] M files to delete" と件数が 0 の部分を省いて出す。REQ-cli-040 は行の形を "[先] N files to merge, M files to delete" と書いており、件数が 0 の部分を省くかは定めていない。旧総合仕様の出力例 "[server1] 3 files to merge (2 modified, 1 added)" は削除の部分を出していない。

### FLAG-cli-016: 種類の違いによるスキップと merge の終了コード
- kind: contradiction
- related: REQ-cli-047, FLAG-cli-010, REQ-merge-001
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A7

旧個別仕様 symlink-merge の 3.3 は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある merge の終了コードを 2 にするとするが、実装は merge の終了コードの判定に skipped を数えないため、そのスキップだけの merge は終了コード 0 になる。

### FLAG-cli-017: merge の dry-run の終了コード
- kind: contradiction
- related: REQ-cli-047, REQ-cli-004, FLAG-cli-014
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A8

旧個別仕様 symlink-merge の 3.3 は --dry-run の終了コードを 0 とするが、実装は --dry-run の merge でも中身を読み比べるファイルを読めなかったとき failed に出し、終了コード 2 を返す。

### FLAG-cli-018: 機密ファイルのスキップ理由の文言
- kind: contradiction
- related: REQ-cli-048, REQ-cli-003
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A9

旧総合仕様の JSON 出力スキーマは merge の JSON で機密ファイルのスキップの reason を "sensitive" とするが、実装は "sensitive file" とする。

### FLAG-cli-019: merge のスキップの行の形
- kind: contradiction
- related: REQ-cli-049
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A10

旧個別仕様 symlink-merge の 3.3 はスキップのテキスト出力を "  - パス (skipped: 理由)" の行とするが、実装の merge は "Skipped: パス (理由)" の行で出す。

### FLAG-cli-020: リモート間の merge の確認
- kind: contradiction
- related: REQ-cli-003, REQ-cli-047
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A11

旧総合仕様のサーバ間比較の節はリモート間の merge でサーバ名を入力させる確認を出し、--force で確認を省略できるとするが、CLI の実装は確認を出さずに --force も --dry-run もないリモート間の merge を止め、テキストでは "Warning: merging between two remote servers (左 → 右)" と "Use --force to proceed, or --dry-run to preview changes." を出し、JSON では failed に path が "" の一件を出し、終了コード 2 を返す。

### FLAG-cli-021: merge の確認のプロンプトと --force の働き
- kind: contradiction
- related: REQ-cli-003
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A12, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A10

旧総合仕様の使い方の例とマージ前確認の節は merge が確認のプロンプトを出し、--force で省略するとするが、CLI の merge は確認のプロンプトを出さず（TUI には確認がある）、--force は機密ファイルを対象に含めること、リモート間の merge を止めないこと、REQ-cli-051 の参照先に対する確認をしないことに効く。

### FLAG-cli-022: 機密ファイルの警告と件数
- kind: contradiction
- related: REQ-cli-003
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A13

旧総合仕様のセンシティブファイル警告の節は機密ファイルを検知したらマージの前に警告して続行するかを尋ねるとするが、CLI の merge は尋ねずに --force のない機密ファイルをスキップし、テキスト形式のときだけ（--dry-run でも）標準エラーに "N sensitive file(s) will be skipped. Use --force to include them." を出す。この N には書き込み先にだけあるファイルなど機密ファイル以外のスキップも数えるため、機密ファイルがなくても件数が出ることがある。

### FLAG-cli-023: 三者の中身がそろわないファイル
- kind: gap
- related: REQ-cli-017, REQ-cli-051
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A14

--ref があり --force も --dry-run もない merge で、実装は左・右・参照先のどれかで中身がそろわないファイルを書き込まずに failed に error "three-way comparison incomplete" で出すため、左にだけある新しいファイルは --ref 付きでは --force なしに書き込めない。旧資料に記述がなくテストもない。

### FLAG-cli-024: dry-run で参照先に対する確認をしない
- kind: gap
- related: REQ-cli-004, REQ-cli-051
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A15, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A10

--ref がある --dry-run の merge で、実装は REQ-cli-051 の参照先に対する確認をしないため、実際に実行すると REQ-cli-051 で書き込まれないファイルも merged に status "would merge" で出す。旧資料に記述がなくテストもない。

### FLAG-cli-026: 書き込むファイルのない merge と集約先の場所
- kind: ambiguity
- related: REQ-backup-018, REQ-cli-051
- source: docs/decision/records/2026-09-28-merge-cli-mutant-flags.md#A2, docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A10

バックアップが有効で集約先の場所が決まらない構成で、書き込むファイルが一つもない merge のとき、実装は全てのファイルが REQ-cli-051 の参照先に対する確認で外れた場合はエラーで止まらず failed にその確認の error（"three-way conflict" か "destination changed since reference"）を出し、全てのファイルが機密ファイルなどのスキップで外れ REQ-cli-051 の確認で外れたファイルもない場合は "backup store location could not be determined" のエラーで止まる。REQ-backup-018 の「書き込む前にエラーで止まる」が書き込むファイルのない merge に及ぶかを IR は決めていない。

### FLAG-cli-027: 参照先を使う merge での symlink
- kind: ambiguity
- related: REQ-cli-051, REQ-merge-003
- source: docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A9

--ref を使う merge で読み込み元か書き込み先が symlink のとき、REQ-cli-051 の確認が symlink を何で比べるか（リンク先の文字列か、辿った先の中身か）を実装から確かめておらず、REQ-cli-051 の対象から外している。

### FLAG-cli-028: 旧総合仕様の diff の JSON の例
- kind: contradiction
- related: REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A11

旧総合仕様の diff の JSON の例は一つのファイルのオブジェクト（left・right に updated_at、conflict_count など）だが、実装は常に files の配列を持つ形で、left・right は label と root を持つ。

### FLAG-cli-029: リモートのバイナリのハッシュを計算する場所
- kind: contradiction
- related: REQ-cli-061
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A12

旧総合仕様はリモートのファイルのハッシュをリモート側で計算し、ダウンロードしてからの計算を禁じるが、実装の diff はファイルの中身を全て読んでからローカルでハッシュを計算する（実装を読んで分かったことで未実行）。

### FLAG-cli-030: 片側で読めないファイルの diff
- kind: gap
- related: REQ-cli-056
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A13

左右にあるファイルが片側で読めないとき、diff はその側を空として全ての行を削除または追加として出し、終了コード 1 を返してエラーにしない。旧資料に記述がなくテストもない。

### FLAG-cli-031: パスを指定した diff と exclude
- kind: contradiction
- related: REQ-config-003, REQ-cli-052
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A14

旧総合仕様の除外フィルターの節は exclude のパターンを比較の対象から除外するとするが、20 個以下のパスを指定した diff は exclude に当たるファイルのパスでも差分を出す（パスなしの diff は除外する）。

### FLAG-cli-032: 0 始まりの行番号
- kind: gap
- related: REQ-cli-058, REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A15

diff のテキストの "@@" の行と JSON の left_start・right_start は 0 始まりの行番号で、先頭の行の変更が "@@ -0,1 +0,1 @@" になる。旧資料に記述がなく、行番号を確かめるテストもない。

### FLAG-cli-033: include の外のディレクトリを指定した diff
- kind: gap
- related: REQ-config-004, REQ-cli-052
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A16

include の外のディレクトリを指定した diff は、ローカルと SSH の比較ではそのディレクトリを中身のない項目として出して終了コード 0 を返し、ほかの経路では見つからないパスのエラーになると実装からは読める。旧資料に記述がない。

### FLAG-cli-034: files_with_changes が数えるもの
- kind: contradiction
- related: REQ-cli-053
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A17

利用者向けの手引きは summary の files_with_changes を hunk を一つ以上持つファイルの数とするが、実装はハッシュの違うバイナリ、リンク先の違う symlink、中身を隠した機密ファイルも数える。

### FLAG-cli-035: 中身の同じバイナリの diff
- kind: contradiction
- related: REQ-cli-009, REQ-cli-061
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A18

REQ-cli-009 はバイナリの一致または不一致を示すとするが、中身の同じバイナリのパスを指定した diff は同じハッシュのまま "Binary files differ" と出し、終了コード 0 を返す。

### FLAG-cli-037: ディレクトリの配下の変更のない機密ファイル
- kind: gap
- related: REQ-cli-059, REQ-cli-056
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A20

ディレクトリのパスを指定した diff は、配下の変更のない機密ファイルも中身を隠した形で出し、変更のあるファイルに数えて終了コード 1 を返す（実装を読んで分かったことで未実行）。

### FLAG-cli-038: 片側にだけある中身のないファイル
- kind: gap
- related: REQ-cli-060
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A21

片側にだけある 0 バイトのファイルの diff は、両側を空として差分なしと扱い、見出しだけを出して終了コード 0 を返す。ディレクトリを展開したときは項目を出さず、ない側について標準エラーに "treating as empty" の警告を出すと実装からは読める。

### FLAG-cli-039: 100MB を超えるファイルの diff
- kind: gap
- related: REQ-cli-061
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A22

100MB を超えるファイルの diff は、ローカルと SSH の経路では読み取りに失敗してその側を空か "missing" として扱うが、エージェントの経路では大きさの制限なく読むと実装からは読める（未実行）。旧資料に記述がない。

### FLAG-cli-041: 読まずに数えたファイルが読むと同じになる場合
- kind: contradiction
- related: REQ-cli-055
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A24

先頭の 8,192 バイトより後にだけ不正な UTF-8 を含み、読むと左右が同じテキストになるファイルがあると、--max-files が 1 のときだけそのファイルを読まずに変更のあるファイルと数え、truncated を true、changed_files_total を実際より多く出す。REQ-cli-055 は変更のあるファイルを数えるとする。

### FLAG-cli-042: 境目で切れた多バイトの文字
- kind: ambiguity
- related: REQ-cli-061
- source: docs/decision/records/2026-09-29-adopt-diff-output.md#A25

正しい UTF-8 のテキストでも、先頭の 8,192 バイトの境目で多バイトの文字が切れるとバイナリと判定される。REQ-cli-061 の「先頭 8,192 バイトに不正な UTF-8 を含む」に境目で切れた文字が当たるかは読み分けられない。

### FLAG-cli-044: SSH の側だけ参照先がない symlink
- kind: contradiction
- related: REQ-cli-022
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A2

REQ-cli-022 は symlink の参照先がないか読めないとき空ファイルと同一扱いせずエラー終了するとするが、SSH の経路の側だけ参照先がない symlink は、エラーにならずその側を空として比べ、終了コード 1 を返す（ローカルの側ならエラーになる）。

### FLAG-cli-045: SSH の側のディレクトリ symlink と通常ファイル
- kind: contradiction
- related: REQ-cli-021, EX-cli-061
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A3

REQ-cli-021 と EX-cli-061 はディレクトリ symlink と通常ファイルの組でも配下を入口からの子パスで示すとするが、ローカルの側が通常ファイルで SSH の側がディレクトリ symlink のとき、diff はリンクの配下の子ファイルを出さない（逆の向きは出す）。

### FLAG-cli-046: 機密ファイルを指す同じ symlink
- kind: contradiction
- related: REQ-cli-020
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A4

REQ-cli-020 はリンク文字列・種類・内容が全て同じなら差分なしとするが、機密ファイルを指す symlink は、--force なしで内容を隠すとき、左右のリンク文字列も参照先の内容も同じでも差分ありと数え、終了コード 1 を返す。

### FLAG-cli-047: パスを指定しない diff の symlink
- kind: ambiguity
- related: REQ-cli-020, REQ-cli-021
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A5

REQ-cli-020 と REQ-cli-021 の文は経路を限らないが、その文書の導入は symlink を明示指定したときとし、パスを指定しない diff では、リンク文字列が同じ symlink を一覧に出さず参照先の内容も比べない。要件がパスを指定しない diff にも当たるかが読み分けられない。

### FLAG-cli-048: 種類の違いの note の文言
- kind: contradiction
- related: REQ-cli-020
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A6

REQ-cli-020 は解決後の種類の差を示すとするが、note は相手側に項目がない片側だけの symlink でも "type mismatch: symlink vs file"、相手側が通常ファイルのディレクトリ symlink でも "type mismatch: symlink vs directory" と出し、相手側の種類を正しく示さない場合がある（実装を読んで分かったことで未実行）。

### FLAG-cli-049: ディレクトリの子の一覧の件数の打ち切り
- kind: contradiction
- related: REQ-cli-021
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A7

REQ-cli-021 は件数超過を不完全な比較としてエラーで報告するとするが、diff がディレクトリの子を一覧するときは一つのディレクトリにつき 10,000 件で黙って打ち切り、不完全な比較として報告しない（実装を読んで分かったことで未実行）。

### FLAG-cli-050: 通常のディレクトリの展開の理由のテキスト
- kind: contradiction
- related: REQ-cli-022
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A8

REQ-cli-022 は比較できなかったパスと理由を示すとするが、通常のディレクトリの展開で循環や件数超過が起きたとき、テキスト出力には理由が出ない（JSON では errors に出る。実装を読んで分かったことで未実行）。

### FLAG-cli-052: 比べなかったときの hunks のリンク文字列
- kind: contradiction
- related: REQ-cli-024
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A10

REQ-cli-024 はリンク文字列と参照先の内容差を区別して示すとするが、機密として隠したとき、root_dir の外で内容を比べなかったとき、参照先のディレクトリが読めないとき、循環したときにも hunks にリンク文字列の削除と追加の行が残り、テキストではその前に "Resolved content differs" と出る（実装を読んで分かったことで未実行）。

### FLAG-cli-053: symlink の参照先の内容の --max-lines
- kind: contradiction
- related: REQ-cli-054
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A11

REQ-cli-054 は --max-lines で打ち切ったファイルの truncated を true にするとするが、symlink の参照先の内容の差分は、打ち切っても truncated を true にせず、テキストにも "... (output truncated)" を出さない（実装を読んで分かったことで未実行）。

### FLAG-cli-054: symlink の diff の仕様のない挙動
- kind: gap
- related: REQ-cli-020
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A12

diff は、リンク文字列も参照先の内容も同じ symlink を差分のない項目として files に残してテキストにも "Link target:" を出し、100MB を超える参照先を --force を付けても読めないエラーにし、絶対パスの指定を拒否しない（実装を読んで分かったことで未実行）。旧資料に記述がなくテストもない。

### FLAG-cli-055: 手引きの symlink の欄名
- kind: contradiction
- related: REQ-cli-024
- source: docs/decision/records/2026-09-30-adopt-diff-links.md#A13

利用者向けの手引き "skills/remote-merge/references/json-schemas.md" の Symlink の例はリンク先を "left_symlink_target" と "right_symlink_target" で出すとするが、REQ-cli-024 と実装は link_targets の left と right で出す。
