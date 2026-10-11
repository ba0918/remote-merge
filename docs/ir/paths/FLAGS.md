# Flags

## Flags

### FLAG-paths-003: 秒しか出さない find では SSH の子の一覧の更新時刻がナノ秒まで一致しない
- kind: gap
- related: REQ-paths-017
- source: docs/decision/records/2026-10-11-agent-operations.md#A36, docs/decision/records/2026-10-12-ssh-output-parsing-consistency-3.md#A1

SSH の経路の子の一覧は、リモートの find の %T@ が出した桁までの更新時刻を持つ。%T@ が秒しか出さない find（CentOS 5 の findutils 4.2.27。コンテナで確かめた）のサーバで、ナノ秒を記録するファイルシステムを一覧したときは、ナノ秒の部分がエージェントとローカルの経路と一致しない。
