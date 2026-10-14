# sync を kotowari に取り込む際の判断

## Context

sync の IR は旧仕様の移行時に、複数の書き込み先の結果を区別する要件（[REQ-merge-015](../../ir/merge/multi-target.md#REQ-merge-015)）と、部分成功の終了コード（[REQ-cli-019](../../ir/cli/results.md#REQ-cli-019)）だけが作られ、書き込み先の指定・確認・書き込み先ごとの状態・出力と既存テストの大半が仕分けられていなかった。
旧資料（[旧総合仕様](../../archive/spec.md) のマルチサーバ同期の節・使い方の例・JSON 出力スキーマ、[旧個別仕様 symlink-merge](../../archive/spec/symlink-merge.md) の 3.3・3.11・3.12、既存要件）と、入口 `sync`（パス、--left、--right、--dry-run、--force、--delete、--with-permissions、--checksum、--format、--max-entries）から見える現行実装・テストを突き合わせ、一致するものは現状を仕様として追認し、食い違い・欠落・曖昧さは FLAG として未決のまま残す。
既存要件 REQ-merge-015、REQ-cli-018、REQ-cli-019 は sync について実装と一致したため変更しない。「merge と sync」を主語にした共通の要件（merge/ の比べ方・削除・symlink・読めないファイル・権限・並行更新・指定先だけの更新、cli/ の安全確認と参照先）は merge の取り込みで両方の入口を合わせて扱い、バックアップ・除外と対象のフィルター・走査件数の上限は他の話題の IR が扱うため、この記録では扱わない。書き込み元と先を明示しないと書き込まない規則は [REQ-cli-002](../../ir/cli/safety.md#REQ-cli-002) のままとし、A1 は sync の入口が受け付ける指定とエラーの中身だけを定める。--yes は SSH のホスト鍵の確認にだけ効くため SSH の話題とする。
要件は 8 件で limits.requirements を超えないため、1 文書にまとめる。
テストの内訳（対象 36 件）: 要件の根拠 22 件、FLAG の挙動のテスト 8 件、実装詳細をなぞるだけ 2 件、残す 4 件。

## Agreements

- A1 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は sync の --left に指定できるのは 1 サーバだけとし、実装は --left を一つ、--right を一つ以上受け付け、--left がないとき "--left is required for sync command"、--right がないとき "--right requires at least one target server for sync command"、--right に同じ名前が重なるとき "Duplicate target server: 名前"、--right の一つが --left と同じとき "--left and --right must be different (both resolved to '名前')"、設定にないサーバ名を指定したとき "Server '名前' not found in config" のエラーで止める。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A2 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は sync を server1 → server2 → server3 の順に逐次実行するとし、実装は --right に指定した順に書き込み先を一つずつ処理する。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は全サーバの差分のまとめを一覧表示した後に一括で確認し、--force で省略できるとし、実装は --force も --dry-run もなく書き込む予定があるときだけ、標準エラーに "Sync: 元 -> 先1, 先2" と書き込む予定のある書き込み先ごとの "[先] N files to merge, M files to delete" を出して "Proceed? [y/N] " と尋ね、"y" か "Y" なら書き込み、それ以外なら "Sync cancelled." を出して何も書かずに終了コード 0 で終わる。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は書き込み先ごとの status を "success"・"partial"・"failed" とし、読み取りに失敗したファイルがある書き込み先を "failed" か "partial" とし、実装は書き込めたファイルがあり失敗がなければ "success"、書き込めたファイルと失敗の両方があれば "partial"、書き込めたファイルがなく失敗があれば "failed"、どちらもなければ "success" とする。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A5 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は sync の JSON の summary に total_servers・successful_servers・total_files_merged・total_files_deleted・total_files_failed を出すとし、実装は書き込み先の数、status が "success" の書き込み先の数、全書き込み先の merged・deleted・failed の件数の合計をそれぞれに出す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A6 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は sync の JSON に left（label と root）、targets（target・merged・skipped・deleted・failed・status）、summary を出すとし、実装は同じ形で出し、deleted は空でも出し、status の値を小文字の "success"・"partial"・"failed" とする。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A7 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は失敗のある sync の全体の終了コードを 2 とし、実装はすべての書き込み先の status が "success" なら 0、一つでも "partial" か "failed" があれば 2、エラーで止まったときは 2 を返す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 現行実装をレビューなしで仕様とする。実装は --dry-run の sync で書き込む予定のファイルを書き込み先ごとの merged に status "would merge" で並べ、書き込み先を変更しない。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。[旧総合仕様のマルチサーバ同期の節](../../archive/spec.md) はパスを省いた "sync --left local --right server1 server2 server3" で全体を同期する例を示すが、実装はパスを一つ以上必須とし、全体は "." で指定する（同じ旧資料の使い方の例 "sync . --left local --right server1 server2" とは一致する）。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。--format json の sync で確認を断ると、実装は標準出力に何も出さずに終了コード 0 で終わる。JSON 指定なら成功とエラーのどちらでも JSON を返すという REQ-cli-018 に取り消しが含まれるかの読みが分かれる。
  - why: 読んだ後も既存要件と一致するか食い違うかの確信が持てず、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A11 未決の FLAG として残す。接続またはツリーの取得に失敗した書き込み先を、実装は --right の指定順によらず結果の末尾に並べる。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A12 未決の FLAG として残す。読み込み元への接続またはツリーの取得に失敗したとき、実装は書き込み先ごとの結果を出さずに sync 全体をエラーで止める。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A13 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある書き込み先の status を "partial" にし全体の終了コードを 2 にするとするが、実装は status の判定に skipped を数えないため、そのスキップだけの書き込み先は "success" になり終了コードは 0 になる。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A14 未決の FLAG として残す。実装は書き込み先の status の判定に削除の成否を数えないため、書き込んだファイルがなく削除だけがある書き込み先で一部の削除に失敗すると、削除できたファイルがあっても "partial" ではなく "failed" になる。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A15 未決の FLAG として残す。[旧総合仕様のマルチサーバ同期の節](../../archive/spec.md) は dry-run の出力を書き込み先ごとの "[server1] 3 files to merge (2 modified, 1 added)" と "M"・"+" の記号付きのファイルの行、最後の "Total: 7 merge operations across 3 servers" で示すが、実装のテキスト出力は "Sync: 元 → 先1, 先2" の見出し、書き込み先ごとの "[先] success" の行、"plan"・"ok"・"D"・"skip"・"FAILED" を付けたファイルの行、最後の "Summary: N/M servers successful, N files merged" の行を出し、書き込む予定がなく失敗もないときは先頭に "No files to sync." を出す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A16 未決の FLAG として残す。--dry-run の sync で書き込む予定がなく削除予定だけがある書き込み先を、実装はテキストで "(deleted)" と削除済みの形で出し、JSON でも deleted の status を実際に削除したときと同じ "ok" にするため、予定と実行済みを区別できない。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A17 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は --dry-run の終了コードを 0 とするが、実装は --dry-run の sync でも中身を読み比べるファイルを読めなかった書き込み先を "failed" にし、終了コード 2 を返す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）

## Rejected

- A18 TUI から sync を実行する機能は作らない。
  - why: 旧総合仕様は TUI モードでの sync を将来の拡張としていたが、TUI は凍結中で、利用者は作らないと答えた。
  - decided_by: user
