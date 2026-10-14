# --dry-run の sync の status と failed の並びを、実行時と同じ決め方にそろえる

## Context

[2026-10-12-skip-subtree-consistency-2.md](./2026-10-12-skip-subtree-consistency-2.md) から [2026-10-12-skip-subtree-consistency-4.md](./2026-10-12-skip-subtree-consistency-4.md) までで、merge と sync の --dry-run は、実行時が書く前に確かめるのと同じパスを確かめ、確認がエラーになるパスを failed に出すようになった。
その後、--dry-run の failed を計画のパスの順に並べ直す変更と、存在しない部分を辿る関数を `resolve_below_missing` から `walk_missing_part`（crates/remote-merge-core/src/path_resolution/walk.rs）に改める変更が入った。consistency-3 の A1 が名前を挙げる `resolve_below_missing` は、この関数の前の名前である。
ここでは、--dry-run の結果が書き込む予定と失敗を両方持つときの sync の status と、failed の並びを、IR に照らしてどう扱うかを決める。

Position: A1 と A2 で出揃った。

## Agreements

- A1 sync の --dry-run は、書き込み先の status を、書き込む予定のファイルを書き込めたファイルとして実行時と同じ判定（[TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007)）で決める。書き込む予定と失敗があれば "partial" にする。コードを IR に合わせ、IR は変えない。
  - why: [REQ-cli-091](../../ir/cli/safety.md#REQ-cli-091) の二文目と、それを決めた [quality-cli A7](./2026-10-05-quality-cli.md#A7)（利用者の決定）は、--dry-run の sync の status を「予定と失敗があれば "partial"」と定めている。コード（crates/remote-merge/src/cli/sync.rs の `build_dry_run_targets`）は失敗が一件でもあると "failed" にしており、IR どうしに矛盾はないので、直す向きはコードである。--dry-run がパスの確認の失敗を failed に出すようになったため、書き込む予定と失敗が同じ書き込み先に混ざる場合が増えていた。試し（コミットしていない一時の結合テスト。書き込み先の "deep" を実パス 4085 バイトのディレクトリへの symlink にし、読み込み元から `sync .` を走らせた）の結果は、--dry-run が `"merged":[{"path":"deep/a","status":"would merge"}]`・failed 4 件・`"status":"failed"`、実行時が同じ failed 4 件で `"status":"partial"` だった。契約テスト `merge_dry_run_paths::sync_previews_a_too_long_path_under_a_known_directory_as_the_same_failure` に status の確かめを足すと、直す前は --dry-run の側で `"status":"failed"` で落ち、`build_dry_run_targets` が実行時と同じ `compute_target_status` を使うようにした後は通り、全テスト 2428 件が通った（`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`）。書き込む予定がない書き込み先（予定がなく失敗だけ、または両方ない）の status は前と変わらない。
  - decided_by: 一貫性の段（consistency phase）
- A2 --dry-run の failed を、実行時と同じく計画のパスの順に並べることは IR に書かない。
  - why: IR は、実行時についても failed の中の並びを定めていない。そのため、並びの違いは IR との食い違いではない。--dry-run の結果の正直さを実行時と同じにする [quality-cli A7](./2026-10-05-quality-cli.md#A7) の向きには沿う。consistency-3 の A1 が問い合わせのまとめ方を IR に書かなかったのと同じく、実装の選択として扱う。試し（上と同じ一時の結合テスト。読み込み元に "deep/a"・"deep/aaaaaaaaaa.txt"・"deep/bbbbbbbbbbb/x"・symlink の "deep/lnkkkkkkkkkk"・"deep/zzzzzzzzzz.txt" を置き、確認の組が二つに分かれるようにした）では、merge と sync のどちらも、--dry-run と実行時の failed が並びまで同じ 4 件になり、終了コードはどちらも 2 だった。
  - decided_by: 一貫性の段（consistency phase）
