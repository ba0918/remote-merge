# Agent の配置先の安全

リモートで Agent のバイナリを実行してよい配置先のディレクトリとバイナリの条件。

## Requirements

### REQ-ssh-014: 安全でない配置先の中のものを実行しない
- kind: prohibition
- source: docs/decision/records/2026-10-05-harden-security.md#A3, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

Agent の配置先のディレクトリが symlink であるとき、所有者が SSH のセッションの uid（sudo=true では root）でないとき、またはグループかその他が書き込めるとき、版の確認を含めてその中のものを何も実行せず、Agent を利用できないものとして扱う。

### REQ-ssh-015: 新しく作る配置先は所有者だけが使える
- kind: state_driven
- source: docs/decision/records/2026-10-05-harden-security.md#A3, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

Agent の配置先のディレクトリがないとき、所有者だけが読み書き・実行できるモード 700 のディレクトリとして作る。

### REQ-ssh-016: 手元と違う配置済みのバイナリを実行しない
- kind: prohibition
- source: docs/decision/records/2026-10-05-harden-security.md#A4, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

ローカルの Agent のバイナリを解決できるとき、配置先のバイナリの版が一致しても SHA-256 がローカルのバイナリと一致しないかリモートで計算できないなら、配置先のバイナリを Agent として実行せず、版が違うときと同じく置き直す。

## Examples

```gherkin
@id=EX-ssh-023 @about=REQ-ssh-014 @source=docs/decision/records/2026-10-05-harden-security.md#A3
Scenario: 誰でも書き込める配置先
Given Agent の配置先のディレクトリがモード 777 で版の確認に正しく答えるプログラムを含む
When sudo=false のサーバへマージする
Then 配置先の中のものは実行されず SSH 経路で書き込み先が更新される

@id=EX-ssh-024 @about=REQ-ssh-015 @source=docs/decision/records/2026-10-05-harden-security.md#A3
Scenario: 配置先を初めて作る
Given Agent の配置先のディレクトリがない
When Agent を使うサーバへ接続する
Then 配置先のディレクトリはモード 700 で作られる

@id=EX-ssh-025 @about=REQ-ssh-016 @source=docs/decision/records/2026-10-05-harden-security.md#A4
Scenario: 版だけが一致するバイナリが置かれている
Given 安全な配置先に版の確認に正しく答えるがローカルの Agent と中身の違うバイナリがある
When sudo=false のサーバへマージする
Then 置かれたバイナリは Agent として実行されず SSH 経路で書き込み先が更新される
```
