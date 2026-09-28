# merge の symlink と削除を kotowari に取り込む際の判断

## Context

merge を 4 回に分けて取り込むうちの 3 回目で、書き込み先の symlink と種類の違い、引数によるパス脱出、`--delete` による削除を扱う。
これらの要件（[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001)〜[REQ-merge-004](../../ir/merge/symlink.md#REQ-merge-004)、[REQ-merge-007](../../ir/merge/symlink.md#REQ-merge-007)、[REQ-merge-008](../../ir/merge/deletion.md#REQ-merge-008)、[REQ-merge-016](../../ir/merge/deletion.md#REQ-merge-016)）は旧仕様の移行時に作られたが、実装・旧資料・テストとの突き合わせが済んでいなかった。
旧資料（[旧個別仕様 symlink-merge](../../archive/spec/symlink-merge.md) の 1〜3.5・5〜7 章、[旧総合仕様](../../archive/spec.md) のディレクトリマージ時の削除セマンティクスとシンボリックリンクの扱いの節と JSON 出力スキーマ、既存要件）と、入口 `merge` と `sync`（パス、--delete）から見える現行実装・テストを突き合わせ、一致するものは現状を仕様として追認し、食い違い・欠落は FLAG として未決のまま残す。
既存要件はいずれも実装と一致したため変更しない。REQ-merge-004 はローカルと SSH の経路で一致し、エージェントの経路の食い違いを FLAG に残す。root_dir 自体の symlink と途中のディレクトリ symlink の配下のツリー取得（symlink-merge 3.6・3.7）は scan、root_dir に ".." を含む相対パスで "Path not found" になる件は root_dir の解決として config の話題で扱う。
要件は各文書で 6 件のため、既存の文書に足す。
テストの内訳（対象 79 件）: 要件の根拠 36 件、FLAG の挙動のテスト 2 件、実装詳細をなぞるだけ 0 件、残す 41 件。

## Agreements

- A1 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は symlink を作るマージでローカルの symlink をそのまま複製し、マージではリンク先のパスを書き換えるとし、実装は merge と sync で、読み込み元が末尾の symlink で書き込み先にそのパスがないとき、書き込み先に同じリンク先の symlink を作り、リンクの先へは辿らない。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A2 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は書き込み先にだけあるファイルを今までどおりのスキップとして扱うとし、実装は --delete のない merge と sync で書き込み先にだけあるファイルを変更せず、skipped に reason "right-only file (use --delete to remove)" で出す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 現行実装をレビューなしで仕様とする。実装は --delete の merge と sync で、書き込み先にだけある機密ファイルを --force がなければ削除せず、skipped に reason "sensitive file (use --force to include)" で出す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は merge の JSON の deleted に path・status・backup を出し status を "ok" とするとし、実装は削除したファイルを deleted に path、status "ok"、バックアップが有効なときだけ backup（"セッションID/パス"）で出す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A5 現行実装をレビューなしで仕様とする。実装は merge のテキスト出力で削除したファイルを "Deleted: パス" の行で出し、バックアップがあれば続けて " (backup: バックアップ)" を出す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A6 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は種類の違いによるスキップの reason を "type mismatch: left is <kind>, right is <kind>" とするが、実装は "source and destination have different file types" とする。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A7 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は --delete の対象が symlink のときのスキップの reason を "symlink deletion not supported" とするが、実装は削除の計画のときに "destination is a symlink"、削除の直前に確かめたときに "destination is a symlink; deletion skipped" とする。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は --dry-run でも種類の違いによるスキップを同じ理由で出すとするが、実装は種類の違いを書き込むときにだけ判定するため、--dry-run の merge と sync はそのファイルを merged に status "would merge" で出す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.5](../../archive/spec/symlink-merge.md) は ".." の成分を含むか絶対パスの対象をそのファイルだけ failed の error "path traversal detected: <相対パス>" で出して他のファイルの処理を続けるとするが、実装の merge は ".." を含む引数でコマンド全体を "path traversal not allowed: パス"、絶対パスの引数で "absolute subpath not allowed: パス" のエラーで止める。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.4・3.5](../../archive/spec/symlink-merge.md) はエージェントが root_dir の外を指す途中の symlink 越しの書き込みと、".." や絶対パスをリンク先に持つ symlink の作成を拒否せず、SSH の経路に切り替わることもないとするが、実装のエージェントはどちらも拒否し、書き込みは SSH の経路に切り替えて行う。sudo を使う設定では SSH の経路への切り替えが止められるため、その書き込みは失敗するとコードからは読める（実行しての確認はしていない）。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A11 未決の FLAG として残す。--dry-run の merge --delete で書き込む予定がなく削除予定だけがあるとき、実装はテキストで削除予定を削除済みの形 "Deleted: パス" で出し、JSON でも deleted の status を実際に削除したときと同じ "ok" にするため、予定と実行済みを区別できない。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A12 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は種類を判定するためにパスを辿れなかった対象を skipped の reason "cannot resolve path: <原因>" で出すとするが、実装の --delete は削除の直前に書き込み先を調べられなかったファイルを failed の error "cannot inspect destination: 原因" で出す。削除そのものに失敗したときの failed の error "Delete failed: 原因" は記述もテストもない。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
