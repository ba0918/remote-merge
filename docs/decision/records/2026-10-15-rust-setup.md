# Rust の版・ツールチェーン・lint の決定

## Context

Rust のルール（ba0918-rust）を AGENTS.md の routing に足したうえで、このリポジトリの Rust の設定を照らした。
edition は全 crate が 2021 のままで、MSRV・ツールチェーンの固定・マニフェストの lint 設定がなく、clippy はコマンド行の -D warnings だけで動いていた。
2026-10-15 の実測（依存の要求・lint ごとの指摘数）に沿って、今の変更で有効にするものと、直してから広げるものに分ける。

Position: 版・ツールチェーン・ゲートと A5 は反映済み。A6（unwrap_used）と A7 の needless_pass_by_value も反映済み。残りは A7 の wildcard_enum_match_arm と ssh の print。

## Agreements

- A1 edition を 2024 にする（1.85 で安定した最新の安定版）。workspace の [workspace.package] で一度だけ宣言して各 crate が継承し、resolver は 2024 の既定である 3 にする。移行は cargo fix --edition で行い、整形は style edition 2024 に従う。移行ツールが drop の順序が変わると警告した 12 箇所は、ロックとチャネルの寿命が交差しないことを確認した。
  - why: ルールは新規・既存の crate に最新の安定版 edition を求める。2024 では let-chains が使え、unsafe の扱いが edition で明示される。
  - decided_by: AI（2026-10-15 の利用者の指示「edition 2024 を推奨」による）
- A2 ツールチェーンを rust-toolchain.toml で 1.99.0（2026-10-01 の最新安定版）に固定し、rustfmt と clippy を含める。CI は "stable" ではなく固定した版を入れ、ゲートは --locked で解決結果を固定する。
  - why: 固定しないと手元と CI が別の版で回り、版を上げたときに新しい lint の指摘がすり抜ける。
  - decided_by: AI（ba0918-rust の規則）
- A3 MSRV を 1.88 として workspace に宣言し、CI に 1.88.0 で check するジョブを置く。開発依存（テスト）は約束に含めない（--all-targets を付けない）。
  - why: 1.85（edition 2024 の最小）では依存の home 0.5.12 が rustc 1.88 を要求して check が通らず、1.88.0 で通ることを確かめた。
  - decided_by: AI（実測による）
- A4 ゲートを CI と pre-push でそろえる。clippy は --workspace --all-targets --all-features --locked、テストは --workspace --all-features --locked。
  - why: リポジトリ全体とロックファイルの一致を、コマンド行の指定ではなくゲートで保証する。
  - decided_by: AI（ba0918-rust の規則）
- A5 lint 設定を [workspace.lints] に置いて各 crate が継承する。有効にしたのは、unsafe_code = deny・unsafe_op_in_unsafe_fn = deny と、clippy の allow_attributes_without_reason・undocumented_unsafe_blocks・todo・unimplemented・dbg_macro。unsafe を使う 3 つのモジュールには理由つきの expect を置き、unsafe を使わない crate（protocol・settings・config・engine・製品の lib と bin）は forbid にした。出力の lint は ssh を除く 6 つのライブラリの直下で deny にし、テストが出す注意書きは理由つきの expect にした。clippy::allow_attributes は有効にしない。
  - why: ルールは「expect で抑制して理由を書く」「unsafe は名指しにしたモジュールだけに許す」「ライブラリは標準出力・標準エラーに書かない」を求める。todo 系は 0 件で、既存の allow と unsafe の指摘はすべて同じ変更で直すか理由を添えた。allow_attributes を有効にしないのは、known_hosts_guard の 3 つの条件つきの allow が、機能の有無で「使われない」状態が変わり、expect にすると使われるビルドで未達になるため。理由つきの allow のままにして review で見る。
  - decided_by: AI（ba0918-rust の規則と実測）
- A6 unwrap_used の採用は、次のコードの掃除で crate ごとに行う。今回は有効にしない。
  - why: 非テスト文脈の指摘が 1113 件ある（unwrap() の総数は 4187）。clippy の allow-unwrap-in-tests はこの構成（_tests.rs のモジュールと cfg(test) の下）をテストと見ず、統合テストは crate 直下の属性の外にある。crate ごとに、テストのモジュールへ理由つきの expect を置き、生産コードの unwrap を expect か別の形に直してから有効にする。
  - decided_by: AI（実測による）
- A7 clippy の needless_pass_by_value（58 件）と wildcard_enum_match_arm（133 件）、ssh ライブラリの対話の問いの print（9 件）も、直してから有効にする。
  - why: ルールは「指摘を直すか、理由つきの抑制を同じ変更で置く」を求める。数が多く、ssh の print は問いを呼び出し側へ渡す設計の変更になるため、コードの掃除の単位に分ける。
  - decided_by: AI（実測による）
