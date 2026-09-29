# 走査の一覧に載るもの

status・diff・merge・sync のツリーの走査が、symlink と root_dir をどう一覧に載せるか。

## Requirements

### REQ-scan-006: symlink はそれ自体を一覧に載せる
- kind: ubiquitous
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A1
- verification: unit

ツリーの走査は、ローカル・SSH・エージェントのどの経路でも symlink 自体を一覧の一つの項目としてリンク先の文字列とともに載せ、リンク先がない symlink も載せる。

### REQ-scan-007: root_dir の symlink を辿る
- kind: state_driven
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A2
- verification: unit

root_dir 自体がディレクトリへの symlink のとき、status・diff・merge・sync のツリーの走査はリンク先の配下を列挙し、設定の root_dir の末尾の "/" の有無で一覧は変わらない。
