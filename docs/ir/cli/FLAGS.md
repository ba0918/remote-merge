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
