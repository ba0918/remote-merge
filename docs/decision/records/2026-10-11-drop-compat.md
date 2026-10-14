# このツールの古い版との互換の分岐を捨てる

## Context

[2026-10-11-agent-operations.md の A3](./2026-10-11-agent-operations.md#A3) で、利用者は「このツールの古い版との互換の分岐はすべて捨てる」と決め、エージェントの版の互換以外を、すぐ後の別の単位で捨てるとした。この記録はその単位である（品質の台帳の ao-1）。
A3 が名前を挙げたのは、設定の使われなくなったキー（[REQ-config-030](../../ir/config/filters.md#REQ-config-030)・[REQ-config-031](../../ir/config/filters.md#REQ-config-031)）と、捨てると知らないキーになるので未実装の [REQ-config-039](../../ir/config/loading.md#REQ-config-039) の実装、バックアップのセッションの古い探し方（[REQ-backup-043](../../ir/backup/sessions.md#REQ-backup-043)）、古い置き場の除外（[REQ-backup-016](../../ir/backup/storage.md#REQ-backup-016)）、--ref の別名 --reference、コードの中の互換の名残である。
利用者は 2026-10-11 に、この単位からは自分に戻さず自走してよいと指示した。ここに書く決定は、どれも A3 とその前提の決定から導いたもので、新しい方針は足していない。サーバの道具の違いを吸収する分岐（[A4](./2026-10-11-agent-operations.md#A4)）は互換に数えず、この単位でも変えない。

Position: 決定は A1〜A7 で出揃った。A7 は実装の後のレビューで分かったことを利用者が決めた。

## Agreements

- A1 [filter] の sensitive と badge_scan_max_files を、警告して読み飛ばすのをやめ、製品の知らないキーとして扱う。[REQ-config-030](../../ir/config/filters.md#REQ-config-030)・[REQ-config-031](../../ir/config/filters.md#REQ-config-031) を IR から消す。
  - why: A3 が名前を挙げた。どちらも以前の版の設定を動かし続けるためだけの警告である（[drop-sensitive-file-handling A4](./2026-10-04-drop-sensitive-file-handling.md#A4)・[tui-disposition A5](./2026-10-04-tui-disposition.md#A5)）。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3)。2026-10-11 の自走の指示により、この単位で書き下ろした）
- A2 [agent] の timeout_secs・tree_chunk_size・max_file_chunk_bytes も、警告して読み飛ばすのをやめ、製品の知らないキーとして扱う。[REQ-config-042](../../ir/config/defaults.md#REQ-config-042) を IR から消す。
  - why: A3 の「設定の使われなくなったキー」に当たる。この三つを警告にした [quality-config A6](./2026-10-05-quality-config.md#A6) 自身が「badge_scan_max_files と同じく」と書いていて、同じ種類の互換である。A3 は ID を 030・031 だけ挙げたが、残すと「使われなくなったキー」の互換が一つの種類だけ残る。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3) から導いた）
- A3 設定の知らないキーとセクションのエラー（[quality-config A3](./2026-10-05-quality-config.md#A3)）を、例外なしで実装する。[REQ-config-039](../../ir/config/loading.md#REQ-config-039) から sensitive と badge_scan_max_files の例外の文を消す。A1・A2 の五つのキーを書いた設定もこのエラーで止まり、終了コード 2 を返す。文言は今までどおり実装者に委ねる（[quality-config A10](./2026-10-05-quality-config.md#A10)）。
  - why: A3 が「捨てると知らないキーになるので REQ-config-039 の実装を含む」と決めた。例外の文は、A1 で消す要件を指していた。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3)）
- A4 バックアップのセッションは、今の書き込み先の識別（[REQ-backup-021](../../ir/backup/sessions.md#REQ-backup-021)）だけで探す。以前の版の識別で保存したセッションは一覧にも rollback にも期限切れの整理にも出さず、集約先に残ったままにする。[REQ-backup-043](../../ir/backup/sessions.md#REQ-backup-043) と例 EX-backup-024・EX-backup-028 を IR から消す。EX-backup-023 は、この版で保存したセッションどうしの例として REQ-backup-021 だけに結びつける。
  - why: A3 が名前を挙げた。以前の識別でも探すことにした [quality-backup A1](./2026-10-05-quality-backup.md#A1) は「既存のセッションが見えなくならない」ことを条件にしていたが、後の A3 で利用者はこの互換も捨てると決めた。以前の識別で保存したセッションを整理しないのは、設定にない書き込み先の履歴を消さない [REQ-backup-009](../../ir/backup/sessions.md#REQ-backup-009) と同じ扱いになる。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3)）
- A5 書き込み先にある ".remote-merge-backup/" を特別に扱うのをやめる。設定の exclude に常に足していたこの名前を足さず、ほかのディレクトリと同じく走査・比較・書き込みの対象にする。除きたい利用者は exclude に書く。[REQ-backup-016](../../ir/backup/storage.md#REQ-backup-016) を IR から消し、境目の例を [REQ-config-003](../../ir/config/filters.md#REQ-config-003) に足す。
  - why: A3 が「古い置き場の除外」として名前を挙げた。".remote-merge-backup/" は以前の版が書き込み先に作ったバックアップの置き場で（[adopt-backup A30](./2026-09-27-adopt-backup.md#A30)）、今の版は集約先にだけ保存する（[REQ-backup-001](../../ir/backup/storage.md#REQ-backup-001)）。名前を常に除く今の実装は、どの深さの同じ名前のディレクトリも黙って対象から外していた。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3)）
- A6 diff・merge・sync の --ref の別名 --reference を捨てる。--reference を書くと、ほかの知らないオプションと同じく使い方のエラーになる。IR は別名を書いていないので、IR は変えない。
  - why: A3 が名前を挙げた。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3)）
- A7 設定ファイルを解釈できないときのエラー（TOML の書き方の誤り、値の型の違い、知らないキーとセクション）は、標準エラーにも --format json の error にも、設定ファイルの行の抜き出しと、書かれた値を含めない。どの設定ファイルの何行目・何列目か、どのキーか、なぜ解釈できないかは出してよい。[REQ-config-043](../../ir/config/loading.md#REQ-config-043) として IR に足す。
  - why: 実装の後のレビューで、`[servers]` の一行に password と書き間違いのキーが並ぶ設定で、エラーがその行をそのまま出し、パスワードが標準エラーと JSON の error に出ると分かった（手元で再現した）。型の違う値でも、この単位より前から同じように出ていた。A3 で知らないキーをエラーにしたので、出るきっかけが増えた。利用者は「今直す」と決めた。REQ-config-043 で `docs/ir/config/loading.md` は要件の数の目安を超えるが（kotowari の too_many_requirements）、設定ファイルの読み込みとそのエラーの規則は一つの文書にあるほうが探しやすいので、分けずに残す。
  - decided_by: 利用者（2026-10-11）

## Prohibitions

- P1 捨てたキー・別名・以前の識別のセッション・".remote-merge-backup/" を、別の形で受け付ける道（移し替え、捨てたことを知らせる専用の警告やエラーの文言）は作らない。
  - why: 互換の分岐を捨てる A3 と逆になる。知らないキーのエラー（A3）と使い方のエラー（A6）で止まれば、利用者は書いたものが使えないと分かる。
  - decided_by: 利用者（[agent-operations A3](./2026-10-11-agent-operations.md#A3) から導いた）
- P2 サーバの道具の違いを吸収する分岐（[agent-operations A4](./2026-10-11-agent-operations.md#A4)）は、この単位で変えない。
  - why: 利用者が互換に数えないと決めた。
  - decided_by: 利用者（2026-10-11）

## Rejected

- R1 A2 の [agent] の三つのキーは警告のまま残す。
  - why: 「設定の使われなくなったキー」の互換が一つの種類だけ残り、A3 の「すべて捨てる」に合わない（A2）。
- R2 以前の識別のセッションを今の識別へ移す。
  - why: 移し替えも互換の分岐で、A3 と逆になる（P1）。

## Revisions

- 2026-10-11: [drop-sensitive-file-handling A4](./2026-10-04-drop-sensitive-file-handling.md#A4)・[A9](./2026-10-04-drop-sensitive-file-handling.md#A9)、[tui-disposition A5](./2026-10-04-tui-disposition.md#A5) の badge_scan_max_files の警告を A1 で、[quality-config A6](./2026-10-05-quality-config.md#A6) を A2 で、[quality-config A3](./2026-10-05-quality-config.md#A3) の例外の文を A3 で、[quality-backup A1](./2026-10-05-quality-backup.md#A1)・[A5](./2026-10-05-quality-backup.md#A5) と [path-resolution A58](./2026-10-06-path-resolution.md#A58) の以前の識別の探し方を A4 で、[adopt-backup A30](./2026-09-27-adopt-backup.md#A30) を A5 で置き換えた。
- 2026-10-11: 実装の後のレビューで、設定のエラーがパスワードを出すと分かり、A7 を足した。
