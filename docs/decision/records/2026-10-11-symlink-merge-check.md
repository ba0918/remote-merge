# symlink の merge でも、比べたときからの書き込み先の変化を確かめる

## Context

品質の台帳の merge_sync-9（T10）を直す単位である。[REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033) は、比べたときから書き込み先の中身が変わったファイルを書かずに failed に出すと決めている。ところが、読み込み元が symlink の merge（書き込み先に symlink を作る）と、書き込み先が symlink の merge（symlink を消して通常ファイルを書く）は、書き込み先を消す前にこの確認をしない。比べた後に書き込み先の symlink が通常ファイルに替わっていると、バックアップが無効のときはそのファイルが写しもなく消える（コードを読んで確かめた。`crates/remote-merge/src/service/merge_flow.rs` の `MergeAction::CreateSymlink` と `MergeAction::ReplaceSymlinkWithFile`）。

利用者は 2026-10-11 に、品質の台帳の単位を順に自走して進めてよいと指示した。ここに書く決定は REQ-merge-033 の当てはめで、新しい方針は足していない。

Position: 決定は A1 で出揃った。

## Agreements

- A1 書き込み先が比べたときに symlink だったファイルは、merge と sync で消して置き換える前に、今も symlink で、リンク先の文字列が比べたときと同じかを確かめる。違えば（通常ファイルやディレクトリに替わった、リンク先が変わった、消えた）、REQ-merge-033 の「比べたときから書き込み先の中身が変わった」として書かずに failed に出し、書き込み先は変えない。例 EX-merge-053・EX-merge-054 を REQ-merge-033 に足す。読み込み元と書き込み先の種類が違う組は今も書かずにスキップするので（`determine_merge_action` の SkipDifferentKind）、比べたときに通常ファイルだった書き込み先を symlink で置き換えることはない。
  - why: REQ-merge-033 は書き込み先の種類を分けずに「書き込み先の中身が変わったファイル」と決めている。symlink の中身として比べているのはリンク先の文字列である（[REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017) の一覧もリンク先を読んだ文字列そのままで返す）。確かめずに消すと、比べた後に置かれたファイルが、バックアップが無効のときは写しもなく失われる。
  - decided_by: 利用者（[merge-backup-flag-resolution A9](./2026-10-04-merge-backup-flag-resolution.md#A9) の REQ-merge-033 の当てはめ。2026-10-11 の自走の指示により、この単位で書き下ろした）
