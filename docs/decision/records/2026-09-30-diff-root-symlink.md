# CLI diff の root_dir の外かどうかの判定の基準の判断

## Context

CLI diff の symlink とディレクトリの取り込みで、設定の root_dir が symlink を経由するとき、diff が root_dir の中の通常のファイルまで "content not compared (outside root_dir; use --follow-external-links)" のエラーにして終了コード 2 を返すと確かめ、FLAG-cli-043 に残して、テスト整理の後に直すことにした（[A1](./2026-09-30-adopt-diff-links.md#A1)）。
実装は、ファイルの symlink を辿った後の実パスと、設定に書いた root_dir の文字列をそのまま前方一致で比べており、root_dir 自体を実パスに直していないことが原因と読める。
[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026) は root_dir 内の symlink を通常どおり辿り、root_dir の外へ出る参照先だけを --follow-external-links のときに辿るとしており、要件は変えずに判定の基準を決めて FLAG-cli-043 を外す。

## Agreements

- A1 diff がパスを root_dir の外かどうか判定するときは、root_dir をローカル・SSH・エージェントのどの経路でもファイルと同じ方法で実パスに直した場所を基準にし、設定に書いた root_dir の文字列と比べない。そのため、root_dir が symlink を経由していても、root_dir の中のファイルは root_dir の中として扱う。
  - why: ファイルの実パスと root_dir の書いたままの文字列を比べると、root_dir が symlink を経由するよくある配置で diff が使えなくなる。ファイルの実パスを調べる仕組みは三つの経路に共通のため、root_dir も同じ仕組みで直すと経路ごとの食い違いが出にくい。
  - rejected: ローカルの root_dir だけを実パスに直す案。リモートの経路で同じ食い違いが残る。
  - decided_by: 利用者（推奨を採用）
