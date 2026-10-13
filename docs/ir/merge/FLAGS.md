# Flags

## Flags

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

### FLAG-merge-021: dd が nofollow を受け付けないリモートの書き込みの競合
- kind: gap
- related: REQ-merge-038, REQ-paths-013
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A11, docs/decision/records/2026-10-12-foundation-3.md#D12, docs/decision/records/2026-10-12-foundation-3.md#D37, docs/decision/records/2026-10-12-foundation-3.md#D44, docs/decision/records/2026-10-12-foundation.md#A11

エージェントを使わないリモートで dd が "oflag=nofollow" を受け付けないときだけ、通常ファイルの書き込みは、書く直前に書き込み先が symlink でないことを確かめてから書く。その確認と書き込みの間に書き込み先が symlink に替わると、リンク先へ書く競合が残る。受け付けない dd があることは "docs/remote-tools.md" に書いた道具の限界で、ほかの経路は書き込み先を開くときに symlink を辿らない。
エージェントを使わないリモートで dd が "conv=excl" を受け付けない（BusyBox など）ときだけ、新しいファイルを作る書き込み（REQ-merge-043）は、シェルの noclobber の `>` で開く。noclobber は、すでにある通常ファイルでないもの（デバイスや FIFO）を指す symlink を辿って開くので、書き込み先がないと確かめた後に、そのような symlink が置かれると、開いた後にパスが symlink かを確かめて書かずに失敗するまでの間に、リンク先を開く（FIFO なら開くところで待つ）競合が残る。書くのはログインした利用者の権限で、中身は書かない。"conv=excl" を受け付ける dd では、dd が O_EXCL|O_NOFOLLOW で開くので、この競合はない。
どちらの競合も、確かめと書き込みの間に誰かが書き込み先を symlink にすり替えたときにだけ起き、並行するふつうの作業では起きないので、security の境界（出典の foundation A11）の外として直す対象にしない。
