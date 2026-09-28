# Flags

## Flags

### FLAG-merge-001: 中身まで同じファイルの報告
- kind: contradiction
- related: REQ-merge-005, REQ-cli-049
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A5

旧個別仕様 symlink-merge の 3.9 は中身まで同じファイルを明示した merge で書かずに skipped に reason "identical" を出すとするが、実装は skipped を空にし、テキストでは "no files to merge in the specified path(s)" を出す。

### FLAG-merge-002: リモートの読み込み元の権限の複製
- kind: contradiction
- related: REQ-merge-014, REQ-merge-012
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A6

REQ-merge-014 は --with-permissions を指定したとき読み込み元のファイル権限を書き込み先に反映するとするが、実装は読み込み元がローカルのときだけ権限を写し、読み込み元がリモートのときは何も知らせずに写さず、新しく作るファイルには設定された権限も付けない。

### FLAG-merge-003: 新規ファイルと権限の複製の優先
- kind: ambiguity
- related: REQ-merge-012, REQ-merge-014
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A7

--with-permissions を指定して新しいファイルを作るとき、実装は書き込み先に設定された権限ではなく読み込み元の権限を付ける。新規作成は設定された権限にするという REQ-merge-012 と、読み込み元の権限を反映するという REQ-merge-014 のどちらを優先するかを IR は決めていない。

### FLAG-merge-004: 権限の複製の失敗
- kind: gap
- related: REQ-merge-014
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A8

--with-permissions で書き込み先の権限を変えることに失敗したとき、実装はログに警告を残すだけで、そのファイルの結果を status "ok" のまま出す。旧資料に記述がなくテストもない。

### FLAG-merge-005: 読み比べない書き込み先の読み取り失敗
- kind: gap
- related: REQ-merge-017, REQ-merge-019
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A9

中身を読み比べないファイル（--checksum のないディレクトリ指定でサイズが違うものなど）の書き込み先を読めなかったとき、実装はそのファイルを書かずに failed に出すが、error は "read failed:" で始まらず読み取りのエラーそのものになる。旧資料は中身の読み比べでの読み取りの失敗だけを定めており、この場合の出し方は記述がなくテストもない。

### FLAG-merge-006: 更新の検知と中断の範囲
- kind: ambiguity
- related: REQ-merge-011
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A10

旧総合仕様の楽観的ロックの節は差分を取得した時点から書き込み先が変更されていたらマージを中断するとするが、CLI の実装はそのファイルだけを書かずに failed に "destination content changed since comparison: パス"（比較したときになかった書き込み先が現れたときは "destination appeared since comparison: パス"）で出し、他のファイルは続ける。中断が merge 全体を止める意味かが読み分けられない。

### FLAG-merge-007: 読み込み元の権限が 0 のときの複製
- kind: gap
- related: REQ-merge-014
- source: docs/decision/records/2026-09-28-merge-write-mutant-flags.md#A1

--with-permissions の merge で読み込み元がローカルのとき、実装は読み込み元の権限の値が 0（mode 000）なら書き込み先の権限を変えずに残す。REQ-merge-014 は読み込み元のファイル権限を書き込み先に反映するとし、字のとおり読むと 0 も反映することになるが、IR はこの場合を決めていない。読み込み元は中身を読んだ後に権限を読むため、この違いは読み込みと権限の読み取りの間に読み込み元の権限が 0 に変わったときにだけ起きる。

### FLAG-merge-008: 種類の違いによるスキップ理由
- kind: contradiction
- related: REQ-merge-001
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A6

旧個別仕様 symlink-merge の 3.3 は種類の違いによるスキップの reason を "type mismatch: left is <kind>, right is <kind>" とするが、実装は "source and destination have different file types" とする。

### FLAG-merge-009: 削除しない symlink のスキップ理由
- kind: contradiction
- related: REQ-merge-002
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A7

旧個別仕様 symlink-merge の 3.3 は --delete の対象が symlink のときのスキップの reason を "symlink deletion not supported" とするが、実装は削除の計画のときに "destination is a symlink"、削除の直前に確かめたときに "destination is a symlink; deletion skipped" とする。

### FLAG-merge-010: dry-run での種類の違い
- kind: contradiction
- related: REQ-merge-001, REQ-cli-004
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A8

旧個別仕様 symlink-merge の 3.3 は --dry-run でも種類の違いによるスキップを同じ理由で出すとするが、実装は種類の違いを書き込むときにだけ判定するため、--dry-run の merge と sync はそのファイルを merged に status "would merge" で出す。

### FLAG-merge-011: パス脱出の拒否の範囲と文言
- kind: contradiction
- related: REQ-merge-007
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A9

旧個別仕様 symlink-merge の 3.5 は ".." の成分を含むか絶対パスの対象をそのファイルだけ failed の error "path traversal detected: <相対パス>" で出して他のファイルの処理を続けるとするが、実装の merge は ".." を含む引数でコマンド全体を "path traversal not allowed: パス"、絶対パスの引数で "absolute subpath not allowed: パス" のエラーで止める。

### FLAG-merge-012: エージェントの経路の拒否と切り替え
- kind: contradiction
- related: REQ-merge-004
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A10

旧個別仕様 symlink-merge の 3.4・3.5 はエージェントが root_dir の外を指す途中の symlink 越しの書き込みと、".." や絶対パスをリンク先に持つ symlink の作成を拒否せず、SSH の経路に切り替わることもないとするが、実装のエージェントはどちらも拒否し、書き込みは SSH の経路に切り替えて行う。sudo を使う設定では SSH の経路への切り替えが止められるため、その書き込みは失敗するとコードからは読める（実行しての確認はしていない）。

### FLAG-merge-013: merge の dry-run の削除予定の表示
- kind: gap
- related: REQ-merge-026, REQ-merge-027, REQ-cli-004
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A11

--dry-run の merge --delete で書き込む予定がなく削除予定だけがあるとき、実装はテキストで削除予定を削除済みの形 "Deleted: パス" で出し、JSON でも deleted の status を実際に削除したときと同じ "ok" にするため、予定と実行済みを区別できない。旧資料に記述がなくテストもない。

### FLAG-merge-014: 削除の直前に調べられないときと削除の失敗
- kind: contradiction
- related: REQ-merge-008, REQ-merge-016
- source: docs/decision/records/2026-09-28-adopt-merge-links.md#A12

旧個別仕様 symlink-merge の 3.3 は種類を判定するためにパスを辿れなかった対象を skipped の reason "cannot resolve path: <原因>" で出すとするが、実装の --delete は削除の直前に書き込み先を調べられなかったファイルを failed の error "cannot inspect destination: 原因" で出す。削除そのものに失敗したときの failed の error "Delete failed: 原因" は記述もテストもない。
