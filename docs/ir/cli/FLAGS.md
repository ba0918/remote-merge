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
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A12

旧総合仕様の使い方の例とマージ前確認の節は merge が確認のプロンプトを出し、--force で省略するとするが、CLI の merge は確認のプロンプトを出さず（TUI には確認がある）、--force は機密ファイルを対象に含めること、リモート間の merge を止めないこと、三者の競合の確認をしないことに効く。

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

### FLAG-cli-024: dry-run で競合を確かめない
- kind: gap
- related: REQ-cli-004, REQ-cli-051
- source: docs/decision/records/2026-09-28-adopt-merge-cli.md#A15

--ref がある --dry-run の merge で、実装は三者の競合を確かめないため、実際に実行すると競合で失敗するファイルも merged に status "would merge" で出す。旧資料に記述がなくテストもない。

### FLAG-cli-025: 別々の箇所の変更は競合か
- kind: ambiguity
- related: REQ-cli-016, REQ-cli-017, REQ-cli-051
- source: docs/decision/records/2026-09-28-merge-cli-mutant-flags.md#A1

--ref があり --force のない merge で、参照先に対して左右がテキストのファイルの別々の箇所を変えたとき（参照先 "a b c d e"、左 "A b c d e"、右 "a b c d E" の各行）、実装は重なる変更がないため競合とせず、書き込み先を左の中身で上書きし、右の変更 "E" は失われ、failed は空で終了コード 0 になる。REQ-cli-016 と REQ-cli-051 の「参照先に対して左右が異なる変更」に別々の箇所の変更が含まれるかを IR は決めておらず、含むと読むなら実装は REQ-cli-017 と食い違い、書き込み先の変更を黙って失う不具合の疑いがある。

### FLAG-cli-026: 書き込むファイルのない merge と集約先の場所
- kind: ambiguity
- related: REQ-backup-018, REQ-cli-051
- source: docs/decision/records/2026-09-28-merge-cli-mutant-flags.md#A2

バックアップが有効で集約先の場所が決まらない構成で、書き込むファイルが一つもない merge のとき、実装は全てのファイルが参照先に対する競合で外れた場合はエラーで止まらず failed に "three-way conflict" を出し、全てのファイルが機密ファイルなどのスキップで外れ競合もない場合は "backup store location could not be determined" のエラーで止まる。REQ-backup-018 の「書き込む前にエラーで止まる」が書き込むファイルのない merge に及ぶかを IR は決めていない。
