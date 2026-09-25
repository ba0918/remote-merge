# 古い SSH サーバへの接続

サーバごとのアルゴリズム指定と、接続に失敗したときの案内。

## Requirements

### REQ-ssh-006: サーバごとに接続方式を指定できる
- kind: state_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A36
- verification: unit

利用可能な SSH アルゴリズムをサーバごとに明示指定し、その設定を接続に適用できる。

### REQ-ssh-007: 接続失敗の理由を示す
- kind: event_driven
- source: docs/decision/records/2026-09-25-spec-migration.md#A36
- verification: unit

暗号方式が合わず SSH 接続に失敗したとき、理由と変更できる設定のヒントを表示する。

## Examples

```gherkin
@id=EX-ssh-011 @about=REQ-ssh-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A36
Scenario: 古いサーバだけに互換設定を与える
Given 二つの接続先のうち一方に対応する暗号方式が明示設定されている
When そのサーバに接続する
Then そのサーバに指定した設定が適用される

@id=EX-ssh-012 @about=REQ-ssh-006 @source=docs/decision/records/2026-09-25-spec-migration.md#A36
Scenario: 別のサーバには互換設定がない
Given 二つ目の接続先には暗号方式を明示設定していない
When 二つ目のサーバに接続する
Then 一つ目のサーバだけの設定は適用されない

@id=EX-ssh-013 @about=REQ-ssh-007 @source=docs/decision/records/2026-09-25-spec-migration.md#A36
Scenario: 鍵交換が合わない
Given 接続先と共通の鍵交換方式がない
When SSH 接続を試す
Then 不一致の理由と設定のヒントが表示される

```
