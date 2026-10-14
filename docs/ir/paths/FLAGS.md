# Flags

## Flags

### FLAG-paths-004: macOS のローカルでは OS のパスの長さの上限が先に当たる
- kind: gap
- related: REQ-paths-008, REQ-paths-003
- source: docs/decision/records/2026-10-06-path-resolution.md#A47, docs/decision/records/2026-10-12-foundation-3.md#D10

確かめる側が macOS のマシン（ローカル）のときは、OS の PATH_MAX が 1024 バイトなので、1024 バイト以上 4096 バイト未満の実パスで OS が先に失敗し、パスの確認は "cannot resolve path: cannot read" になる。Linux のリモートを確かめたときは、同じパスが REQ-paths-003 の上限まで辿れる。経路の違いではなく OS の限界なので、パスの確認が経路によらないこと（REQ-paths-008）の唯一の例外としてここに残す。
