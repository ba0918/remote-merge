# リモートのコマンドの実行

SSH の経路がリモートで実行するコマンドを、リモートのログインシェルの種類によらず同じに動かす規則。

## Requirements

### REQ-ssh-020: ログインシェルによらずリモートのコマンドを sh で実行する
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A5
- verification: unit

SSH で接続したリモートでのコマンドの実行は、リモートのログインシェルが POSIX の sh でなくても（fish でも）、ログインシェルが sh のときと同じ結果になる。
リモートで実行するすべてのコマンドは、実行するスクリプトを一つの引数にした "sh -c" として送り、その引数は単一引用符で囲み、単一引用符とバックスラッシュは引用符の外に出して前にバックスラッシュを置く。

## Examples

```gherkin
@id=EX-ssh-033 @about=REQ-ssh-020 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A5
Scenario: ログインシェルが fish のサーバへのマージ
Given リモートのログインシェルが fish で、エージェントを使わない
When 書き込み・symlink の作成・削除を含むマージを実行する
Then ログインシェルが sh のときと同じくすべて成功する
```
