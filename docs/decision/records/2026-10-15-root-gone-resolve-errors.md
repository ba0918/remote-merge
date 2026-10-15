# root_dir を辿れない誤りを root_dir がないとして扱う範囲

## Context

レビュー（2026-10-15 の diff レビューと境界のレビュー）で、root_dir の確認の答えが
"not a directory" の誤り（祖先が通常ファイル）のときの扱いが、include の確認だけ揃って
いないことが見つかった。直したうえで再確認すると、同じ種類の場面がまだ残っていた。

- root_dir そのものが通常ファイル（や、それを指す symlink）のとき、include の確認は
  kind を見ずに root として受け取り、値の確認が "not a directory" の誤りで止まる
  （include を設定していない同じ場面は走査の root_dir の誤りになる）。
- root_dir の祖先がリンク切れの symlink のとき、include の確認はその誤り
  （"dangling symbolic link"）をそのまま返す。まとめた問い（`inspect_under_root`）と
  変更の途中の確かめ直し（`RootWatch`）も同じ誤りを root_dir がないとしない。
  走査は同じ状態を RootMissing として止める（"Path not found: root_dir"）。

走査（core の `root_identity` と `failure_kind`）は、root_dir を辿れない答えを
RootMissing・RootNotDirectory として止め、その種類を RootGone にする（D47）。権限がない
（RootUnreadable）だけは root_dir がないとせず、権限がない失敗にして続ける
（REQ-merge-044）。まとめた問いと確かめ直しをこの範囲に揃える。

## Agreements

- A1 root_dir の答えが "not a directory"（祖先がディレクトリでない）か "dangling symbolic
  link"（祖先がリンク切れの symlink）の誤りのときは、root_dir がないとして尋ねた側の誤りに
  する。権限がない・ループ・読めない root_dir・接続断は、元の答えのままにする。
  - why: 走査はどちらも RootMissing / RootNotDirectory として止める（REQ-scan-015・017）。
    REQ-merge-044 の「root_dir がなくなった（ディレクトリでなくなったもの、祖先がディレクトリ
    でなくなったものを含む）」に、リンク切れの祖先（存在しない祖先）も含めて読む。
  - decided_by: AI（レビューの bd-r2-1・bd-r2-2・bd-r2-3 への答え。走査の答えに揃える）
- A2 まとめた問い（`inspect_under_root`）・変更の途中の確かめ直し（`RootWatch::root_is_gone`）・
  include の確認（`check_include`）の 3 か所で、同じ判定 `is_root_gone_error` を使う。
  include の確認は、root_dir が読めるディレクトリのときだけ受け取り、それ以外は走査の最初の
  問いに任せる。include の値の確認の誤りは、今までどおりその誤りで止める（REQ-config-032）。
  - why: 同じ規則が当たる場所を 1 つの判定にまとめる（規則の置き場所を 1 つにする）。
    include を設定していない同じ場面と同じ誤り・同じ文言になる。
  - decided_by: AI（レビューの bd-r2-1・bd-r2-2）
- A3 root_dir の祖先がループしている（"too many levels of symbolic links"）ときは、root_dir が
  ないとせず、辿れない失敗のままにする。
  - why: 走査の RootUnreadable は権限がない失敗と同じ扱いで、root_dir がないとはしない
    （D48 の一覧に無い）。A1 の 2 つだけを root_dir がないとする。
  - decided_by: AI（走査の答えに揃える）

## Position

A1〜A3 は反映済み（`is_root_gone_error` と 3 か所の呼び出し、include の確認の kind の確認、
`an_include_check_defers_root_dirs_that_are_not_readable_directories` の 4 つの形のテスト）。
再確認（境界のレビューの round 3）で、include の有無で誤り・文言が同じになること、merge が
root_dir がない error と手を付けていない error で止まること、rollback が権限のときだけ
どのパスも "cannot resolve path: " の失敗になることを確かめた。エージェントの経路は
コードの読みのみ（契約テストの再現はしていない）。
