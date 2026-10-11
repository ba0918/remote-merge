# 書き込み先にない symlink を作るときも、書く前に作成先のパスを確かめる

## Context

[2026-10-12-skip-subtree-consistency-3.md](./2026-10-12-skip-subtree-consistency-3.md) の A1 は、merge と sync の --dry-run が「実行時が書く前のパスの確認をするのと同じパス（比べたときの書き込み先のツリーにない、書き込む予定のパス）」を確かめると決めた。
その根拠は、実行時がツリーにないパスを書く前に `target_path_exists`（crates/remote-merge/src/service/merge_flow.rs）で確かめることだった。
ところが実行時がこの確認をするのは、読み込み元が通常ファイルのとき（`MergeAction::Normal`）だけだった。読み込み元が symlink で書き込み先にない（`MergeAction::CreateSymlink` で `target_exists` が false の）ときは、`missing_parent_directories` で祖先を一つずつ確かめるだけで、作成先のパス自身を確かめていなかった。
ここでは、この違いを IR に照らしてどちらに合わせるかを決める。

Position: A1 で出揃った。

## Agreements

- A1 比べたときの書き込み先になかった symlink を作るときも、通常ファイルを書くときと同じく、作る前に作成先のパスを確かめ（`CoreRuntime::inspect_path`）、エラーならそのパスをそのエラーで失敗にする。IR は変えない。--dry-run の側も変えない。
  - why: [REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) は、パスの確認の結果が途中の祖先がディレクトリでないとき、merge と sync がそのパスを "cannot resolve path: not a directory" のエラーとして扱うと定め、読み込み元の種類で分けていない。[REQ-paths-003](../../ir/paths/resolution.md#REQ-paths-003) は、つないだ実パスが 4096 バイト以上になるパスを辿らずに止めると定める。祖先を一つずつ確かめるだけでは、祖先が通常ファイルであること（その祖先自身は「存在する」と読める）も、末尾の要素をつないだ長さも分からない。IR どうしに矛盾はなく、コードの側が IR に届いていなかったので、コードを IR に合わせる。合わせた後は、consistency-3 A1 の「実行時はツリーにないパスを書く前に確かめる」が読み込み元の種類によらず成り立ち、--dry-run（もともとこのパスを確かめている）と実行時の結果が一致する。試し（コミットしていない一時の結合テスト。契約テストの fixture を使い、ローカルの経路で走らせた）の結果は次のとおり。
    - 書き込み先の "deep" を実パス 4085 バイトのディレクトリへの symlink にし、読み込み元の "deep/longlink.txt" を symlink にして `merge .` を走らせた。--dry-run は `"failed":[{"error":"cannot resolve path: path too long","path":"deep/longlink.txt"}]`・終了コード 2、実行時は `"merged":[{"path":"deep/longlink.txt","status":"ok"}]`・終了コード 0 で、書き込み先の "deep" を通った先に symlink ができていた。sync も同じ形だった。
    - 書き込み先の "x" を通常ファイルにし、読み込み元の "x/lnk" を symlink にして `merge x/lnk` を走らせた。--dry-run は `"cannot resolve path: not a directory"`・終了コード 2、実行時は作成の OS エラー（`File exists (os error 17)`）で失敗した。この fixture では書き込み先がリモートの設定をローカルに差し替えたものなので、そのエラーが「接続断で結果が分からない」の形に包まれて出た。この包み方はこの範囲の外のため調べていない。
    - 直す前、足した契約テスト `merge_new_link_paths`（merge と sync、どちらも --dry-run ありとなし）の 4 件は、--dry-run なしの側で上の出力のとおり落ちた。直した後は 4 件とも通り、全テスト 2427 件が通った（`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features`）。
    - 比べた後に作成先に項目が現れたときの扱い（[REQ-merge-040](../../ir/merge/concurrency.md#REQ-merge-040)）は変えない。確認の結果が「存在する」でも、ここでは失敗にせず、これまでどおり作成そのものの失敗に任せる。
  - decided_by: 一貫性の段（consistency phase）
