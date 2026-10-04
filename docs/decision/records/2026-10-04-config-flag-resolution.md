# 設定の判断のいらない FLAG を閉じる際の判断

## Context

設定の話題には、取り込みのときに未決として残した FLAG が溜まっていた。
2026-10-04 に利用者が「仕分けして判断がいらないものを全て対処して」と指示したため、すべての FLAG を読み取りだけで仕分け、承認済みの要件か現行のコードで決まるものを選んだ。
この記録は、そのうち設定の FLAG について、既に決まっているので閉じるものと、現行の挙動をそのまま仕様にするものを残す。
利用者の判断が要るものと、前提が確かめられなかったものは FLAG のまま残す。

## Agreements

- A1 FLAG-config-009 を IR を変えずに閉じる。パーミッションは桁数を問わず "0o" で始まる形か数字だけの形を 8 進数として読み、"64" や "7" のような 3 桁でない形も拒まない。
  - why: 承認済みの [REQ-config-015](../../ir/config/values.md#REQ-config-015) は "0o" で始まる形か数字だけの形を 8 進数として読むと定めて桁数を制限せず、その元の [A2 (パーミッションの読み方)](./2026-09-28-adopt-config-values.md#A2) も同じである。実装も桁数を見ない。3 桁の形だけを挙げるのは旧資料だけである。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A2 FLAG-config-014 を IR を変えずに閉じる。既定の sensitive のパターンは常に使い、設定の sensitive のパターンはそれに足すだけで、既定のパターンを外す方法は設けない。
  - why: 承認済みの [REQ-config-026](../../ir/config/filters.md#REQ-config-026) は 6 つのパターンを常に sensitive とし設定のパターンをそれに足すと定め、その元の [A6 (既定の sensitive)](./2026-09-29-adopt-config-filters.md#A6) も同じである。実装も同じで、既定を「上書き」できるとするのは旧資料だけである。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
- A3 FLAG-config-003 を閉じ、[TBL-config-001](../../ir/config/defaults.md#TBL-config-001) にトップレベルの max_scan_entries（既定値 50000）と badge_scan_max_files（既定値 500）の行を足し、[REQ-config-027](../../ir/config/precedence.md#REQ-config-027) を足す。二つのキーは、プロジェクト設定にあればプロジェクト側、プロジェクト設定になくグローバル設定にあればグローバル側、どちらにもなければ既定値を使う。旧資料の [scan] セクションと既定値 100,000・5,000 は採らない。[scan] セクションに書いた値を知らせずに無視することは、未決の FLAG-config-007 の範囲として決めない。あわせて、利用者向けの手引き "skills/remote-merge/SKILL.md" の設定の Full reference で [agent] の下に書かれていた二つのキーをトップレベル（最初のセクションより前）に移す。
  - why: 実装は二つのキーをトップレベルで読み、既定値を 50,000 と 500 とし、キーごとにプロジェクト設定、グローバル設定、既定値の順で選ぶ。利用者向けの手引きもトップレベルのキーで同じ既定値とする。旧資料の [scan] セクションと別の既定値は旧総合仕様にしかない。手引きの例は二つのキーを [agent] の後に置いており、TOML では [agent] の中のキーになって読まれないため、実装が読む位置に直す。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
  - superseded_by: [A1（TUI を完全に取り除く）](./2026-10-04-tui-disposition.md#A1)
- A4 FLAG-config-008 を閉じ、[REQ-config-028](../../ir/config/loading.md#REQ-config-028) を足す。init・logs・events に --config を指定したときは "Warning: --config is ignored for the 'サブコマンド名' subcommand" を標準エラーに出し、設定を読まずに続ける。
  - why: 今の実装は三つのサブコマンドでこの警告を出し、設定を読む処理を呼ばずに続ける。三つはどれも設定を使わないため、指定先がなくても [REQ-config-007](../../ir/config/loading.md#REQ-config-007) のエラーで止めずに続けるのは、--config が設定の読み込み先を指定するものだと定める [REQ-config-006](../../ir/config/loading.md#REQ-config-006) と矛盾しない。旧資料には記述がなく、決め直す材料もない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
  - superseded_by: [A1（TUI を完全に取り除く）](./2026-10-04-tui-disposition.md#A1)
- A5 FLAG-config-010 を閉じ、[REQ-config-029](../../ir/config/values.md#REQ-config-029) を足す。auth が "key" のサーバに password が書かれているときは "servers.サーバ名: password is set but auth is 'key' — password will be ignored" の警告を出し、その password を認証に使わない。
  - why: 今の実装は設定を読むときにこの警告を出し、鍵による認証の経路は password を読まない。CLI では警告が標準エラーに出る。auth の値を "key" か "password" に限る [REQ-config-014](../../ir/config/values.md#REQ-config-014) と、auth が "key" のサーバの鍵のパスを定める [REQ-config-020](../../ir/config/values.md#REQ-config-020) に沿った挙動で、旧資料には記述がなく、決め直す材料もない。
  - decided_by: AI（2026-10-04 の利用者の指示「判断がいらないものは全て対処」による委任。仕分けの推奨を採用）
