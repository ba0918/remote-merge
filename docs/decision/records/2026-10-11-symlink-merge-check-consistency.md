# symlink の merge の書き込み先の確認で、UTF-8 でないリンク先を変わっていないと見なさない

## Context

[symlink-merge-check A1](./2026-10-11-symlink-merge-check.md#A1) は、比べたときに symlink だった書き込み先を消す前に、リンク先の文字列が比べたときと同じかを確かめると決めた。
実装は、比べたときのツリーと、書き込み先の親ディレクトリの子の一覧のリンク先を文字列で比べる。
どちらの文字列も、リンク先の UTF-8 でないバイトを置換文字 U+FFFD に置き換えて作る（"crates/remote-merge-core/src/listing.rs" の child_node、"crates/remote-merge-agent/src/agent/dispatch.rs"、"crates/remote-merge-ssh/src/tree_parser.rs"）。
そのため、UTF-8 でないバイトだけが違うリンク先に作り直されても同じ文字列になり、書き込み先が変わったことを見落として上書きする。
実装と IR を突き合わせ、この差を測って直した。あわせて、A1 の Context が挙げた `ReplaceSymlinkWithFile` の分岐に merge から届くかを測った。

Position: 決定は A1・A2 で出揃った。

## Agreements

- A1 比べたときに symlink だった書き込み先のリンク先が UTF-8 として読めないバイトか置換文字 U+FFFD を含むときは、比べたときと今のリンク先の文字列が同じでも変わっていないことを確かめられないので、merge と sync は書き込み先を変えずにそのファイルを failed に出す。例 EX-merge-055 を [REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033) に足す。
  - why: [REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033) は比べたときから書き込み先の中身が変わったファイルを書かずに failed に出すとし、[REQ-merge-011](../../ir/merge/concurrency.md#REQ-merge-011) は他者の更新を上書きすることを禁じる。リンク先を置換文字に置き換えた文字列では、UTF-8 でないバイトだけが違う作り直しを見分けられない。測ると、比べた後に書き込み先の symlink を "\xff.txt" から "\xfe.txt" を指すように作り直してバックアップを無効にして merge すると、修正前はローカル・SSH・エージェントの三つの経路とも Merged になり、作り直したリンクを a.txt で上書きした（`cargo nextest run --all-features --test contract merge_symlink_changed_routes` の ex_merge_055 の修正前の結果 `left: [(false, Some("a.txt")), (false, Some("a.txt")), (false, Some("a.txt"))]`）。バイト列のまま比べるにはツリーと子の一覧のリンク先の型を三つの経路で変える必要があり、この変更の範囲を超える。確かめられないものを failed に出すのは、禁じられた上書きを起こさない唯一の既定である。バックアップが有効なときは、修正前から save_symlink_update がこうしたリンク先で "backup failed: cannot resolve path: non-UTF-8 path" の failed にしていたので、バックアップの有無で結果が分かれていた差もなくなる。UTF-8 として正しく U+FFFD そのものを含むリンク先も、置き換えた結果と見分けられないので同じく failed になる。
  - rejected: 文字列が同じなら置き換える（修正前の実装）。上の測定のとおり、比べた後の作り直しを上書きし、REQ-merge-033 と REQ-merge-011 に反する。
  - decided_by: 整合の確認（consistency phase）。REQ-merge-033 と REQ-merge-011 から導いた
- A2 `ReplaceSymlinkWithFile` の分岐（書き込み先が symlink で読み込み元にない）には、書き込み先の確認を足さない。merge からはこの分岐に届かず、そのパスは右だけのファイルとしてスキップされる。
  - why: 読み込み元と書き込み先の両方にあり種類が違う組は determine_merge_action が SkipDifferentKind にし、読み込み元にないパスは status が RightOnly になって filter_merge_candidates が merge の対象から外す。使い捨ての統合テストで、左に link.txt がなく右に b.txt を指す symlink link.txt がある状態でバックアップを無効にして execute_merge を動かすと、paths に "link.txt" を渡しても "." を渡しても `skipped=[("link.txt", "right-only file (use --delete to remove)")]` になり、symlink は b.txt を指したまま残った。sync も同じ execute_single_merge と同じ絞り込みを通る（コードの読みによる。sync では測っていない）。[symlink-merge-check](./2026-10-11-symlink-merge-check.md#A1) の Context がこの分岐を確かめない場所に挙げたのはコードの読みで、届くかどうかは確かめていなかった。
  - decided_by: 整合の確認（consistency phase）。測定による
