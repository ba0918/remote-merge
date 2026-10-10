# Flags

## Flags

### FLAG-merge-002: リモートの読み込み元の権限の複製
- kind: contradiction
- related: REQ-merge-014, REQ-merge-012
- source: docs/decision/records/2026-09-28-adopt-merge-write.md#A6

REQ-merge-014 は --with-permissions を指定したとき読み込み元のファイル権限を書き込み先に反映するとするが、実装は読み込み元がローカルのときだけ権限を写し、読み込み元がリモートのときは何も知らせずに写さず、新しく作るファイルには設定された権限も付けない。

### FLAG-merge-007: 読み込み元の権限が 0 のときの複製
- kind: gap
- related: REQ-merge-014
- source: docs/decision/records/2026-09-28-merge-write-mutant-flags.md#A1

--with-permissions の merge で読み込み元がローカルのとき、実装は読み込み元の権限の値が 0（mode 000）なら書き込み先の権限を変えずに残す。REQ-merge-014 は読み込み元のファイル権限を書き込み先に反映するとし、字のとおり読むと 0 も反映することになるが、IR はこの場合を決めていない。読み込み元は中身を読んだ後に権限を読むため、この違いは読み込みと権限の読み取りの間に読み込み元の権限が 0 に変わったときにだけ起きる。

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

### FLAG-merge-020: REQ-merge-041 の確かめ方の理由に出典がない
- kind: gap
- related: REQ-merge-041
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A14

REQ-merge-041 の how_to_verify は、契約テストを作らない理由（比べた後に書き込み先が消える競合でだけ起きる）と、SSH のテストが生成した削除のコマンドを sh で実行することを書いているが、引いている決定（2026-10-05-ssh-strict-exec の A4・A9・A14）はどちらも書いていない。2026-10-11 の agent-operations の突き合わせで 3 回目まで残った。エージェントの経路も削除を行うようになる（REQ-ssh-010）ので、確かめ方をエージェントの削除のテストと合わせて書き直すときに、理由の出典を決める。
