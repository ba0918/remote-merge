# 走査と子の一覧の経路差をそろえた単位で、パスを指定した merge の exclude の確かめを測り直す

## Context

走査と子の一覧の exclude と symlink の扱いを三つの経路でそろえる単位（計画 scan-route-parity）の変更と、それに続けて IR と実装をそろえた変更（[scan-route-parity-consistency](./2026-10-11-scan-route-parity-consistency.md) の A1〜A3）を、IR と突き合わせ直した。
変更したコードと IR の要件の食い違いは新たに見つからなかった。
一方で、[scan-route-parity-consistency A3](./2026-10-11-scan-route-parity-consistency.md#A3) が FLAG-scan-007 を広げた根拠の確かめは、ファイルを元の側にだけ置いていたため、書き込みを決めたのは SSH の側の走査ではなくローカルの側の走査だった。
この記録は、確かめを三つの経路で測り直した結果と、それに合わせて FLAG の本文を改めたことを残す。

Position: A1 を反映した。

## Agreements

- A1 [FLAG-scan-007](../../ir/scan/FLAGS.md) の本文の確かめの記述を、「SSH の経路で確かめ、ローカルとエージェントは未実行」から、元の側の走査がローカル・SSH・エージェントのそれぞれの経路のときに確かめた、に改める。FLAG の中身（ファイルのパスを指定した merge と sync も、パスのパターンで除外したディレクトリの下のファイルを書き込む）と、A3 で FLAG を広げた判断は変えない。
  - why: A3 の確かめ（`a_cut_ssh_permission_change_of_a_created_directory_is_confirmed_as_changed` の元の形に exclude "a/vendor" を足したもの）は `fixture.put("a/vendor/x.txt", Some("new\n"), None)` で、ファイルはローカル（元の側）にだけあり、SSH の側の "a/vendor" の下は空だった。したがって書き込みを決めたのはローカルの経路の走査で、SSH の経路の走査が exclude をどう当てるかは観測していない。そこで、exclude "a/vendor" の設定で "a/vendor/x.txt" を元の側にだけ置き、`merge a/vendor/x.txt` を三通りに実行する使い捨てのテストを `crates/remote-merge/tests/contract/` に一時的に置き、`systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features -E 'test(zz_probe)' --no-capture` で測った（コミットしていない）。元の側がローカル（`--left local --right fixture`、エージェントは無効）、SSH（`--left fixture --right local`、エージェントは無効）、エージェント（同じ向きでエージェントを有効、受けた要求は ListTree・StatFiles・ReadFiles・Shutdown）のどれでも、終了コード 0 で先の側に "a/vendor/x.txt" ができた。三つの経路とも親ディレクトリからの相対パスに当てて除外しないので、経路差はなく、FLAG の食い違いは三つの経路で起きる。
  - decided_by: 整合の段（consistency phase）
