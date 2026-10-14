# 残っていた FLAGS 10 件の裁定

## Context

土台の作り直し（段 1〜5）が終わった時点で、IR には矛盾・欠落・曖昧さを示す FLAG が 10 件残っていた。
kotowari の complete は FLAG が 0 のときだけ true になるため、仕様の正本を確定するには一件ずつの裁定が要る。
2026-10-15 に利用者は「一件ずつ裁定し、IR へ反映するか、直さないと明示して閉じる」方針を承認した。
行番号の基底（A4）・競合の JSON の欄（A5）・[local] の root_dir の既定値（A7）は利用者が選び、残りは提示した裁定案を利用者が承認した。

Position: 10 件すべて裁定済み。IR・コード・テストへの反映まで終えた。

## Agreements

- A1 FLAG-backup-005 を閉じる。同じ日時の ID の連番が u64 の最大値まで使われているときは、次の ID を決められないとしてバックアップの保存をエラーで止める（加算のあふれで panic しない）。
  - why: [REQ-backup-022](../../ir/backup/sessions.md#REQ-backup-022) は同じ日時に "-N" を付けて ID を分けるが、N が u64 の最大値のときは [REQ-backup-023](../../ir/backup/sessions.md#REQ-backup-023) の「重複しない」を保ったまま次を作る方法が存在しない。作れないことを利用者に見えるエラーにするのが、panic で落ちるより正しい。
  - rejected: 連番があふれたら別の日時の ID に付け替える（同じ秒の連番という形式が崩れ、ID の順序の規則と合わなくなる）。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A2 FLAG-cli-007 を閉じる。確認を断った取り消しは成功とエラーのどちらでもないので、[REQ-cli-018](../../ir/cli/results.md#REQ-cli-018) の対象は成功とエラーの場合に限る。sync の取り消しは標準出力に何も出さない（標準エラーの "Sync cancelled." と終了コード 0 は [REQ-cli-040](../../ir/cli/sync.md#REQ-cli-040) のまま）。
  - why: 取り消しは利用者が対話で選んだ結果であり、機械可読の結果を伴う処理は起きていない。空の標準出力と終了コード 0 は「何もしなかった」を正しく表す。
  - rejected: 取り消しを表す JSON（{"cancelled": true} など）を出す（結果の形に第三の状態が入り、消費側の分岐が増える）。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A3 FLAG-cli-015 を閉じる。sync の書き込む前の確認の行は、件数が 0 の部分を出さず、書き込む予定が何もない書き込み先の行を出さない。
  - why: 実装がそのとおりで、出力を短く保つ。REQ-cli-040 の行の形に省くことを書き足す。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A4 FLAG-cli-032 を閉じる。diff の "@@" の行と JSON の left_start・right_start は 0 始まりのままとする。
  - why: 自作の差分行であり、テキストと JSON で同じ基底を持つ方が消費側（LLM 連携を含む）に分かりやすい。1 始まりに変えると同じ理由で出力・テスト・手引きを広く直すことになる。
  - rejected: テキストだけ 1 始まりにして diff の慣習に寄せる（同じ diff の中で行番号の基底が二つに割れる）。
  - decided_by: 利用者（推奨を採用）
- A5 FLAG-cli-058 を閉じる。競合の JSON の行の競合の要素は "ref_range"（0 始まりで終端を含まない {"start": 数, "end": 数}）・"left_lines"・"right_lines" だけとし、削除済みの TUI（[tui-disposition](./2026-10-04-tui-disposition.md#A11)）のための 4 欄（left_diff_range・right_diff_range・left_file_lines・right_file_lines）は出さない。
  - why: 4 欄は TUI の表示のために計算していた内部の行範囲で、リポジトリ内に読み手がなく、手引き "skills/remote-merge/references/json-schemas.md" も説明していない。ファイル全体の競合の {"whole_file": true}（[REQ-cli-099](../../ir/cli/conflicts.md#REQ-cli-099)）はそのまま残る。
  - rejected: 4 欄をそのまま仕様にする（使い手のいない内部表現を契約として固定する）。
  - decided_by: 利用者（推奨を採用）
- A6 FLAG-config-001 を閉じる。`グローバル設定` は OS の設定ディレクトリの下の "remote-merge/config.toml" から読む（Linux では XDG_CONFIG_HOME があればその下、なければ "~/.config" の下、macOS では "~/Library/Application Support" の下）。README と手引きの "~/.config/remote-merge/config.toml" の記述をこれに合わせる。
  - why: 実装は OS の設定ディレクトリを使っており、これがクロスプラットフォームの普通の置き場所である。Linux で XDG_CONFIG_HOME を設定していない場合の場所は手引きの記述と同じになる。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A7 FLAG-config-002 を閉じる。[local] の root_dir は明示が要り、既定値は置かない。`グローバル設定` にも `プロジェクト設定` にも [local] の root_dir がない設定はエラーで止まる。
  - why: ローカルの root_dir は比べる・書く対象の片側の起点で、実行したディレクトリを暗黙に使うと意図しない木を扱う事故になる。止まる場所と理由は実装どおりで分かりやすく、手引きの「既定 "."」の記述を直す。
  - rejected: ないとき "." にする（実行場所が変わるとローカル側の木が黙って変わる）。
  - decided_by: 利用者（推奨を採用）
- A8 FLAG-config-005 を閉じる。ローカルの root_dir が存在しないときの知らせ方は変えない。走査の失敗が "Path not found: パス" とパスを示し、どの設定を見直すかはパスから分かる。
  - why: 旧総合仕様も IR もローカルの root_dir がない場合の専用の文言を定めておらず、パスの表示で足りる。専用の文言は要件の追加になるので、必要になったときに決める。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A9 FLAG-config-011 を閉じる。設定ファイルの password の平文は受け付け、その値を使って接続するときは "Server '<サーバ名>': using plaintext password from config. Key authentication is recommended." の警告を標準エラーに出す。接続時にパスワードを尋ねることはない。
  - why: 組織内向けの道具で、パスワードの置き方は利用者が選べる（[foundation A11](./2026-10-12-foundation.md#A11) の境界の内側）。鍵認証を促す警告で足りる。環境変数 REMOTE_MERGE_PASSWORD_<サーバ名> の優先（[REQ-config-019](../../ir/config/values.md#REQ-config-019)）はそのまま。
  - rejected: 平文を拒んで接続時にパスワードを尋ねる（非対話で動く道具の前提に反する）。
  - decided_by: AI（提示した裁定案を利用者が承認）
- A10 FLAG-paths-004 を閉じる。macOS のローカルで OS のパスの長さの上限（PATH_MAX、1024 バイト）が先に当たることは、製品の規則ではなく OS の限界として [REQ-paths-008](../../ir/paths/routes.md#REQ-paths-008) の文に書き、FLAG は残さない。
  - why: 要件は経路によらず同じ結果を求めており、この違いは製品が選んだ差ではなく OS が先に失敗する差である。FLAG として持ち続けると complete にならないため、既知の例外として要件の文に含める。
  - decided_by: AI（提示した裁定案を利用者が承認）
