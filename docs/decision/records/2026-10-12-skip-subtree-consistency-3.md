# --dry-run が書き込み先のパスの確認を、実行時と同じパスに同じ手順でする

## Context

[2026-10-12-skip-subtree-consistency-2.md](./2026-10-12-skip-subtree-consistency-2.md) の A1 は、merge と sync の --dry-run が書き込む予定のパスについて実行時と同じパスの確認をすると IR に書いた。
同時に、確認するパスを「比べたときの書き込み先のツリーになく、祖先がツリーで分かるディレクトリだけではないもの」に絞ることを、利用者から見える違いを生まない実装の選択として IR に書かなかった。
その後、確認をツリーにない祖先ごとに一度にまとめる変更が入った。
ここでは、この二つの絞り方が実行時と同じ結果になるかを確かめ、IR とコードをどう合わせるかを決める。

Position: A1 で出揃った。

## Agreements

- A1 merge と sync の --dry-run は、実行時が書く前のパスの確認をするのと同じパス（比べたときの書き込み先のツリーにない、書き込む予定のパス）を確かめ、確認がエラーになるパスを、途中の祖先がディレクトリでない場合に限らず、実行時と同じエラーとして failed に出す。[REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) の --dry-run の一文をそう書き直し、例 [EX-paths-039](../../ir/paths/routes.md#EX-paths-039) を足す。祖先がすべてツリーで分かるディレクトリのパスも確かめる。問い合わせは親ごとに一度にまとめる。まず最初のパスを確かめ、ないと分かれば、残りのパスはその結果の親の実パスと名前から、存在しない部分を辿る手順（`resolve_below_missing`）で求める。consistency-2 の A1 の「絞ることは利用者から見える違いを生まない」は成り立たないので、この決定で置き換える。
  - why: パスの確認は、存在しない部分でも長さの上限を確かめる（[REQ-paths-003](../../ir/paths/resolution.md#REQ-paths-003)、crates/remote-merge-core/src/path_resolution/walk.rs の `joined_within_limits`）。そのため、祖先がディレクトリと分かっていても、書き込み先でつないだ実パスが 4096 バイト以上になるパスは、実行時は "cannot resolve path: path too long" で失敗する（crates/remote-merge/src/service/merge_flow.rs の `target_path_exists`）。試し（コミットしていない一時の結合テスト。書き込み先の "deep" を実パス 4085 バイトのディレクトリへの symlink にし、読み込み元から root_dir 全体を merge した）の結果は次のとおり。
    - 506b12e では、ない祖先の下の "deep/newdir/f.txt" が --dry-run で `"merged":[{"path":"deep/newdir/f.txt","status":"would merge"}]`・終了コード 0 だった。実行時は `"failed":[{"error":"cannot resolve path: path too long",...}]`・終了コード 2 だった。
    - "deep" の直下の "deep/ffffffffff.txt" も、--dry-run では "would merge"・0、実行時は同じ失敗・2 だった。
    - ない祖先の下の側は d8db11e で直った。直下の側は d8db11e の後も同じ食い違いが残った（同じ試しで確認）。
    - [REQ-cli-004](../../ir/cli/safety.md#REQ-cli-004) と [quality-cli A7](./2026-10-05-quality-cli.md#A7) は dry-run の結果の正直さを実行時と同じにすると決めている。実行すると失敗するパスを "would merge" と出すのはこれに反し、IR の「実行時と同じパスの確認」とも合わない。直す向きはコードである。
    - 問い合わせの回数に IR の決まりはない。そのため、まとめ方は IR に書かない。親ごとにまとめても、確かめるパスとその結果は実行時と同じになる。理由は三つある。最初のパスの確認が、親の検索権限と長さを実行時と同じ手順で確かめる。ツリーで分かるディレクトリの直下でツリーにないパスは、比べたときに存在しない。存在しない部分の手順は d8db11e の単体テストで全体を辿るのと同じ結果になる。
    - 直した後の契約テスト `merge_dry_run_paths` に足した 2 件（merge と sync。どちらも --dry-run ありとなしで走らせる。"deep/a" は予定に出て、"deep/longname.txt" だけが path too long で failed に出て、終了コード 2）は、直す前に --dry-run の側で `"failed":[]` と両方の "would merge" で落ちた。
  - decided_by: 一貫性の段（consistency phase）
