# 設定のエラーに書かれた中身を出さないと決めた後の、IR とコードの突き合わせで決めたこと

## Context

[drop-compat A7](./2026-10-11-drop-compat.md#A7) で、設定ファイルを解釈できないときのエラーに設定ファイルの行と書かれた値を出さないと決め、実装した。
その実装で、このエラーは "Failed to parse config file: " のすぐ後に設定ファイルの絶対パスを出すようになった。
[REQ-config-009](../../ir/config/loading.md#REQ-config-009) は接頭辞に続けて理由を示すとだけ書いていて、パスを書いていなかった。
ここでは、試しの実行で確かめた今の出力をもとに、IR をどう合わせたかを決める。

Position: A1 で出揃った。

## Agreements

- A1 [REQ-config-009](../../ir/config/loading.md#REQ-config-009) のエラーは "Failed to parse config file: 絶対パス: " に続けて読めなかった理由を示すものとし、絶対パスは読めなかった設定ファイルのものとする。IR をこの形に書き直し、REQ-config-009 の三つの契約テストでパスを確かめる。コードは変えない。
  - why: A7 の実装（`crates/remote-merge-core/src/error.rs` の `ConfigParse` と `crates/remote-merge-config/src/lib.rs` の `load_raw_config`）で、エラーの文言が接頭辞・設定ファイルの絶対パス・位置と理由の順になった。この範囲より前は接頭辞のすぐ後が toml の理由だったので、利用者から見える違いである。[REQ-config-043](../../ir/config/loading.md#REQ-config-043) はパスを出してよいと書き、[REQ-config-007](../../ir/config/loading.md#REQ-config-007) と [REQ-config-040](../../ir/config/loading.md#REQ-config-040) も設定ファイルの絶対パスをエラーに出すので、IR と矛盾しない具体化である。試し: 作業ディレクトリの ".remote-merge.toml" の [servers] に書き間違いや型の違う値を書いて `remote-merge status --left local --right develop` を動かすと、どの場合も "Failed to parse config file: <作業ディレクトリの絶対パス>/.remote-merge.toml: line 4, column 76: unknown field `bogus`, ..." の形で止まった（終了コード 2、--format json の error も同じ文言）。同じ試しで、知らないキー、文字列・整数・浮動小数・日時・配列・表の型の違い、範囲外の整数、閉じていない文字列、不正なエスケープ、重複したキー、欠けたキーのどれでも、同じ行に書いたパスワードはエラーに出なかった。
  - decided_by: 一貫性の段（consistency phase）
