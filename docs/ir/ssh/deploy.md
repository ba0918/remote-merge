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
- source: docs/decision/records/2026-10-05-harden-security.md#A3, docs/decision/records/2026-10-05-harden-security.md#A10, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

Agent の配置先のディレクトリがないとき、所有者だけが読み書き・実行できるモード 700 のディレクトリとして作る。途中の親ディレクトリもないときは、それらもモード 700 で作る。

### REQ-ssh-016: 手元と違う配置済みのバイナリを実行しない
- kind: prohibition
- source: docs/decision/records/2026-10-05-harden-security.md#A8, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

ローカルの Agent のバイナリを解決でき、リモートで SHA-256 を計算できるとき、配置先のバイナリの版が一致しても SHA-256 がローカルのバイナリと一致しないなら、配置先のバイナリを Agent として実行せず、版が違うときと同じく置き直す。リモートで計算できないときは照合せずに実行し、そのことを利用者に警告する。

### REQ-ssh-017: 他人が差し替えられる場所の配置先の中のものを実行しない
- kind: prohibition
- source: docs/decision/records/2026-10-05-harden-security.md#A9, docs/decision/records/2026-10-05-harden-security.md#A7
- verification: unit

Agent の配置先のディレクトリの祖先に、書かれたパスか symlink を解決した実際のパスで、グループかその他が書き込めて sticky bit のないディレクトリがあるとき、版の確認を含めて配置先の中のものを何も実行せず、Agent を利用できないものとして扱う。

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

@id=EX-ssh-025 @about=REQ-ssh-016 @source=docs/decision/records/2026-10-05-harden-security.md#A8
Scenario: 版だけが一致するバイナリが置かれている
Given 安全な配置先に版の確認に正しく答えるがローカルの Agent と中身の違うバイナリがある
When sudo=false のサーバへマージする
Then 置かれたバイナリは Agent として実行されず SSH 経路で書き込み先が更新される

@id=EX-ssh-027 @about=REQ-ssh-017 @source=docs/decision/records/2026-10-05-harden-security.md#A9
Scenario: 誰でも書き込める親の下の配置先
Given Agent の配置先のディレクトリはモード 700 で版の確認に正しく答えるプログラムを含み、その親がモード 777 で sticky bit がない
When sudo=false のサーバへマージする
Then 配置先の中のものは実行されず SSH 経路で書き込み先が更新される
```
