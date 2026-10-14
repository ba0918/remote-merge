# 走査と子の一覧の経路差をそろえた単位で、exclude で除いたディレクトリの下の失敗の扱いを IR に書く

## Context

走査と子の一覧の exclude と symlink の扱いを三つの経路でそろえる単位（計画 scan-route-parity）で、SSH の走査も exclude に当たったディレクトリの下で辿れないことを失敗にしないように直した。
この変更を [config/filters.md](../../ir/config/filters.md)、[scan/directory-links.md](../../ir/scan/directory-links.md)、[scan/limits.md](../../ir/scan/limits.md) の IR と突き合わせた。
三つの経路はどれも、除いたディレクトリの下の循環する symlink や読めないディレクトリで走査を止めないが、IR はそれを書いておらず、読めないディレクトリと循環の要件は文字どおりに読むとその場合も止めるように読めた。
また、SSH の経路は、名前に ASCII 以外の文字を含むパスでだけ、まだ走査を止めていた。
この記録は、IR に書いた振る舞い、残る差の置き場、直したコードの根拠を残す。

Position: A1〜A3 を反映した。

## Agreements

- A1 [REQ-config-021](../../ir/config/filters.md#REQ-config-021) と [REQ-config-022](../../ir/config/filters.md#REQ-config-022) に、ローカル・SSH・エージェントのどの経路の走査も、除外したディレクトリの下にある循環する symlink や読めないディレクトリでは走査を止めず、不完全な一覧とも報告しないことを書く。[REQ-scan-002](../../ir/scan/directory-links.md#REQ-scan-002) と [REQ-scan-014](../../ir/scan/limits.md#REQ-scan-014) には、除外したディレクトリの下はこの二つの要件による、と書き添える。
  - why: [REQ-config-003](../../ir/config/filters.md#REQ-config-003) は除外したパスを走査の対象から外し、REQ-config-021・022 は除外したディレクトリの下も全て除外するので、その下で辿れないことは「走査の途中」の出来事ではないと読むのが既存の IR と合う。一方で REQ-scan-014 の「走査の途中に読めないディレクトリがあるとき」と REQ-scan-002 の「循環が起きた場合」は文字どおりにはその場合も含むように読め、利用者に見える成否が IR から決まらなかった。三つの経路の振る舞いは測って確かめた。SSH の経路は契約テスト `a_cycle_below_a_directory_matched_by_a_path_pattern_is_not_walked_over_ssh` ほか 7 件（`crates/remote-merge/tests/contract/scan_routes_agree.rs`）が通る。ローカルとエージェントの経路は、同じ構成（循環は "a/b/back" から root_dir へ、読めないのは "a/b/locked"）を、exclude "a/b"・"b"・"a/?"・"**/b"・"[b]" のそれぞれで一時的なテスト `zz_probe_local_route`・`zz_probe_agent_route` に置き、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features -E 'test(zz_probe)' --no-capture` で測った（uid 1000。コミットしていない）。20 通りすべてが終了コード 0 で、一覧は "a/keep.txt" の equal だけだった（エージェントの経路は、テストの補助がエージェントの起動と "find -L" を使わなかったことを確かめている）。SSH の経路が受け流す標準エラーは、GNU find の決まった二つの文言で、パスが除くパスの下のものに限り、文言は debug のログに残す。結果は、その下へ降りないローカルとエージェントの経路と同じなので、エラーを握りつぶすことにはならない。
  - decided_by: 整合の段（consistency phase）
- A2 SSH の経路で、find のパターンに移せない exclude の下の失敗を GNU findutils 4.5.11 より前の find で見分けられず、走査を止める差は、[FLAG-config-016](../../ir/config/FLAGS.md) に残す。コードは直さない。
  - why: コンテナで、一般の利用者として、"a/b/locked"（読めない）と "a/b/sub/back"（root_dir への循環）を置いて `LC_ALL=C find -L /tmp/r -mindepth 1 ! -xtype l -printf ''` を動かした。centos:7 の findutils 4.5.11 は `find: '/tmp/r/a/b/locked': Permission denied` と `find: File system loop detected; '/tmp/r/a/b/sub/back' is part of the same file system loop as '/tmp/r'.` を出し、見分ける形と同じだった。centos:6 の 4.4.2 は引用を "`" で始め（`` find: `/tmp/r/a/b/locked': Permission denied ``）、centos:5 の 4.2.27 は引用せず（`find: /tmp/r/a/b/locked: Permission denied`）、循環を `find: Symbolic link `/tmp/r/a/b/sub/back' is part of a loop in the directory hierarchy; we have already visited the directory to which it points.` と出した。どちらも終了コードは 1 である。一方、`'(' -path /tmp/r/a/b -o -name b ')' -prune -o ! -xtype l -printf '%y %p\n'` は centos:5 と centos:6 の両方で終了コード 0 で、除いたディレクトリの下へ降りなかった。古い文言まで見分けるかは、この単位の範囲の外として後の単位に残してある。差の起きる範囲を IR から読めるよう、FLAG に置く。
  - decided_by: 整合の段（consistency phase）
- A3 SSH の経路が受け流す find の診断のパスは、GNU find が C ロケールで逃がした形（`\'`・`\\`・`\a`・`\b`・`\f`・`\n`・`\r`・`\t`・`\v`・3 桁の 8 進）を戻してから exclude に当てる（`crates/remote-merge-ssh/src/find_diagnostics.rs` の `quoted`）。戻したバイトが UTF-8 として読めないパスと、ほかの形の逃がしは見分けず、走査を止める。
  - why: GNU find 4.9.0 を `LC_ALL=C` で動かすと、"日本/b/locked" は `'.../j/\346\227\245\346\234\254/b/locked'`、"é1" は `\303\2511`、改行・タブ・ベル・CR・FF・VT・BS は `\n`・`\t`・`\a`・`\r`・`\f`・`\v`・`\b`、0x01 と 0x7f は `\001`・`\177` と出た（8 進はいつも 3 桁で、後ろの数字はそのまま続く）。centos:6 の 4.4.2 と centos:7 の 4.5.11 でも "é" は `\303\251`、改行は `\n` だった。戻す前のコードでは、契約テスト `an_unreadable_directory_with_a_non_ascii_name_below_a_directory_matched_by_a_glob_path_pattern_does_not_fail_the_ssh_scan`（exclude "a/?" と "**/b"、右の "a/b/日本" を読めなくする）が `SSH command execution failed: LC_ALL=C find -L ...` で失敗し、ローカルとエージェントの経路は A1 の測定のとおり一覧を返すので、[REQ-config-022](../../ir/config/filters.md#REQ-config-022) の「どの経路の走査も同じに除外する」に反していた。戻した後は `cargo nextest run --all-features` の全 2333 件が通った。
  - decided_by: 整合の段（consistency phase）
