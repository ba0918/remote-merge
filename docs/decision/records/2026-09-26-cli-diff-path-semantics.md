# CLI diff の symlink とディレクトリ指定

## Context

隔離された SSH 環境で既存の diff テストを動かした結果、symlink を明示指定した場合とディレクトリの末尾スラッシュの有無で、期待と現行動作が食い違った。
現行の CLI diff はファイルを明示指定すると直接内容を読み、ツリーを持たないため symlink のリンク文字列を検出できない。
同じディレクトリでも末尾のスラッシュによりスキャン経路が変わり、差分が検出されない場合がある。
通常テストの移行時に期待を弱めず、不具合と並列実行時のログ検査の不安定さを分けて判断する。

## 現在の統一規則

CLI diff はリンク文字列と、リンクを辿って得られる種類・内容の両方を比較する。片側だけ、ファイルとディレクトリ、リンクと通常ファイルといった組み合わせでも、読める側を辿る（A31）。root_dir 内は既定で辿り、外へ出るときだけ --follow-external-links が必要になる（A32–A34）。--force は機密内容の表示にのみ使う（A5、A22）。比較できなかった箇所は差分なしとせずエラーにする（A4、A12、A30、A33）。以下の A13、A21、A25、A26 は会話中に撤回された案であり、現在の規則ではない。

## Agreements

- A1 CLI diff で両側が末尾 symlink の場合、リンク文字列の差と参照先ファイル内容の差を別々に表示し、どちらか一方でも異なれば差分として報告する。
  - why: パスの相違も内容の相違も利用者が比較の判断に必要とし、同じリンク文字列の先で内容だけが変化した場合も見逃さないため。
  - decided_by: user
- A2 既存のディレクトリを CLI diff に指定したとき、末尾のスラッシュの有無で配下の比較結果を変えない。
  - why: パスの書き方だけで差分を見逃さないため。
  - decided_by: user (took the recommendation)
- A3 A1 と A2 に対応する製品側の差分処理は、テスト環境移行と同じ専用ブランチで仕様とテストを先に追加したうえで、小さな独立したコミットとして修正する。
  - why: 現行動作に合わせてテストを弱めず、発見した製品の不具合を検知可能なまま直すため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A19](2026-09-26-cli-diff-path-semantics.md#A19)
- A4 symlink の参照先の内容が存在しないか読めない場合、取得できたリンク文字列は表示し、内容を取得できないことを明示して非ゼロのエラー終了にする。
  - why: 内容が空だったと誤認させず、比較を完了できなかったことを伝えるため。
  - decided_by: user (took the recommendation)
- A5 symlink の参照先が機密ファイルに当たる場合、通常の diff は参照先内容を隠し、明示的な --force のときだけ表示する。
  - why: symlink 経由で通常の機密ファイルの表示制限を回避させないため。
  - decided_by: user (took the recommendation)
- A6 通常の cargo test --all-features で並列実行時にだけ診断ログが捕捉されない問題は、原因を切り分けて検知力を維持した最小修正を同じブランチで行う。
  - why: 通常テストの成功が実行順に依存する状態を残さないため。
  - decided_by: user (took the recommendation)
- A7 ディレクトリを指す末尾 symlink も、リンク文字列と参照先ディレクトリの配下の内容差を比較する。
  - why: ディレクトリへのリンクでもパスの変更と配下の変更を別々に確認するため。
  - decided_by: user
- A8 root_dir の外を指す symlink も、参照先が読めるときはその内容まで比較する。
  - why: 比較対象に明示した symlink の参照先が root_dir の外でも内容の差を見られるようにするため。
  - decided_by: user
  - superseded_by: [A32](2026-09-26-cli-diff-path-semantics.md#A32)
- A9 CLI diff のテキストと JSON の双方で、symlink のリンク文字列の差と参照先内容の差を区別できるようにする。JSON は左右のリンク先パスを任意のフィールドに持ち、内容差は差分欄に載せる。
  - why: 人間の表示と機械からの利用の双方で、どちらが異なるか判別できるため。
  - decided_by: user (took the recommendation)
- A10 symlink の入口名か参照先パスのどちらかが機密パターンに該当する場合、root_dir 内外を問わず通常の diff では参照先内容を隠し、明示的な --force のときだけ表示する。
  - why: symlink を経由して機密ファイルの内容が表示される抜け道を作らないため。
  - decided_by: user (took the recommendation)
- A11 ディレクトリ symlink の再帰走査で循環や既存の最大件数の上限に達した場合は、不完全な差分を成功扱いにせず理由を示してエラー終了にする。root_dir 外でも同じ上限を適用する。
  - why: 一部しか走査していない結果を完全な比較として誤解させないため。
  - decided_by: user (took the recommendation)
- A12 ディレクトリ symlink の配下で一部のファイルが読めない場合、読めた子の差分と読めない子のパス・理由を示し、全体はエラー終了にする。
  - why: 取得できた結果を捨てず、比較できなかった範囲も見逃さないため。
  - decided_by: user (took the recommendation)
- A13 一方がディレクトリ symlink、もう一方が通常ディレクトリの場合、種類とリンク先パスの違いを示して配下の再帰比較はしない。
  - why: 種類の相違を子ファイルの結果で隠さないため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A31](2026-09-26-cli-diff-path-semantics.md#A31)
- A14 一方が通常ファイルへの symlink、もう一方が通常ファイルの場合は、種類とリンク先パスの相違に加えて両側のファイル内容も比較する。
  - why: 種類の違いだけでなく両側で実際に読める内容の差も判断できるため。
  - decided_by: user
- A15 JSON の symlink 項目は既存の symlink フラグを保ち、link_targets に左右のリンク文字列を持つ。リンクでない側の値は null とする。内容差は既存の hunks またはハッシュ欄を使い、ディレクトリの子は入口からの相対パスで示す。
  - why: 既存の出力と共存しながらリンク情報と子ファイルの差を区別して機械で処理できるため。
  - decided_by: user (took the recommendation)
- A16 symlink の参照先がバイナリファイルの場合、リンク文字列の差とは別に既存のバイナリ diff と同じ SHA-256 ハッシュで内容差を示す。
  - why: バイナリ本文をテキスト差分に変換せず内容の相違を識別するため。
  - decided_by: user (took the recommendation)
- A17 通常ファイルへの symlink と通常ファイルの内容が同じでも、ファイルの種類が違えば差分ありと報告する。
  - why: 内容の一致によってファイル構造の相違を隠さないため。
  - decided_by: user (took the recommendation)
- A18 ディレクトリ symlink の中に symlink が入れ子になっていても、循環検知と最大走査件数は指定した比較全体で共有する。
  - why: リンクごとに件数上限をリセットして大量走査を許さないため。
  - decided_by: user (took the recommendation)
- A19 symlink の再帰・root_dir 外の内容比較・JSON 拡張は製品仕様と計画を改訂してから同じ専用ブランチで機能ごとに分けて実装する。
  - why: 当初の小さな不具合修正を超えるため、テスト環境移行と区別した検証証拠を残すため。
  - decided_by: user (took the recommendation)
- A20 ディレクトリ symlink 配下の一部が読めないとき、JSON は読めた差分を files に残し、トップレベルの errors に入口からの子パスと理由を載せてエラー終了する。テキストも両方を表示する。
  - why: 部分的な結果を失わずに比較不能だった箇所を機械からも識別できるため。
  - decided_by: user (took the recommendation)
- A21 ディレクトリ symlink と通常ファイルまたはファイル symlink の比較は種類違いとリンク文字列だけ示し、参照先の内容は展開しない。
  - why: ディレクトリ対ファイルの種類違いを、片側の子ファイルの差で隠さないため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A31](2026-09-26-cli-diff-path-semantics.md#A31)
- A22 ディレクトリ symlink 配下の入れ子リンクを含め、入口・各段階のリンク文字列・最終参照先のいずれかが機密パターンに該当したら、--force なしでは内容をテキスト・JSON・バイナリハッシュに表示しない。
  - why: 途中の通常名のリンクを経由して機密ファイルの内容が漏れるのを防ぐため。
  - decided_by: user (took the recommendation)
- A23 利用者が CLI に指定した入力パスの親ディレクトリへの遡りは従来どおり拒否するが、明示指定された symlink の解決で生じる root_dir 外への移動とその先の入れ子リンクは、循環・走査件数上限・各段階の機密判定を適用して追跡する。
  - why: 不正な入力パスを許すことなく、利用者が比較に指定したリンクの参照先を一貫して辿るため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A32](2026-09-26-cli-diff-path-semantics.md#A32)
- A24 ディレクトリ symlink を指定するときは、末尾のスラッシュの有無によらずリンク文字列と配下の同じ子ファイルを比較する。
  - why: パスの書き方でリンク自体の差が消えないようにするため。
  - decided_by: user (took the recommendation)
- A25 ディレクトリ symlink が片側だけにある場合は、片側の種類とリンク文字列を差分として示し、配下の子ファイルは展開しない。項目の欠損と参照先の読取失敗は区別し、参照先が読めない場合は取得エラーとする。
  - why: 比較する相手のない配下の展開を避けつつ、壊れたリンクを単なる片側の欠損として扱わないため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A31](2026-09-26-cli-diff-path-semantics.md#A31)
- A26 両側が symlink でも一方の参照先がディレクトリでもう一方が通常ファイルなら、種類とリンク文字列の違いだけ示し、子や本文は展開しない。
  - why: 構造の異なる対象を同じ内容の差分として扱わないため。
  - decided_by: user (took the recommendation)
  - superseded_by: [A31](2026-09-26-cli-diff-path-semantics.md#A31)
- A27 ディレクトリ symlink 自体を JSON の files に独立した項目として示し、symlink フラグと link_targets を持たせ、子は別の項目とする。子が同じでも親のリンク文字列が違えば差分件数と終了状態は差分ありにする。
  - why: リンク先の差を子ファイルの差に紛れさせず、機械からも種類を判別できるため。
  - decided_by: user (took the recommendation)
- A28 通常ファイルへの symlink が片側だけにある場合は、リンク文字列と参照先の内容を片側だけの差分として表示する。参照先を読めない場合は取得エラーとする。
  - why: ファイルの片側差分と同様に存在する内容も判断できるようにするため。
  - decided_by: user
- A29 左右のリンク文字列と参照先内容がともに同じ場合は差分なしとして終了コード0を返す。
  - why: symlink という種類だけを差分ありと数えないため。
  - decided_by: user (took the recommendation)
- A30 ディレクトリ symlink の循環または件数上限で走査を止めた場合も、読めた子の差分を部分結果として残し、errors と非ゼロのエラー終了で不完全さを明示する。
  - why: 途中までの結果を利用できる一方、完了した比較だとは誤認させないため。
  - decided_by: user (took the recommendation)
- A31 CLI diff は左右のいずれであっても symlink のリンク文字列を示し、参照先が読める場合は解決後の種類と内容を比較する。ファイル・ディレクトリ、片側だけの存在、種類の違いによって参照先を辿る規則を変えない。
  - why: 利用者がどの形で symlink を指定しても、リンク自体と指し先の実体を一貫して確認できるため。
  - decided_by: user (took the recommendation)
- A32 CLI diff は root_dir 内の symlink を通常どおり辿り、root_dir の外へ出る解決だけは既定で行わず、--follow-external-links が明示されたときに追跡する。利用者が入力したパスの親ディレクトリへの遡りは従来どおり拒否し、機密内容の表示は独立した --force で制御する。
  - why: 通常の比較は統一しつつ、設定された比較範囲を越える読み取りだけ利用者の明示的な選択にするため。
  - decided_by: user (took the recommendation)
- A33 --follow-external-links がない状態で外部へ出る symlink に到達したら、リンク文字列は示すが内容は未検証と明示し、比較未完了のエラー終了（終了コード2）にする。
  - why: 内容が一致すると確認できない状態を差分なしの成功と混同しないため。
  - decided_by: user (took the recommendation)
- A34 --follow-external-links は CLI diff のみに適用し、status・merge・sync の外部 symlink の扱いは変更しない。
  - why: 比較表示の opt-in と書き込み操作の安全境界を混同しないため。
  - decided_by: user (took the recommendation)

## Revisions

- A3 の「小さな独立コミット」前提は、参照先ディレクトリ・root_dir 外・JSON 出力まで含む判断を受け、[A19](2026-09-26-cli-diff-path-semantics.md#A19) に改めた。
- A8 と A23 の root_dir 外を無条件に辿る前提は、[A32](2026-09-26-cli-diff-path-semantics.md#A32) の明示的な opt-in に改めた。
- A13、A21、A25、A26 の種類・片側の有無による例外は、[A31](2026-09-26-cli-diff-path-semantics.md#A31) の統一規則に改めた。
