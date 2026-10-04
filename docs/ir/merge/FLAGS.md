# Flags

## Flags

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

### FLAG-merge-007: 読み込み元の権限が 0 のときの複製
- kind: gap
- related: REQ-merge-014
- source: docs/decision/records/2026-09-28-merge-write-mutant-flags.md#A1

--with-permissions の merge で読み込み元がローカルのとき、実装は読み込み元の権限の値が 0（mode 000）なら書き込み先の権限を変えずに残す。REQ-merge-014 は読み込み元のファイル権限を書き込み先に反映するとし、字のとおり読むと 0 も反映することになるが、IR はこの場合を決めていない。読み込み元は中身を読んだ後に権限を読むため、この違いは読み込みと権限の読み取りの間に読み込み元の権限が 0 に変わったときにだけ起きる。

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

### FLAG-merge-016: CLI の --hunks の確認と衝突
- kind: contradiction
- related: REQ-merge-010, REQ-merge-031
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A6

REQ-merge-010 は hunk マージが衝突の有無と書き込みの確認を経ずに書き込み先を変更しないとするが、CLI の --hunks の merge は確認を出さずに書き込み、衝突を確かめるのは --ref があり --force がないときだけである。

### FLAG-merge-017: --hunks の書き込み直前の確認
- kind: contradiction
- related: REQ-merge-011
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A7

利用者向けの手引き "skills/remote-merge/SKILL.md" は書き込む前に更新日時で楽観的ロックを確かめるとし、REQ-merge-011 は差分確認からマージまでに書き込み先が変わったら書き込みを止めるとするが、実装の --hunks の merge は書き込む直前の確認をせず、実行時に読んだ書き込み先との差分に番号を当てて書き込む。

### FLAG-merge-019: 差分のないファイルの --hunks のテキスト
- kind: gap
- related: REQ-merge-030
- source: docs/decision/records/2026-09-28-adopt-merge-hunks.md#A9

差分のないファイルを --hunks で指定すると、実装は JSON で status "skipped (no changes)" を出して終了コード 0 を返すが、テキストでは書き込んだときと同じ "Merged: パス" の行を出す。旧資料に記述がなくテストもない。
