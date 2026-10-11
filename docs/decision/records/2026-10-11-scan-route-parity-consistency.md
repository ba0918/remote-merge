# 走査と子の一覧の経路差をそろえた単位で、残った古い前提を IR と実装にそろえる

## Context

走査と子の一覧の exclude と symlink の扱いを三つの経路でそろえる単位（計画 scan-route-parity）の変更を、[config/filters.md](../../ir/config/filters.md)、[scan/directory-links.md](../../ir/scan/directory-links.md)、[paths/operations.md](../../ir/paths/operations.md) の IR と突き合わせた。
変更したコードと IR の要件の食い違いはなかった。
一方で、[scan-route-parity A1](./2026-10-11-scan-route-parity.md#A1) で閉じた FLAG-paths-002 を前提にしたコードのコメントとテストが残っていた。また、エージェントの経路も辿るようになったことを、FLAGS の記述が反映していなかった。
この記録は、それぞれの扱いとその根拠を残す。

Position: A1・A2 を反映した。

## Agreements

- A1 接続断の後に権限を確かめる子の一覧（`crates/remote-merge/src/runtime/confirm.rs` の `observe_mode`）が exclude をどの経路でも当てないことは、IR に書かずにコードのコメントに残す。その理由を FLAG-paths-002 から「当てると、exclude に当たるディレクトリを作ったときにその権限を読めない」に書き直す。この扱いを確かめていた契約テストは、exclude で隠す前提を外し、[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) の「切れた権限の変更を変えたと確かめる」ことだけを確かめるものにする（名前を `a_cut_ssh_permission_change_of_a_created_directory_is_confirmed_as_changed` に変え、設定の exclude を外す）。
  - why: そのテスト（変更前の名前 `an_ssh_permission_change_is_confirmed_on_an_entry_the_exclude_patterns_would_hide`）は、SSH の子の一覧が "vendor/**" を深い階層の "a/vendor" にも当てること（FLAG-paths-002）を前提にしていた。この単位で SSH の一覧が root_dir からのパスに当てるようになったので、"vendor/**" はどの経路でも "a/vendor" を隠さない。`observe_mode` の SSH の経路に設定の exclude を当てる一時的な変更を入れて `cargo nextest run --all-features -E 'test(an_ssh_permission_change_is_confirmed_on_an_entry_the_exclude_patterns_would_hide)'` を実行すると、"vendor/**" のままでは通り（変更を見分けられない）、exclude を "a/vendor" にすると失敗した。ただし exclude "a/vendor" で "a/vendor/x.txt" を merge できるのは、パスを指定した merge が exclude を root_dir からのパスの子孫に当てないためで、[REQ-config-022](../../ir/config/filters.md#REQ-config-022) と [REQ-config-003](../../ir/config/filters.md#REQ-config-003) に反する。それを前提にしたテストは仕様が述べない規則を固定するので、証拠にならない。IR のとおりなら、merge が書き込むパスは exclude に当たらないので、exclude を当てないことは利用者から見えず、IR の外になる。
  - rejected: テストの exclude を "a/vendor" にして、隠す前提を保つ（REQ-config-022 に反する merge の振る舞いをテストの前提にすることになる）
  - decided_by: 整合の段（consistency phase）
- A2 [FLAG-scan-002](../../ir/scan/FLAGS.md) の「merge と sync は途中のディレクトリ symlink を辿る」経路に、エージェントの経路を加える。
  - why: この単位でエージェントの走査がディレクトリ symlink の配下を辿るようになった（[REQ-scan-011](../../ir/scan/directory-links.md#REQ-scan-011)、`crates/remote-merge-agent/src/agent/tree_scan.rs` の `process_entry`）。merge と sync のエージェントの経路のツリーの取得も、status と同じ `try_agent_fetch_tree_recursive`・`try_agent_fetch_tree_for_subpath`（`crates/remote-merge/src/runtime/remote_target.rs:174`・`:198`）を使う。FLAG の記述は「ローカルと SSH の経路では」のままで、経路の範囲が実装と合わなくなっていた。旧資料との食い違いという FLAG の中身は変わらないので、FLAG は閉じない。
  - decided_by: 整合の段（consistency phase）
