# 内容の行番号と元バイト範囲の一致

## Context

内容の解釈を共通化した変更について、実装と承認済みの仕様を突き合わせた。
diff の行番号は単独の CR でも進む一方、元バイトの行範囲は LF だけで進んでいた。
通常のテキストファイルに CR が入ると、表示した hunk の部分マージが範囲外参照で停止した。
既存の行分割を変えず、選択した変更を元バイトで適用できるようにする。

Position: 行範囲の修正と契約テストを実行し、全 2398 テスト、fmt、Clippy の成功を確認した。

## Agreements

- A1 元バイトとデコード後の行範囲は、既存の diff と同じく LF、CRLF、単独の CR を区切りとし、区切りのバイトをその行に含める。
  - why: [REQ-content-002](../../ir/content/interpretation.md#REQ-content-002) と [REQ-merge-046](../../ir/merge/hunks.md#REQ-merge-046) は解釈と選択箇所の元バイトを一致させる。契約テストに読み込み元 "new\rsame\r\nend\n"、書き込み先 "old\rsame\r\nend\n" を渡すと、修正前は core の content/mod.rs:78 で "index out of bounds: the len is 2 but the index is 2" となった。既存の diff の区切りを保って範囲を一致させる修正であり、仕様の意味は変更しない。
  - decided_by: consistency phase（実装と仕様の整合確認役）
- A2 デコード、差分の構築、選択バイトの連結は既存の実装を再利用し、元バイト範囲の区切り条件だけを修正する。
  - why: 再利用の層は、デコード＝既存 content、差分と選択＝既存 PairScript、元バイト範囲＝既存 line_ranges の最小修正とした。標準の str::lines は区切りを除去し単独の CR を分割しない。既存依存 similar 2.7.0 の文字列側は単独の CR を区切るが、バイト側の利用には追加 feature が要るため、既存のバイト走査に一条件を足す。全文の再エンコードや往復変換の検査は加えない。
  - decided_by: consistency phase（実装と仕様の整合確認役）

## Verification

- RED: `CARGO_BUILD_JOBS=2 cargo nextest run --all-features -p remote-merge --test contract req_merge_046_carriage_returns` は上記の範囲外参照で失敗した。
- GREEN: `CARGO_BUILD_JOBS=2 cargo nextest run --all-features` は全 2398 件成功した。追加した契約テストは UTF-8、EUC-JP、Shift_JIS の各指定で CR と CRLF の混在、および CR で終わる内容を扱う。
- `cargo fmt --all --check` と `CARGO_BUILD_JOBS=2 cargo clippy --all-targets --all-features -- -D warnings` は終了コード 0。
- REFACTOR: 変更は一つの区切り条件と理由のコメントだけで、デコードと選択の責務を変えていないため追加の整理はしない。
