# 読んでいる間に上限を超えて伸びたファイルの扱いと仕様の突き合わせ

## Context

中身を読む前に調べた大きさだけを上限に当て、その後は終わりまで読んでいた core の `machine_ops::read` を、上限の次の 1 バイトまで読んだら止め、読めた大きさをもう一度上限に当てるようにした変更（05a9d908）について、変わったコードと仕様を突き合わせた。
[REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079) は、読む前に大きさを調べて上限を超えるファイルを読まないことだけを述べ、調べた後に伸びたファイルをどう扱うかを述べていなかった。
一方で [2026-10-05-size-limit.md の A3](./2026-10-05-size-limit.md#A3) は、調べてから読むまでの間に大きくなった場合は防がないと決めていて、変更後のコードと逆だった。
この記録は、どちらに合わせたかと、その根拠を残す。

Position: 変更後のコードに合わせて REQ-cli-079 に一文を足し、A3 をこの記録の A1 で置き換えた（A1）。伸びたファイルの失敗が持つ大きさは IR に書かず、コードも変えない（A2）。

## Agreements

- A1 diff・merge・sync・rollback が中身をメモリに持って読むとき、読む前に調べた大きさが上限以下でも、読んでいる中身が 104,857,600 バイトを超えたときは、超えた時点で読むのをやめ、それまでに読んだ中身を使わずに上限を超える失敗にする。これを [REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079) に書き、[2026-10-05-size-limit.md の A3](./2026-10-05-size-limit.md#A3) を置き換える。コードは変えない。
  - why: A3 の理由は二つある。前半の「REQ-cli-079 は読む前に大きさで決めることを求めて」いることは、変更の後も満たされる。read は今も開く前に調べた大きさを check_size に当て、その時点で上限を超えるファイルは開かない（crates/remote-merge-core/src/machine_ops/mod.rs の 41-43 行）。前半は二つ目の判定を禁じておらず、A3 が二つ目の判定を置かないとした理由は後半の経路差である。また、A3 を含む計画を決めた [2026-10-05-size-limit.md](./2026-10-05-size-limit.md) の Context は、読んだ後のバイト数で上限を判定していたために「上限を超えるファイルも一度はメモリに載る」ことを、直す問題の一つに挙げていた。変更後の二つ目の判定は上限の次の 1 バイトで読むのをやめるので、その問題を戻さず、調べた後に伸びたファイルで残っていた同じ問題を閉じる。後半の「経路の中に二つ目の判定を置くと経路差がまた生まれる」は、次のとおり当たらなくなった。[foundation A12](./2026-10-12-foundation.md#A12) で経路はローカルとエージェントの二つになり、どちらも core の `machine_ops::read`（crates/remote-merge-core/src/machine_ops/mod.rs の 39-57 行）を同じ関数として呼ぶ（crates/remote-merge/src/runtime/local_ops.rs の read_each、crates/remote-merge-agent/src/agent/dispatch/reads.rs の read_one）。二つ目の判定も一つの純粋な関数 check_size（crates/remote-merge-core/src/op_failure/size_limit.rs）で決まるので経路差は生まれず、[REQ-paths-020](../../ir/paths/operations.md#REQ-paths-020) の「上限を超えるは、操作が答えた大きさと上限から決め、経路ごとに決めない」とも食い違わない。REQ-cli-079 が出典にする [foundation-3 の D7](./2026-10-12-foundation-3.md#D7) は「経路は大きさを答えて、上限を超えるものの中身を送らない」と決め、[foundation-4 の E5](./2026-10-12-foundation-4.md#E5) は上限を中身をメモリに持つことを防ぐためのものとしている。変更の前は、調べた後に伸びたファイルを上限を超えても全部読んでいた（下の Verification の測定）。エージェントの read_one は読めた中身をそのまま送るので、エージェントの経路ではその中身が送られていた。[foundation A11](./2026-10-12-foundation.md#A11) は、同時に動く別の作業（並行するデプロイ・ほかの利用者のふつうの編集）で起きる問題を直さないものではなく直す側に置いている。書き足され続けるログや、配置で大きいファイルに差し替わるファイルはこの作業に当たり、直さないものとされた確かめと変更の間のすり替えの攻撃には当たらない。利用者に見えて変わるのは、調べた後に上限を超えて伸びたファイルが、全部読まれる代わりに上限を超える失敗（[TBL-compare-004](../../ir/compare/reading.md#TBL-compare-004) の判定できない）になることだけで、IR の部分どうしは矛盾しない。REQ-cli-079 の「読む前にその大きさを調べ」「中身を読まずに上限を超える失敗」は読む前の判定としてそのまま正しいので書き換えず、一文を足した。[REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011) と TBL-compare-004 は上限の規則を REQ-cli-079 に委ねているので変えない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 調べた後に伸びたファイルの上限を超える失敗が持つ大きさは、ファイルの本当の大きさではなく、読むのをやめるまでに読めたバイト数（上限の次の 1 バイトまでなので、上限が 104,857,600 バイトなら 104,857,601）とする。これは IR に書かず、コードも変えない。
  - why: 失敗の文言（今は "file too large (N bytes, the limit is L bytes; use --force to read it)"）を IR は決めておらず、[REQ-compare-014](../../ir/compare/reading.md#REQ-compare-014) は原因の文言を失敗の種類から一か所で作るとだけ述べる。上限を超えるかどうかとその扱いは大きさの値によらない。本当の大きさを示すには読むのをやめた後にもう一度大きさを調べることになり、それも書き手と競うので正しい値になる保証がない。読めたバイト数は、上限を超えたことを示す大きさの下限として正しい。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- 変更前の測定（リポジトリに入れない）: `git diff 05a9d908 9bef81ac -- crates/remote-merge-core/src/machine_ops/mod.rs | git apply` で read だけを変更前に戻し、`cargo nextest run -p remote-merge-core --all-features -E 'test(/machine_ops::tests::content_that_outgrows/)'` を動かした。大きさを 0 と答えて中身を持つ "/proc/self/status" を上限 10 バイトで読んだ結果が `Ok(1505)`（1505 バイトを全部読んで成功）になり、テストは失敗した。戻した後（05a9d908）は、同じテストと machine_ops::tests::reading_over_the_limit_is_refused_by_the_answered_size が成功する。
- この突き合わせで IR に足した一文には例を足していない。このプロジェクトで kotowari の印を読むのは "crates/remote-merge/tests/contract/" などの契約テストだけで、変更が足したテストは core の単体テスト（印なし）である。調べた後に伸びるファイルを製品のコマンドの上で書き手と競わずに作る手段は、"/proc" のような大きさを 0 と答えるファイルを root_dir に置くことになり、契約テストの例として示す意味が薄い。
