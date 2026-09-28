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
