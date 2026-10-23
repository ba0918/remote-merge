# --format json の一行の出力と仕様の突き合わせ（二回目）

## Context

前の突き合わせ（[2026-10-24-json-one-line-consistency.md の A1](./2026-10-24-json-one-line-consistency.md#A1)）は、[REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) に --help か -h を受けて使い方を出す場合を、JSON を出さない場合として書き足した。その書き足しと契約テストを読み直し、前の記録の測定をこの枝の製品で再現した（下の Verification）。
もう一つ、サブコマンドより前に --version か -V を置くと、--format json を付けていても版の一行を標準出力に出して終了コード 0 で終わるのに、REQ-cli-018 はこの場合を JSON を出さない場合に挙げていなかった。[REQ-cli-076](../../ir/cli/safety.md#REQ-cli-076) も --version を扱うが、サブコマンドを指定しない起動に限る。
この記録は、REQ-cli-018 に何を足したかと、その根拠を残す。

Position: REQ-cli-018 の --help の文に --version の場合を足し、例 [EX-cli-126](../../ir/cli/results.md) と契約テストを足した（A1）。前の記録の A1 は根拠ごと再現でき、変えない。

## Agreements

- A1 --version か -V を受けた引数の解析が版を標準出力に出すとき（サブコマンドより前に置いたとき）も、--format json を付けていても結果にもエラーにも当たらず、JSON を出さずに終了コード 0 で終わる。これを [REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) の --help の文に書き足し、設定ファイルのないまま `--version status --format json` を動かす例 EX-cli-126 と、それを確かめる契約テストを crates/remote-merge/tests/contract/json_one_line.rs に足す。コードは変えない。
  - why: 今の製品で `--version status --format json`・`-V status --format json`・`-V logs --format json`・`--version merge --format json`（パスなし）を動かすと、どれも標準出力に JSON でない版の一行を出し、標準エラーに何も出さず、終了コード 0 で終わる（下の Verification）。REQ-cli-018 の字面では、この終了コード 0 の出力を成功と読むと JSON を返す約束に外れ、JSON を出さない場合としても挙がっていなかった。REQ-cli-076 の「--help と --version はこの扱いから除き」はサブコマンドを指定しない起動に限るので、サブコマンドを指定した起動の --version はどの要件にもなかった。[前の記録の A1](./2026-10-24-json-one-line-consistency.md#A1) が --help の使い方を結果でもエラーでもないと読んだのと同じ読みで、版も結果でもエラーでもない。文を「--version を付けたとき」でなく「--version か -V を受けた引数の解析が版を出すとき」としたのは、版を出すのはサブコマンドより前に置いたときだけで、サブコマンドの後に置くと知らない引数として引数の解析で止まり、終了コード 2 の場合に当たるからである（`status --format json --version` は終了コード 2）。例で設定ファイルを置かないのは、設定を読むより前に引数の解析が版を出し、設定ファイルがないときに `status --format json` が出すエラーの JSON も出ないことを示せるからである。
  - rejected: [REQ-cli-076](../../ir/cli/safety.md#REQ-cli-076) の側に、サブコマンドを指定したときの --version を書く（REQ-cli-076 はサブコマンドなしの起動の要件で、字面が合わなかったのは JSON を返すと約束する REQ-cli-018 の側だから）。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Rejected

- R1 サブコマンドを指定したときの --version を、REQ-cli-076 の側に書く。
  - why: REQ-cli-076 はサブコマンドなしの起動の要件で、字面が合わなかったのは JSON を返すと約束する REQ-cli-018 の側だから。

## Verification

- 空の一時ディレクトリを HOME にし、設定ファイルを置かずに、この枝の製品（cargo build）を動かした。
  - `--version status --format json`・`-V status --format json`・`-V logs --format json`・`--version merge --format json`・`--config x --version status --format json`・`--version status --format json --bogus`: 終了コード 0、標準出力は版の一行（34 バイト、JSON でない）、標準エラーは空。
  - `status --format json --version`・`status --format json -V`・`status --format json --bogus --version`: 終了コード 2、標準出力は空、標準エラーに引数を読む部品の "error: ..."。
  - `status --format json`（設定ファイルがない）: 終了コード 2、標準出力は {"error":"..."} の一行。
- 前の記録の測定を同じ条件で再現した。`status --format json -h`・`logs --format json --help`・`merge --format json --help` は終了コード 0、標準出力は使い方（1,367・816・2,004 バイト）、標準エラーは空。`status --format json --bogus --help` と `logs --format json --tail abc --help` は終了コード 2 で標準出力は空、`status --format json --help --bogus` と `logs --format json --help --tail abc` は終了コード 0 で使い方を出す。サブコマンドより前の `--help status --format json`・`-h status --format json` も終了コード 0 で全体の使い方（1,195 バイト）を出し、REQ-cli-018 の --help の文に当たる。前の記録が足した契約テスト（json_one_line::ex_cli_125_…）は通る。
- 足した契約テスト（json_one_line::ex_cli_126_…）は、コードを変えずに最初から通る。この記録は今の振る舞いを仕様に書くもので、振る舞いを変えないからである。
