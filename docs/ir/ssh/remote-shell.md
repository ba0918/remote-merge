# リモートのコマンドの実行

エージェントを置いて起動するためにリモートで実行するコマンドを、リモートのログインシェルが Bourne 系のシェルか fish なら、その種類によらず同じに動かす規則。

## Requirements

### REQ-ssh-020: Bourne 系と fish のログインシェルからリモートのコマンドを sh で実行する
- kind: ubiquitous
- source: docs/decision/records/2026-10-05-ssh-strict-exec.md#A5, docs/decision/records/2026-10-05-ssh-strict-exec.md#A13, docs/decision/records/2026-10-12-foundation.md#A12
- verification: unit

エージェントを置いて起動するために SSH で接続したリモートで実行するコマンドは、リモートのログインシェルが Bourne 系のシェル（POSIX の sh の文法でコマンドを解釈するシェルで、sh・bash・dash がこれにあたる）か fish なら、ログインシェルが sh のときと同じ結果になる。
csh と tcsh のログインシェルでの動作は保証しない。
リモートで実行するすべてのコマンドは、実行するスクリプトを一つの引数にした "sh -c" として送り、その引数は単一引用符で囲み、単一引用符とバックスラッシュは引用符の外に出して前にバックスラッシュを置く。

## Examples

```gherkin
@id=EX-ssh-033 @about=REQ-ssh-020 @source=docs/decision/records/2026-10-05-ssh-strict-exec.md#A5
Scenario: ログインシェルが fish のサーバへのマージ
Given リモートのログインシェルが fish である
When エージェントを置いて起動し、書き込み・symlink の作成・削除を含むマージを実行する
Then ログインシェルが sh のときと同じくすべて成功する
```
