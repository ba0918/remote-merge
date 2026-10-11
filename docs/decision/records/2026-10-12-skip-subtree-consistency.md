# 種類の違いでスキップしたパスの配下を外した後の、IR とコードの突き合わせで決めたこと

## Context

[2026-10-12-skip-subtree.md](./2026-10-12-skip-subtree.md) の A1〜A4 を実装した後、変えたコードを IR と読み比べた。
実装は、配下のパスを引数で直接指定したときも、種類の違う祖先を一件のスキップにして配下を外す。ただし書き込み先の祖先がディレクトリでないときは、配下を残して書くときの "cannot resolve path: not a directory" の失敗に任せる。
[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001) は「スキップした対象の配下」としか書いておらず、引数に入っていない祖先をどう扱うかが書かれていなかった。
ここでは、試しの実行で確かめた事実をもとに、この違いを IR が支えるかと、IR にどう書くかを決める。

Position: A1 で出揃った。

## Agreements

- A1 --delete を付けて、書き込み先にだけある配下のパス（例 "x/only.txt"）を直接指定し、その祖先 "x" が読み込み元と書き込み先で種類が違うときも、"x" をスキップした対象として扱い、配下を消さず、skipped には "x" を一件だけ出す。読み込み元の配下のパスを直接指定し、書き込み先の祖先がディレクトリでないときは、[REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) のとおり "cannot resolve path: not a directory" の失敗にする。[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001) にこの一文を足し、例 [EX-merge-061](../../ir/merge/symlink.md#EX-merge-061) を足す。コードは変えない。
  - why: 書き込み先の祖先が通常ファイルの側は、[EX-paths-022](../../ir/paths/routes.md#EX-paths-022) がまさに「書き込み先の "dir" が通常ファイルで "dir/a.txt" を merge するとエラー」と定め、[path-resolution A16](./2026-10-06-path-resolution.md#A16) は「ディレクトリでない」の結果をスキップへ読み替えてよいのは rollback だけとしている。そのため、この側を `merge .` と同じスキップにそろえると IR に反する。どちらの書き方でも書き込み先は変わらないので、[skip-subtree A4](./2026-10-12-skip-subtree.md#A4) が問題にした「書き方で書く・書かないが変わる」違いにも当たらない。もう一方の側（書き込み先の祖先がディレクトリ）は、直接指定したパスが読み込み元に存在しえないので、書き込み先にだけあるパスになり、違いが出るのは --delete のときだけである。そこで祖先をスキップした対象として扱うのは、[skip-subtree A3](./2026-10-12-skip-subtree.md#A3)（スキップしたパスの下は消さない）と A4 の「書き方によらず同じ結果」に沿う具体化で、IR と矛盾しない。試し（契約テストの fixture で merge と sync をそれぞれ書き込む回と --dry-run の回で動かした。コミットしていない）: 読み込み元の "x" が通常ファイル、書き込み先の "x" が "x/only.txt" を持つディレクトリで "x/only.txt" を指定すると、--delete 付きは merge・sync とも `"deleted":[],"skipped":[{"path":"x","reason":"source and destination have different file types"}]` で、--delete なしは `"skipped":[{"path":"x/only.txt","reason":"right-only file (use --delete to remove)"}]`（[REQ-merge-024](../../ir/merge/deletion.md#REQ-merge-024) のとおり）だった。--delete なしは走査が指定したパスだけを見る（`resolve_scan_strategy` は --delete のときだけ全体を走査する）ため祖先の種類を判定に使わないが、どちらも何も消さないので、IR の一文は --delete のときに限った。読み込み元の "x" が "x/a.txt" を持つディレクトリ、書き込み先の "x" が通常ファイルで "x/a.txt" を指定すると、書き込む回は --delete の有無によらず merge・sync とも `"failed":[{"error":"cannot resolve path: not a directory","path":"x/a.txt"}]` だった。直す前の契約テストでは、--delete 付きの "x/only.txt" の指定でファイルが消えていた（実装の報告の RED）。
  - decided_by: 一貫性の段（consistency phase）
