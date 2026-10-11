# Flags

## Flags

### FLAG-paths-001: SSH の子の一覧が名前に TAB か改行を含む子を読み違える
- kind: gap
- related: REQ-paths-013, REQ-paths-017
- source: docs/decision/records/2026-10-11-agent-operations.md#A16

SSH の経路の子の一覧は、名前に TAB か改行を含む子を正しく読めない。エージェントの経路はこの影響を受けない。SSH の走査と一覧の出力の読み方をまとめて直す別の単位で扱う（品質の台帳に記録する）。

### FLAG-paths-003: 秒しか出さない find では SSH の子の一覧の更新時刻がナノ秒まで一致しない
- kind: gap
- related: REQ-paths-017
- source: docs/decision/records/2026-10-11-agent-operations.md#A36

SSH の経路の子の一覧は、リモートの find の %T@ が出した桁までの更新時刻を持つ。%T@ が秒しか出さない find（CentOS 5 の findutils 4.2 と見込む。未確認）のサーバで、ナノ秒を記録するファイルシステムを一覧したときは、ナノ秒の部分がエージェントとローカルの経路と一致しない。
