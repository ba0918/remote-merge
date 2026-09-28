# merge の指定・確認・出力を kotowari に取り込む際の判断

## Context

merge の IR は旧仕様の移行時に、merge と sync に共通の安全確認・参照先・競合・結果の要件（[REQ-cli-002](../../ir/cli/safety.md#REQ-cli-002)〜[REQ-cli-004](../../ir/cli/safety.md#REQ-cli-004)、[REQ-cli-011](../../ir/cli/reference.md#REQ-cli-011)、[REQ-cli-017](../../ir/cli/conflicts.md#REQ-cli-017)〜[REQ-cli-019](../../ir/cli/results.md#REQ-cli-019)）だけが作られ、merge の入口が受け付ける指定・終了コード・出力と既存テストの大半が仕分けられていなかった。
merge は大きいため、指定・確認・出力（この記録）、書き込みの中身、symlink と削除、変更のまとまりを選ぶマージの 4 回に分けて取り込む。
旧資料（[旧総合仕様](../../archive/spec.md) の使い方の例・JSON 出力スキーマ・マージ前確認・センシティブファイル警告・サーバ間比較の節、[旧個別仕様 symlink-merge](../../archive/spec/symlink-merge.md) の 3.3・3.12、既存要件）と、入口 `merge`（パス、--left、--right、--ref、--dry-run、--force、--format）から見える現行実装・テストを突き合わせ、一致するものは現状を仕様として追認し、食い違い・欠落は FLAG として未決のまま残す。
既存要件 REQ-cli-002、REQ-cli-003、REQ-cli-004、REQ-cli-011、REQ-cli-017、REQ-cli-018、REQ-cli-019 は merge と sync について実装と一致したため変更しない。バイナリの転送（[REQ-cli-010](../../ir/cli/binary.md#REQ-cli-010)）は書き込みの中身の回、--delete と種類の違いは symlink と削除の回、--hunks は変更のまとまりの回、--max-entries は走査件数の上限として scan、実行開始時の期限切れバックアップの削除は backup の話題で扱う。
要件は 6 件で limits.requirements を超えないため、1 文書にまとめる。
テストの内訳（対象 70 件）: 要件の根拠 46 件、FLAG の挙動のテスト 6 件、実装詳細をなぞるだけ 3 件、残す 15 件。

## Agreements

- A1 現行実装をレビューなしで仕様とする。実装は merge で、パスがないとき引数の解析エラー、--left か --right がないとき "--left and --right are required for merge command (e.g. --left local --right staging)"、--left と --right が同じとき "--left and --right must be different (both resolved to '名前')"、設定にないサーバ名を指定したとき "Server '名前' not found in config"、--format に text・json・diff 以外を指定したとき "Unknown format: '値' (expected text, json, or diff)" のエラーで止まり、いずれも終了コード 2 を返す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A2 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は機密ファイルのスキップだけがある merge の終了コードを 0 とし、実装は merge の failed が空なら終了コード 0、failed が一件でもあれば 2、エラーで止まったときは 2 を返す。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A3 旧資料と現行実装が一致するため、レビューなしで仕様とする。旧資料は merge の JSON に merged（path・status・backup・ref_badge）、skipped（path・reason）、deleted、failed を出し、deleted は --delete のときだけ中身があり、ref_badge は三者比較のときだけ出すとし、実装は同じ形で出し、failed には path と error を出し、deleted は空でも出し、ref（label と root）は参照先を使うときだけ出し、merged の status をファイル全体を書き込んだとき "ok"、--dry-run では "would merge" とする。
  - why: 旧資料と実装の一致を確認し、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A4 現行実装をレビューなしで仕様とする。実装は merge のテキスト出力で、書き込んだファイルを "Merged: パス"（バックアップがあれば続けて " (backup: バックアップ)"）、--dry-run で書き込む予定のファイルを "Would merge: パス"、失敗したファイルを "Failed: パス (理由)" の行で出し、書き込む対象もスキップも失敗もないときは "no files to merge in the specified path(s)" を出す。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A5 現行実装をレビューなしで仕様とする。実装は --ref に左と同じ指定をした merge で標準エラーに "Warning: --ref server is the same as left side; ref comparison skipped."、右と同じ指定で "Warning: --ref server is the same as right side; ref comparison skipped." を出し、参照先を使わずに続ける。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
- A6 現行実装をレビューなしで仕様とする。実装は --ref があり --force も --dry-run もない merge で、参照先に対して左右が異なる変更をした競合のあるファイルを書き込まずに failed に error "three-way conflict" で出し、競合のない他のファイルは書き込む。
  - why: 旧資料に記述はないが利用者に見える挙動をテストが確かめており、利用者が一覧から外さなかった。
  - decided_by: 利用者（現状追認の一覧を承認）
  - superseded_by: [merge で書き込み先の変更を黙って失う二つの問題を直す判断の A1](./2026-09-28-merge-ref-hunks-fix.md#A1)
- A7 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は種類の違い・symlink の削除・パスを辿れないことによるスキップが一件でもある merge の終了コードを 2 にするとするが、実装は merge の終了コードの判定に skipped を数えないため、そのスキップだけの merge は終了コード 0 になる。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A8 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) は --dry-run の終了コードを 0 とするが、実装は --dry-run の merge でも中身を読み比べるファイルを読めなかったとき failed に出し、終了コード 2 を返す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A9 未決の FLAG として残す。[旧総合仕様の JSON 出力スキーマ](../../archive/spec.md) は merge の JSON で機密ファイルのスキップの reason を "sensitive" とするが、実装は "sensitive file" とする。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A10 未決の FLAG として残す。[旧個別仕様 symlink-merge の 3.3](../../archive/spec/symlink-merge.md) はスキップのテキスト出力を "  - パス (skipped: 理由)" の行とするが、実装の merge は "Skipped: パス (理由)" の行で出す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A11 未決の FLAG として残す。[旧総合仕様のサーバ間比較の節](../../archive/spec.md) はリモート間の merge でサーバ名を入力させる確認を出し、--force で確認を省略できるとするが、CLI の実装は確認を出さずに --force も --dry-run もないリモート間の merge を止め、テキストでは "Warning: merging between two remote servers (左 → 右)" と "Use --force to proceed, or --dry-run to preview changes." を出し、JSON では failed に path が "" の一件を出し、終了コード 2 を返す。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A12 未決の FLAG として残す。[旧総合仕様の使い方の例とマージ前確認の節](../../archive/spec.md) は merge が確認のプロンプトを出し、--force で省略するとするが、CLI の merge は確認のプロンプトを出さず（TUI には確認がある）、--force は機密ファイルを対象に含めること、リモート間の merge を止めないこと、三者の競合の確認をしないことに効く。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A13 未決の FLAG として残す。[旧総合仕様のセンシティブファイル警告の節](../../archive/spec.md) は機密ファイルを検知したらマージの前に警告して続行するかを尋ねるとするが、CLI の merge は尋ねずに --force のない機密ファイルをスキップし、テキスト形式のときだけ（--dry-run でも）標準エラーに "N sensitive file(s) will be skipped. Use --force to include them." を出す。この N には書き込み先にだけあるファイルなど機密ファイル以外のスキップも数えるため、機密ファイルがなくても件数が出ることがある。
  - why: 旧資料と実装が食い違い、どちらに合わせるかは次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A14 未決の FLAG として残す。--ref があり --force も --dry-run もない merge で、実装は左・右・参照先のどれかで中身がそろわないファイルを書き込まずに failed に error "three-way comparison incomplete" で出すため、左にだけある新しいファイルは --ref 付きでは --force なしに書き込めない。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
- A15 未決の FLAG として残す。--ref がある --dry-run の merge で、実装は三者の競合を確かめないため、実際に実行すると競合で失敗するファイルも merged に status "would merge" で出す。旧資料に記述がなくテストもない。
  - why: 利用者に見える挙動に仕様とテストが欠けており、次にこの機能を扱うときに決める。
  - decided_by: 利用者（現状追認の一覧を承認）
