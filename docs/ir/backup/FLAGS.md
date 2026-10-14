# Flags

## Flags

### FLAG-backup-005: セッション ID の連番のあふれで panic する
- kind: gap
- related: REQ-backup-022, REQ-backup-023
- source: docs/decision/records/2026-09-27-backup-mutant-flags.md#A1

既存のセッション ID の N が u64 の最大値のとき、次の ID を作る処理が加算のあふれで panic する。集約先の予約ディレクトリを手で作らない限り起きないが、そのときの扱いは IR に定めがない。
