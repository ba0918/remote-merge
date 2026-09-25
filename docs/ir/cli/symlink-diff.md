# symlink のリンク文字列と参照先内容の差分

CLI diff に symlink を明示指定したとき、リンク自体とその参照先を区別して比較する。

## Requirements

### REQ-cli-020: symlink のパスと参照先を一貫して比較する
- kind: state_driven
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A1, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A16, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A17, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A28, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A29, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
- verification: unit

CLI diff はファイル・ディレクトリ、左右の種類や存在の組み合わせによらず、symlink 自体のリンク文字列を示して参照先を辿り、解決後の種類と内容の差を示す。リンク文字列・種類・内容のいずれかが異なれば差分あり、すべて同じなら差分なしとする。片側だけに項目がある場合も読める内容を片側の差として示し、参照先がバイナリなら通常のバイナリ diff と同じ SHA-256 ハッシュを用いる。

### REQ-cli-021: ディレクトリ symlink の配下を比較する
- kind: state_driven
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A7, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A11, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A18, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A30, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
- verification: unit

ディレクトリへの symlink を含む比較は左右の種類や存在の組み合わせによらず、読めるディレクトリの配下を入口からの子パスで表示する。入れ子の symlink も同じ規則で扱い、指定した比較全体で循環を検出して既存の最大走査件数を一回だけ適用する。循環または上限超過でも読めた差分は残し、不完全な比較をエラーとして報告する。

### REQ-cli-022: 参照先を読めない場合も比較できた範囲を示す
- kind: event_driven
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A4, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A12, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A30, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A33
- verification: unit

symlink の参照先がないか読めないとき、片側の項目自体がない場合と区別し、リンク文字列と取得できた子の差分を表示する。範囲外の追跡が無効、循環・件数超過・読取失敗で比較できなかった入口側のパスと理由を示してエラー終了（終了コード2）にする。JSON では読めた差分を files に残し、失敗をトップレベルの errors の path と reason で示す。読めない参照先を空ファイルと同一扱いしない。

### REQ-cli-023: symlink 経由の機密内容を隠す
- kind: prohibition
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A5, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A10, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A22
- verification: unit

入口名、入れ子になった各段階のリンク文字列、または最終参照先のパスが機密パターンに該当するファイルは、root_dir 内外を問わず通常の CLI diff で内容をテキスト・JSON・バイナリハッシュに表示しない。明示的に --force を指定したときだけ内容差を表示する。

### REQ-cli-024: リンクの差を JSON とテキストに分けて示す
- kind: state_driven
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A9, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A15, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A27
- verification: unit

CLI diff のテキストはリンク文字列と参照先の種類・内容差を区別して示す。JSON の symlink 項目は従来の symlink フラグを保ち、link_targets に left と right のリンク文字列を持つ。通常ファイル側や項目がない側は null とし、内容差は hunks またはバイナリのハッシュ欄で示す。ディレクトリ symlink 自体は files の独立した項目に置き、子ファイルは入口からの相対パスを持つ別の項目にする。子が同じでも親のリンク文字列が違えば差分件数と終了状態は差分ありとする。比較できない箇所はトップレベルの errors に path と reason を載せる。

### REQ-cli-026: 範囲外の symlink 追跡を利用者が選ぶ
- kind: state_driven
- source: docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A32, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A33, docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A34
- verification: unit

CLI diff は root_dir 内の symlink を通常どおり辿り、root_dir の外へ出る参照先と入れ子リンクは --follow-external-links の指定時だけ辿る。指定がない場合はリンク文字列と内容未検証の理由を示してエラー終了する。利用者が指定した入力パスの親ディレクトリへの遡りは禁止したまま、リンクの解決による移動だけを許す。--follow-external-links は status・merge・sync には適用せず、機密内容を表示する --force と独立させる。

## Examples

```gherkin
@id=EX-cli-039 @about=REQ-cli-020,REQ-cli-024 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A1,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A15
Scenario: リンク先のパスとテキスト内容が異なる
Given 左右のファイル symlink は異なるリンク文字列を持ち参照先の内容も異なる
When CLI diff でそのリンクを指定する
Then パス差と参照先のテキスト差が別々に表示され JSON でも区別される

@id=EX-cli-040 @about=REQ-cli-020 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A1
Scenario: 同じリンク文字列の先で内容だけが異なる
Given 左右の symlink は同じリンク文字列を持ち参照先の内容が異なる
When CLI diff でそのリンクを指定する
Then 内容差が表示され差分ありと報告される

@id=EX-cli-053 @about=REQ-cli-020 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A29
Scenario: リンク文字列も参照先内容も同じ
Given 左右の symlink は同じリンク文字列を持ち参照先の内容も同じである
When CLI diff でそのリンクを指定する
Then 差分なしと報告され終了コード0になる

@id=EX-cli-041 @about=REQ-cli-020 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A14,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A17
Scenario: ファイル symlink と通常ファイルは同じ内容を持つ
Given 左はファイル symlink で右は内容が同じ通常ファイルである
When CLI diff で両側を比較する
Then 種類とリンク文字列が示され内容は一致しても差分ありと報告される

@id=EX-cli-042 @about=REQ-cli-020,REQ-cli-026 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A16,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A32
Scenario: root_dir の外にあるバイナリへのリンク
Given 左右の symlink は root_dir の外の異なるバイナリファイルを指す
When --follow-external-links を付けて CLI diff でそのリンクを指定する
Then リンク文字列と参照先の SHA-256 ハッシュ差が別々に示される

@id=EX-cli-054 @about=REQ-cli-020 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A28,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
Scenario: 片側だけにファイル symlink がある
Given 左側だけに読める参照先を持つファイル symlink がある
When CLI diff でそのリンクを指定する
Then リンク文字列と片側だけの内容差が表示され取得エラーにはならない

@id=EX-cli-043 @about=REQ-cli-021,REQ-cli-024 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A7,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A15
Scenario: 別の共有ディレクトリを指すリンク
Given 左右のディレクトリ symlink は別の共有ディレクトリを指し子ファイルの内容も異なる
When CLI diff でそのリンクを指定する
Then リンク文字列の差と入口からの子パスごとの内容差が示される

@id=EX-cli-044 @about=REQ-cli-021 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A11,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A18
Scenario: 入れ子のディレクトリリンクで循環する
Given 配下の symlink が既に辿った祖先を指す
When CLI diff でディレクトリリンクを指定する
Then 読めた範囲の差分と循環の理由が報告され不完全な比較は成功にならない

@id=EX-cli-045 @about=REQ-cli-021 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A11,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A18
Scenario: 入れ子のリンクを合わせると件数上限を超える
Given 各リンクの配下は上限内だが指定した比較全体の件数は上限を超える
When CLI diff でディレクトリリンクを指定する
Then 読めた範囲の差分と件数超過が報告され不完全な比較は成功にならない

@id=EX-cli-055 @about=REQ-cli-021,REQ-cli-023,REQ-cli-026 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A22,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A32
Scenario: 外部ディレクトリの中からさらに外部の機密ファイルを指す
Given 普通のディレクトリリンクの先に別のリンクがあり最終参照先が機密ファイルである
When --follow-external-links を付けて --force なしで CLI diff を実行する
Then 入れ子のリンクも解決され最終参照先の内容とハッシュは表示されない

@id=EX-cli-046 @about=REQ-cli-020,REQ-cli-021 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
Scenario: ディレクトリ symlink と通常ディレクトリが異なる
Given 一方がディレクトリ symlink でもう一方は通常ディレクトリである
When CLI diff でそのパスを指定する
Then 種類とリンク文字列の差と読める子ファイルの差が示される

@id=EX-cli-061 @about=REQ-cli-020,REQ-cli-021 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
Scenario: ディレクトリ symlink と通常ファイルが異なる
Given 左は子ファイルを持つディレクトリ symlink で右は通常ファイルである
When CLI diff でそのパスを指定する
Then 種類とリンク文字列の差に加えて左の子ファイルと右の本文をそれぞれ比較結果として示す

@id=EX-cli-056 @about=REQ-cli-020,REQ-cli-021 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A31
Scenario: ディレクトリ symlink が片側だけにある
Given 一方だけに参照先を読めるディレクトリ symlink があり他方には項目がない
When CLI diff でそのリンクを指定する
Then 片側のリンク文字列と子ファイルの内容が差分として示される

@id=EX-cli-057 @about=REQ-cli-021,REQ-cli-024 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A27
Scenario: ディレクトリリンク先は違うが子の内容は同じ
Given 左右のディレクトリ symlink のリンク文字列が異なり子ファイルの内容は同じである
When JSON 形式の CLI diff でそのリンクを指定する
Then 親リンクは files の独立した差分項目となり差分件数と終了状態に反映される

@id=EX-cli-047 @about=REQ-cli-022,REQ-cli-024 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A12,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A20
Scenario: 配下の一件が読めない
Given ディレクトリ symlink の配下には読める子と読めない子がある
When JSON 形式の CLI diff で指定する
Then 読めた差分は files に残り読めない子のパスと理由は errors に載りエラー終了する

@id=EX-cli-048 @about=REQ-cli-022 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A4
Scenario: ファイルリンクの参照先が存在しない
Given symlink の参照先が存在しない
When CLI diff でそのリンクを指定する
Then リンク文字列と内容取得失敗が報告され空ファイルとみなされずエラー終了する

@id=EX-cli-049 @about=REQ-cli-023 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A10
Scenario: 入口は普通の名前でも参照先が機密ファイルである
Given 通常名の symlink が機密パターンに一致する参照先を指す
When --force を指定せず CLI diff でそのリンクを指定する
Then 参照先の内容はテキストにも JSON にも含まれない

@id=EX-cli-050 @about=REQ-cli-023 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A5
Scenario: 機密参照先への表示を明示する
Given symlink の参照先が機密パターンに一致する
When --force を指定して CLI diff でそのリンクを指定する
Then 参照先の内容差が表示される

@id=EX-cli-058 @about=REQ-cli-022,REQ-cli-026 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A32,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A33
Scenario: 外部リンクの追跡を指定しない
Given root_dir 内の symlink が root_dir 外のファイルを指す
When --follow-external-links を付けずに CLI diff でそのリンクを指定する
Then リンク文字列と内容が未検証の理由が示されエラー終了する

@id=EX-cli-059 @about=REQ-cli-026 @source=docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A32,docs/decision/records/2026-09-26-cli-diff-path-semantics.md#A34
Scenario: 内容表示の許可とは独立して外部リンクを辿る
Given root_dir 外の通常ファイルを指す symlink がある
When --follow-external-links を指定して CLI diff を実行する
Then root_dir 外の内容は比較され --force は不要である
```
