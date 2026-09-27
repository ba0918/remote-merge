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
