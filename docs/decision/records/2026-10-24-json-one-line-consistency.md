# --format json の一行の出力と仕様の突き合わせ

## Context

--format json の結果とエラーを、空白のない一行の JSON で出すようにした変更（7f8db2fe、e793736e）について、変わったコードと仕様を突き合わせた。読んだコードは、crates/remote-merge/src/service/output.rs の format_json と、それを呼ぶ status・diff・merge・sync・rollback の出力、crates/remote-merge/src/main.rs の handle_with_format、crates/remote-merge/src/cli/logs.rs の run_logs である。
コードは [REQ-cli-102](../../ir/cli/results.md#REQ-cli-102) と、[REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) に書き足した logs と引数の解析の文に合っていた。
一つだけ、--format json に --help を付けると使い方を標準出力に出して終了コード 0 で終わるのに、REQ-cli-018 は成功とエラーのどちらでも JSON を返すと書き、JSON を出さない場合に確認を断った取り消しと引数の解析で止まった場合しか挙げていなかった。[2026-10-24-json-one-line.md の A1](./2026-10-24-json-one-line.md#A1) は使い方を結果でもエラーでもないと読んだが、その条件を REQ-cli-102 の文にだけ書いていた。
この記録は、IR に何を足し何を足さなかったかと、その根拠を残す。

Position: REQ-cli-018 に --help の場合を書き足し、例 [EX-cli-125](../../ir/cli/results.md) と契約テストを足した（A1）。引数の解析で止まる場合の括弧の中は例として読み、書き足さなかった（A2）。

## Agreements

- A1 --help か -h を受けた引数の解析が使い方を標準出力に出すときは、--format json を付けていても結果にもエラーにも当たらず、JSON を出さずに終了コード 0 で終わる。これを [REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) に書き足し、パスのない merge に --help を付ける例 EX-cli-125 と、それを確かめる契約テストを crates/remote-merge/tests/contract/json_one_line.rs に足す。コードは変えない。
  - why: 今の製品で `status --format json -h`・`logs --format json --help`・`merge --format json --help`（パスなし）を動かすと、どれも標準出力に JSON でない使い方を出し、標準エラーに何も出さず、終了コード 0 で終わる（下の Verification）。REQ-cli-018 の字面では、この終了コード 0 の出力を成功と読むと JSON を返す約束に外れ、JSON を出さない場合としても挙がっていなかった。[2026-10-24-json-one-line.md の A1](./2026-10-24-json-one-line.md#A1) は、--help の使い方は結果でもエラーでもないと読んで REQ-cli-102 から外しており、同じ読みを REQ-cli-018 にも書くのが、仕様の中で食い違わない書き方である。サブコマンドなしの --help は [REQ-cli-076](../../ir/cli/safety.md#REQ-cli-076) が扱うが、サブコマンドに付けた --help はどの要件にもなかった。文を「--help を付けたとき」でなく「--help か -h を受けた引数の解析が使い方を出すとき」としたのは、--help より前の引数で解析が止まると使い方を出さず、引数の解析で止まった場合（終了コード 2）になるからである（`status --format json --bogus --help` は終了コード 2、`status --format json --help --bogus` は終了コード 0）。並びによってどちらになるかは JSON を出すかどうかに関わらないので、文には書かない。パスのない merge を例にしたのは、必須の引数がなくても --help なら使い方を出すことを、引数の解析で止まる [EX-cli-124](../../ir/cli/results.md) と並べて示せるからである。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 [REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) の「引数の解析で止まったとき（必須の引数がない、値の形が合わない）」の括弧の中は例として読み、ほかの止まり方を文に足さない。
  - why: 知らない引数（`status --format json --bogus`）、値を取る引数に値がない（`status --format json --left`）、一度だけの引数を二度指定する（`--format json --format json`）、文字コード名の引数に使えない名前を渡す（`diff a --format json --left-encoding bogus`）も、どれも標準出力に何も出さず、標準エラーに引数を読む部品の "error: ..." を出して終了コード 2 で終わった。どれも「引数の解析で止まったとき」に当たり、[2026-10-24-json-one-line.md の A11](./2026-10-24-json-one-line.md#A11) の根拠（引数を読む部品が止めるので、JSON でエラーを出す処理まで届かない）もそのまま当てはまる。手引き（skills/remote-merge/references/json-schemas.md）も "such as" で例として挙げている。括弧の中を数え上げに変えると、今の文が言っていない約束を足すことになるので、変えない。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 空の一時ディレクトリを HOME にし、設定ファイルを置かずに、この枝の製品（cargo build）を動かした。
  - `status --format json -h`、`logs --format json --help`、`merge --format json --help`: 終了コード 0、標準出力は使い方（1,367・816・2,004 バイト）、標準エラーは空。
  - `status --format json --help --bogus`・`logs --format json --help --tail abc`: 終了コード 0 で使い方を出す。`status --format json --bogus --help`・`logs --format json --tail abc --help`・`status --format json --left --help`: 終了コード 2、標準出力は空。
  - `merge --format json`、`logs --format json --tail abc`、`status --format json --bogus`、`status --format json --left`、`status --format json --format json`、`status --format json extra`、`diff a --format json --left-encoding bogus`、`merge a --format json --hunks x`、`status --format json --max-entries abc`: どれも終了コード 2、標準出力は空。
  - `status --format json`（設定ファイルがない）と `logs --format json --level bogus`: 終了コード 2、標準出力は {"error":"..."} の一行。設定ファイルがないときのエラーの文は探した二つのパスを改行で並べるが、その改行は文字列の中で "\n" と書かれ、出力は一行のままである。
- 足した契約テスト（json_one_line::ex_cli_125_…）は、コードを変えずに最初から通る。この記録は今の振る舞いを仕様に書くもので、振る舞いを変えないからである。
