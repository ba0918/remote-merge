# レビューで残った 3 件の裁定（--ref・書きかけ・名前で指定したファイル）

## Context

段 5 のレビュー（2026-10-15 の diff レビューと境界のレビュー）で、IR の間で決まっていない、
または IR が決めていない振る舞いが 3 件残った。利用者に判断材料を返し、2026-10-15 に裁定を
受けた。ここに 3 件と、それぞれの反映を記録する。

## Agreements

- A1 `--ref` で左右が通常ファイル、参照先だけがリンク先を辿れない symlink のときは、参照先との組を
  判定できないものとしてそのパスと理由を errors に出し、終了コード 2 を返す（今までの振る舞いの
  まま）。IR の REQ-cli-090・TBL-cli-005 が「種類の違い → differs、errors に載せない」と読めるのに
  対し、REQ-scan-018 は「読むだけのコマンドは errors に出す」とするので、前者に後者の例外を書き足した。
  - why: 2026-10-13 の利用者の判断（読むだけのコマンドは辿れない symlink を errors に出して終了
    コード 2 にする）と揃える。参照先が通常ファイルを指す symlink のときは、今までどおり種類の違い
    として errors に載せない。
  - decided_by: 利用者（2026-10-15）
- A2 sync で、書き込みの途中の `接続断` の後に「書きかけ」と確かめたファイルも、TBL-cli-007 の
  「書き込めたファイル」に数え、それしかない書き込み先の status を "partial" にする（ファイルは
  failed にも出す）。
  - why: F86（2026-10-14 の利用者の判断）の「中身が変わったものは partial」にそろえる。F86 は
    権限の変更の失敗と、古いリンクを消した後の作成の失敗を含めたが、書きかけは利用者の判断が
    挙げられていないとして含めていなかった。中身が一部変わっているのに "failed" だけにすると、
    書き込み先に何も起きていないように見える。
  - decided_by: 利用者（2026-10-15）
- A3 merge・sync で名前で指定したファイルには include と exclude を当てず、指定どおり書く。フィルタに
  当たって親ディレクトリの下の走査に載らなかったファイルは、起点そのものとしてもう一度集めて比べる。
  指定したディレクトリの配下と root_dir 全体には、今までどおりフィルタを当てる。
  - why: diff の REQ-cli-081（名前で指定したファイルにはフィルタを当てない）と同じにする。今までは
    親ディレクトリの下の走査だけを使っていたので、フィルタに当たったファイルが「両側にない」になり、
    実在して中身も違うのに書かれなかった。
  - decided_by: 利用者（2026-10-15）

## Position

A1〜A3 は反映済み。

- A1: `docs/ir/cli/unreadable.md`（REQ-cli-090）と `docs/ir/cli/status-targets.md`（TBL-cli-005）に
  例外を書き足した。実装は変更していない（今までの振る舞いのまま）。
- A2: `docs/ir/cli/sync.md`（REQ-cli-041）に書き足し、`merge_apply.rs` の
  `is_partially_written` で書きかけを `Applied::FailedAfterChange` にした。契約テスト
  `a_sync_whose_only_change_is_partially_written_is_partial` を足した。
- A3: `docs/ir/merge/basic.md`（REQ-merge-022）と `docs/ir/scan/limits.md`（REQ-scan-019）に書き足し、
  `merge_files.rs` の `compare_named_files` を merge と sync の両方から呼ぶようにした。契約テスト
  `merge_filtered_named_paths.rs` を足した。

A2・A3 の実装は、境界のレビュー（round 3）の後にレビュー（他の領域の 1 回目）で確かめる。
