# 古い SSH サーバへの接続

サーバごとのアルゴリズム指定と、接続に失敗したときの案内、エージェントを使わずに扱うリモートに要る道具と、その道具がないときの扱い。

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

### REQ-ssh-025: リモートの道具の有無で扱いを決める
- kind: algorithm
- source: docs/decision/records/2026-10-12-foundation-3.md#D7, docs/decision/records/2026-10-12-foundation-3.md#D8
- verification: unit
- definition: TBL-ssh-002

### REQ-ssh-026: 要る道具がなければ推し量らずに止める
- kind: prohibition
- source: docs/decision/records/2026-10-12-foundation-3.md#D1, docs/decision/records/2026-10-12-foundation-3.md#D8
- verification: unit

TBL-ssh-002 で道具がないの失敗にする道具がリモートにないとき、その道具を要る操作は、道具の名前を含む誤り（paths/operations.md の REQ-paths-020 の道具がない）で失敗し、ほかの道具の答えから推し量った結果も、空の結果も、既定の値も返さない。
find が -printf か "-exec … {} +" を受け付けないときの誤りの文は、要る道具（-printf と "-exec … {} +" を受け付ける find）と、エージェントを使えばそのサーバを扱えることを含む。文言は IR で決めず、実装者が英語で書き、テストはその二つを含むことだけを確かめる。
走査が道具がないで失敗したときは、status・diff・merge・sync は、走査を止めるほかの誤りと同じく何も書き込まずに終了コード 2 で止まる。

## Decision tables

### TBL-ssh-002: エージェントを使わないときのリモートの道具
- source: docs/decision/records/2026-10-12-foundation-3.md#D2, docs/decision/records/2026-10-12-foundation-3.md#D5, docs/decision/records/2026-10-12-foundation-3.md#D7, docs/decision/records/2026-10-12-foundation-3.md#D8, docs/decision/records/2026-10-12-foundation-3.md#D33, docs/decision/records/2026-10-12-foundation-3.md#D35, docs/decision/records/2026-10-12-foundation-2.md#C14

| 道具 | 使う操作 | ないときの扱い |
|---|---|---|
| -printf と "-exec … {} +" を受け付ける find（GNU findutils 4.2.27 以降。BusyBox の find は当たらない） | 走査・子の一覧 | 道具がないの失敗 |
| readlink | パスの確認が symlink に出会ったとき | paths/routes.md の REQ-paths-009 |
| readlink | ハッシュ（symlink のリンク先を比べる値にする）と項目のリンク先の読み込みが symlink に出会ったとき | 道具がないの失敗 |
| ls の "-d" | パスの確認と操作で、ないか入れない理由を見分けるとき | 道具がないの失敗 |
| sha256sum、なければ "shasum -a 256" | ハッシュ | 中身を読む（reconnect.md の REQ-ssh-022） |
| "stat -L -c %s"、stat が "-c" を受け付けなければ "ls -ldn" | 大きさ | 道具がないの失敗 |
| cat | 中身の読み込み、読めないファイルのハッシュの理由、ハッシュを取る道具がないときの中身からのハッシュ、通常ファイルの中身の書き込み | 道具がないの失敗（書き込みは書き込み先を開かずに） |
| openssl の base64 の復号 | バイナリの中身の書き込み | merge/concurrency.md の REQ-merge-039 のとおり書き込み先を開かずに失敗 |
| dd の "oflag=nofollow" | 通常ファイルの中身の書き込み | 書く直前に symlink でないことを確かめてから書く（merge/FLAGS.md の FLAG-merge-021） |

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

@id=EX-ssh-057 @about=REQ-ssh-026,REQ-ssh-025 @source=docs/decision/records/2026-10-12-foundation-3.md#D8
Scenario: BusyBox の find だけのサーバをエージェントなしで比べる
Given サーバの find が -printf を受け付けない BusyBox の find で、そのサーバではエージェントを使わない
When status を実行する
Then 走査は、-printf と "-exec … {} +" を受け付ける find が要ることと、エージェントを使えば扱えることを示す誤りで止まり、空の一覧で比べず、終了コード 2 を返す

@id=EX-ssh-058 @about=REQ-ssh-025 @source=docs/decision/records/2026-10-12-foundation-3.md#D7
Scenario: sha256sum がなく shasum だけのサーバ
Given エージェントを使わないサーバに sha256sum がなく "shasum -a 256" がある
When そのサーバの "a.txt" のハッシュを取る
Then "shasum -a 256" で取った、ローカルで同じ中身から取ったものと同じ SHA-256 が返る
```
