# ファイル全体の UTF-8 の正しさで分ける単位で IR と記録を実装にそろえる

## Context

テキストとバイナリの判定をファイル全体の UTF-8 の正しさで行う単位（計画 utf8-binary）の変更を、IR と決定記録に突き合わせ直した。
判定の規則（[REQ-cli-061](../../ir/cli/binary.md#REQ-cli-061)）と --hunks の merge の止め方（[REQ-merge-028](../../ir/merge/hunks.md#REQ-merge-028)）は実装と一致していた。
一方で、この単位が消した文字列の読み込みの入口を名指しする決定が、今の実装と合わなくなっていた。
この記録は、それぞれをどう直したかと、その根拠を残す。

Position: A1 を反映した。

## Agreements

- A1 ファイルの大きさの上限の判定は、製品のすべての中身の読み込みが通る `CoreRuntime` の二つの読み込みの入口（read_file_bytes・read_files_bytes_batch）に置く。[2026-10-05-size-limit.md の A1](./2026-10-05-size-limit.md#A1) の、入口に一つだけ置き、経路に振り分ける前に大きさだけを尋ねて上限を超えるパスを外すという決定は変えない。
  - why: size-limit の A1 は入口を四つ（read_file・read_files_batch・read_file_bytes・read_files_bytes_batch）と名指ししていたが、この単位で製品のどこからも呼ばれていなかった文字列の読み込みの二つを消した（`crates/remote-merge/src/runtime/side_io.rs` の「読み込み」の節）。`rg -n "fn read_file\b|fn read_files_batch\b" crates --type rust` は何も返さず、残る入口は二つで、どちらも経路に振り分ける前に `refuse_over_limit`・`paths_within_limit` で上限を当てている。上限の振る舞い（[REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079)）は入口の名前を書いておらず、IR は変わらない。
  - decided_by: 整合の段（consistency phase）
