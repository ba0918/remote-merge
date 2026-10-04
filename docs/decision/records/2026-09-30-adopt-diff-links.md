# CLI diff の symlink とディレクトリを kotowari に取り込む際の判断

## Context

CLI diff の差分の出し方（[取り込みの記録](./2026-09-29-adopt-diff-output.md)）に続き、symlink とディレクトリ symlink の比較、root_dir の外を指すリンクの追跡、ディレクトリ指定の末尾の "/" を取り込む。
この範囲の IR は [決定記録 cli-diff-path-semantics](./2026-09-26-cli-diff-path-semantics.md) を出典に要件 7 件（[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020) から [REQ-cli-024](../../ir/cli/symlink-diff.md#REQ-cli-024)、[REQ-cli-025](../../ir/cli/directory-paths.md#REQ-cli-025)、[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026)）と例がそろい、例には全て印付きのテストがある。そのため新しい要件は作らず、既存要件と実装を突き合わせた。一覧を出す前に別の文脈で各要件・各例を実装と照らし合わせ、食い違うと読めた点は試験 SSH サーバに対する実行で確かめた。
既存要件は、REQ-cli-025 は実装と一致し、ほかの 6 件は食い違う点があるため変更せず FLAG を付ける。旧総合仕様のシンボリックリンクの扱いの節の「リンク先パスを比較。内容のdiffは行わない」は cli-diff-path-semantics の決定で置き換え済みのため FLAG にしない。--ref の参照先の扱いは diff の次の回、エージェントの経路で root_dir の外を読むときの切り替えは SSH とエージェントの話題、読めない通常のファイルを空として扱うことは既存の [FLAG-cli-030](../../ir/cli/FLAGS.md#FLAG-cli-030)、どこからも使われていない "src/diff/symlink.rs" は merge の片付けで扱う。
CLI の FLAGS.md は limits.lines の 200 行を超えたままになるが、[差分の出し方の記録](./2026-09-29-adopt-diff-output.md)と同じく、中身は CLI の一つの責務（コマンドの振る舞いの未決事項）に揃っているため分けない。
テストの内訳（対象 39 件）: 要件の根拠 33 件、FLAG の挙動のテスト 0 件、実装詳細をなぞるだけ 0 件、残す 6 件。

## Agreements

- A1 未決の FLAG として残す。[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026) は root_dir 内の symlink を通常どおり辿り root_dir の外へ出る参照先だけを --follow-external-links のときに辿るとするが、設定の root_dir が symlink を経由するとき（例 root_dir が実ディレクトリを指す symlink）、diff は root_dir の中の通常のファイルまで "content not compared (outside root_dir; use --follow-external-links)" のエラーにし、終了コード 2 を返す（diff の実行で確かめた）。修正は diff のこの回のテスト整理の後に行う。
  - why: 既存要件と実装が食い違い、よくある配置で diff が使えなくなるため、利用者は整理の直後に直すことを選んだ。直し方は直すときに決める。
  - decided_by: 利用者（推奨を採用）
- A2 未決の FLAG として残す。[REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022) は symlink の参照先がないか読めないとき空ファイルと同一扱いせずエラー終了するとするが、SSH の経路の側だけ参照先がない symlink は、エラーにならずその側を空として比べ、終了コード 1 を返す（diff の実行で確かめた。ローカルの側ならエラーになる）。
  - why: 既存要件と実装が経路によって食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 未決の FLAG として残す。[REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) と例 EX-cli-061 は、ディレクトリ symlink と通常ファイルの組でも読めるディレクトリの配下を入口からの子パスで示すとするが、ローカルの側が通常ファイルで SSH の側がディレクトリ symlink のとき、diff はリンクの配下の子ファイルを出さない（diff の実行で確かめた。逆の向きは出す）。
  - why: 既存要件と実装が向きによって食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 未決の FLAG として残す。[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020) はリンク文字列・種類・内容が全て同じなら差分なしとするが、機密ファイルを指す symlink は、--force なしで内容を隠すとき、左右のリンク文字列も参照先の内容も同じでも差分ありと数え、終了コード 1 を返す（diff の実行で確かめた）。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
  - superseded_by: [A2（機密ファイルの特別扱いをやめる）](./2026-10-04-drop-sensitive-file-handling.md#A2)
- A5 未決の FLAG として残す。[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020) と [REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) の文は経路を限らないが、その文書の導入は symlink を明示指定したときとし、パスを指定しない diff（root_dir 全体の走査）では、リンク文字列が同じ symlink を一覧に出さず参照先の内容も比べない（diff の実行で確かめた）。要件がパスを指定しない diff にも当たるかが読み分けられない。
  - why: 要件の読みが割れ、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A6 未決の FLAG として残す。[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020) は解決後の種類の差を示すとするが、diff の note は、相手側に項目がない片側だけの symlink でも "type mismatch: symlink vs file"、相手側が通常ファイルのディレクトリ symlink でも "type mismatch: symlink vs directory" と出し、相手側の種類を正しく示さない場合がある。これは実装を読んで分かったことで未実行。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A7 未決の FLAG として残す。[REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) は件数超過でも読めた差分を残し不完全な比較をエラーとして報告するとするが、diff がディレクトリの子を一覧するときは一つのディレクトリにつき 10,000 件で黙って打ち切り、不完全な比較として報告しない。これは実装を読んで分かったことで未実行。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 未決の FLAG として残す。[REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022) は比較できなかったパスと理由を示すとするが、symlink ではない通常のディレクトリの展開で循環や件数超過が起きたとき、テキスト出力には理由が出ない（JSON では errors に出る）。symlink の項目では note として理由が出ることは diff の実行で確かめたが、通常のディレクトリの場合は実装を読んで分かったことで未実行。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。[REQ-cli-023](../../ir/cli/symlink-diff.md#REQ-cli-023) は入口名、入れ子の各段階のリンク文字列、最終参照先のパスのいずれかが機密パターンに当たるとき内容を表示しないとするが、実装はリンクの連鎖を root_dir の中でだけ辿り、途中で root_dir の外へ出るとその先の段のリンク文字列を調べない。これは実装を読んで分かったことで未実行。ディレクトリ symlink の名前が機密パターンに当たるときは、その項目の内容を隠し配下を展開しないことを diff の実行で確かめた。
  - why: 既存要件と実装が食い違う恐れがあり、機密の内容の表示に関わるため、次にこの機能を扱うときに実行で確かめて決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。[REQ-cli-024](../../ir/cli/symlink-diff.md#REQ-cli-024) はリンク文字列と参照先の内容差を区別して示すとするが、機密として隠したとき、root_dir の外で内容を比べなかったとき、参照先のディレクトリが読めないとき、循環したときにも hunks にリンク文字列の削除と追加の行が残り、テキストではその前に "Resolved content differs" と出る。これは実装を読んで分かったことで未実行。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A11 未決の FLAG として残す。[REQ-cli-054](../../ir/cli/diff-output.md#REQ-cli-054) は --max-lines で打ち切ったファイルの truncated を true にするとするが、symlink の参照先の内容の差分は、打ち切っても truncated を true にせず、テキストにも "... (output truncated)" を出さない。これは実装を読んで分かったことで未実行。
  - why: 既存要件と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A12 未決の FLAG として残す。diff は、リンク文字列も参照先の内容も同じ symlink を差分のない項目として files に残してテキストにも "Link target:" を出し、100MB を超える参照先を --force を付けても読めないエラーにし、絶対パスの指定を拒否しない。いずれも旧資料に記述がなく、確かめるテストもない。これは実装を読んで分かったことで未実行。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A13 未決の FLAG として残す。利用者向けの手引き "skills/remote-merge/references/json-schemas.md" の Symlink の例は symlink の項目にリンク先を "left_symlink_target" と "right_symlink_target" で出すとするが、[REQ-cli-024](../../ir/cli/symlink-diff.md#REQ-cli-024) と実装は link_targets の left と right で出す。
  - why: 旧資料と実装が食い違い、手引きを直すかどうかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
