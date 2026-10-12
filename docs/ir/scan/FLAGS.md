# Flags

## Flags

### FLAG-scan-015: 秒しか出さない find では SSH の走査の更新時刻がナノ秒まで一致しない
- kind: gap
- related: REQ-scan-016, REQ-scan-017
- source: docs/decision/records/2026-10-12-ssh-output-parsing-consistency-3.md#A1

SSH の経路の走査は、リモートの find の %T@ が出した桁までの更新時刻を持つ。CentOS 5 の findutils 4.2.27 の %T@ は秒しか出さない（1970 年より前の -1000000.25 秒も "-1000001" と出す。コンテナで確かめた）ため、このサーバでナノ秒を記録するファイルシステムを走査したときは、秒未満の部分がローカルとエージェントの経路と一致しない。子の一覧の同じ差は FLAG-paths-003 に残す。
