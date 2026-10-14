# merge の変更のまとまりを選ぶマージを kotowari に取り込む際の判断

## Context

merge を 4 回に分けて取り込むうちの最後の回で、`--hunks` で選んだ変更のまとまりだけを書き込むマージを扱う。
要件 [REQ-merge-009](../../ir/merge/hunks.md#REQ-merge-009) と [REQ-merge-010](../../ir/merge/hunks.md#REQ-merge-010) は旧仕様の移行時に作られ、根拠のテストは TUI の操作にあり、CLI の `--hunks` との突き合わせが済んでいなかった。
旧資料（[旧総合仕様](../../archive/spec.md) の行単位マージとマージコンフリクト検知の節、[旧個別仕様 symlink-merge](../../archive/spec/symlink-merge.md) の 2 章、利用者向けの手引き "skills/remote-merge/SKILL.md" の hunk merge の節、既存要件）と、入口 `merge <PATH> --hunks <番号>`（--dry-run、--force、--ref、--format を含む）から見える現行実装・テストを突き合わせ、一致するものは現状を仕様として追認し、食い違い・欠落・曖昧さは FLAG として未決のまま残す。
REQ-merge-009 は実装と一致したため変更しない。REQ-merge-010 は書き換えず、CLI との食い違いを FLAG に残す。TUI の hunk 操作は凍結中のため扱わない。
要件は 6 件で limits.requirements を超えないため、既存の文書に足す。
テストの内訳（対象 40 件）: 要件の根拠 30 件、FLAG の挙動のテスト 0 件、実装詳細をなぞるだけ 0 件、残す 10 件。

## Agreements

- A1 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は --hunks にパスをちょうど一つ求めて --delete とは併用できないとし、hunk マージは symlink を拒否するとし、実装は --hunks の merge を次の指定でエラーで止めて終了コード 2 を返す: パスが一つでないとき "--hunks requires exactly one path (got N)"、--delete と併せたとき "--hunks and --delete cannot be used together"、番号が hunk の数以上のとき "Hunk index N is out of range (total hunks: M)"、読み込み元か書き込み先が symlink のとき "Hunk merge is not supported for symlink files: 'パス'"、どちらかがバイナリのとき "Hunk merge is not supported for binary files: 'パス'"、機密ファイルで --force がないとき "Sensitive file 'パス' requires --force for hunk merge"。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
  - superseded_by: [merge で書き込み先の変更を黙って失う二つの問題を直す判断の A6](./2026-09-28-merge-ref-hunks-fix.md#A6)
- A2 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は --hunks の JSON に hunks_applied・hunks_total・direction を出すとし、実装は merged の一件に、書き込んだとき status "merged"、--dry-run では status "would merge"、hunks_applied に指定した番号、hunks_total に hunk の数、direction に "left_to_right"、バックアップが有効なときだけ backup を出す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 現行実装をレビューなしで仕様とする。実装は --hunks の merge のテキスト出力で、書き込んだファイルを "Merged: パス (hunks: 番号,番号/総数)"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run では "Would merge: パス (hunks: 番号,番号/総数)" の行で出す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 現行実装をレビューなしで仕様とする。実装は --ref があり --force のない --hunks の merge で、参照先に対して左右が異なる変更をした競合のあるファイルを書き込まずに "three-way conflict: パス" のエラーで止め、--dry-run でも同じように止める。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A5 未決の FLAG として残す。利用者向けの手引き "skills/remote-merge/SKILL.md" は diff --format json で hunk の番号を調べて --hunks に渡す手順を示すが、実装の diff の JSON の hunk は前後 3 行の文脈でまとめた表示用の区切りで、--hunks は文脈 0 行で変更ごとに分けた操作用の区切りを数えるため、6 行以内に近い二つの変更があると番号がずれ、選んだものと違う変更を書き込みうる。これは実装を読んで分かったことで、実行しての確認はしていない。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
  - superseded_by: [merge で書き込み先の変更を黙って失う二つの問題を直す判断の A3](./2026-09-28-merge-ref-hunks-fix.md#A3)
- A6 未決の FLAG として残す。REQ-merge-010 は hunk マージが衝突の有無と書き込みの確認を経ずに書き込み先を変更しないとするが、CLI の --hunks の merge は確認を出さずに書き込み、衝突を確かめるのは --ref があり --force がないときだけである。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
  - superseded_by: [A15（hunk マージの確認の要件を除く）](./2026-10-04-tui-disposition.md#A15)
- A7 未決の FLAG として残す。利用者向けの手引き "skills/remote-merge/SKILL.md" は書き込む前に更新日時で楽観的ロックを確かめるとし、REQ-merge-011 は差分確認からマージまでに書き込み先が変わったら書き込みを止めるとするが、実装の --hunks の merge は書き込む直前の確認をせず、実行時に読んだ書き込み先との差分に番号を当てて書き込む。
  - why: 旧資料・既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 未決の FLAG として残す。merge の --max-entries のヘルプは設定の上限を上書きするとし、REQ-scan-003 は利用者が件数の上限を変更できるとするが、実装の --hunks の merge は --max-entries を使わず設定の上限で走査する。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。差分のないファイルを --hunks で指定すると、実装は JSON で status "skipped (no changes)" を出して終了コード 0 を返すが、テキストでは書き込んだときと同じ "Merged: パス" の行を出す。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。利用者向けの手引き "skills/remote-merge/SKILL.md" の hunk merge の節は機密ファイルを自動でスキップし --force で含めるとするが、実装の --hunks の merge は機密ファイルをスキップせずエラーで止める。この文が hunk merge を指すのか merge 全体を指すのかが読み分けられない。
  - why: 読んだ後も旧資料と実装が一致するか食い違うかの確信が持てず、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
