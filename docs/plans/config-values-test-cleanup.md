# Plan: 設定の値の検査のテストを要件の根拠に整理する

## Goal

設定の値の検査の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/config/values.md#REQ-config-014`（`- definition:` から TBL-config-002）、`#REQ-config-015`、`#REQ-config-016`、`#REQ-config-017`（`- definition:` から TBL-config-003）、`#REQ-config-018`、`#REQ-config-019`、`#REQ-config-020`
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-config-values.md`（取り込み、A12 の改訂、FLAG-config-009 から 012、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`、`docs/decision/records/2026-09-28-mutation-scope.md`。要件の本文は `kotowari query REQ-config-0nn` で読む。前の回の計画と同じ流れで、その記録 `docs/testing/config-loading-test-cleanup.md` と、そのテスト `tests/contract/config_loading_cli.rs`・`tests/contract/config_merging.rs` が手本になる。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 56 件のうち実装の中身をなぞるだけのテストは 1 件（`src/config.rs` の `test_parse_permissions_field_wrapper`）だった。これまでと同じく元の単体テストは消さない方針とし、削除は行わない。そのため最初の変異テストは省き、最後に一度だけ回す。既存のテストは消さず、書き換えない。製品コードは変えない。

値の読み方とエラーの文言（REQ-config-014、015、017 の設定の行、018 の値の読み方）は、前の回の `config_merging.rs` と同じく公開の関数 `remote_merge::config::load_config_from_paths` に一時ディレクトリの設定ファイルを渡し、返った値かエラーの文字列で確かめる。エラーの文字列は `format!("{:#}", err)` のように原因の連鎖を含めた表示に要件の文言が含まれることで確かめる。status・diff・merge・sync のエラーは設定のエラーも --max-entries のエラーも `src/main.rs` の `handle_with_format` の共通の経路で終了コード 2 になり、前の回の REQ-config-007 から 009 の実行ファイルのテストがその経路を確かめているため、この回では終了コードを実行ファイルで確かめ直さない。REQ-config-015 は servers のサーバごとの file_permissions・dir_permissions と [defaults] の file_permissions・dir_permissions の四つのキーのそれぞれで、"0o664"・"0664"・"664" が 0o664 として読まれることと、三つの理由（8 進数以外、数字がない "0o"、0o777 を超える値）のエラーにキー名が付くことを確かめる。3 桁でない数字の並び（FLAG-config-009）は使わない。0o777 を超える例は "0o1000" のように u32 に収まる短い値にする（収まらない値は IR にない文言のエラーになる）。

REQ-config-016 は merge と sync で新しく作るファイルとディレクトリの権限で確かめる。`tests/contract/merge_paths.rs` の `new_file_uses_destination_configured_mode` と `sync_creates_missing_parent_directory_with_destination_mode` の組み方（`RuntimeTargets::with_local` でサーバをローカルのディレクトリに差し替えて関数呼び出しで実行し、作られたものの mode を読む）を手本にし、サーバの値があるとき・サーバの値がなく [defaults] があるとき・どちらもないとき（0o664 と 0o775）の三通りを、手本どおりファイルは merge で、ディレクトリは sync で確かめる。サーバの値と [defaults] の値は既定値とも一般的な umask の結果とも違う値（例 0o640・0o750 と 0o604・0o705）にし、既定値の場合だけでは chmod の有無を区別できないことを補う。設定は `AppConfig` を組み立てるのではなく、選ぶ順の全体を通すため `load_config_from_paths` で読んだものを使う。

REQ-config-017 の --max-entries の行は、status・diff・merge・sync の四つのコマンドのそれぞれで、関数呼び出し（`tests/contract/status_support.rs`・`merge_support.rs`・`sync_support.rs` と、diff は `remote_merge::cli::diff::execute_diff`）に max_entries を 0 と 1,000,001 で渡し、TBL-config-003 のその行の全体の文言のエラーで止まることを確かめる。設定の行は `load_config_from_paths` で、範囲の両端（1 と上限）が読まれることと、0 と上限を 1 超えた値がその行の全体の文言のエラーになることを確かめる。文言の "値" はカンマなしで出る（1,000,001 は "got 1000001"）。

実行ファイルを通すテストは、REQ-config-018 の警告、REQ-config-019 のパスワード、REQ-config-020 の鍵のパスの三つだけにする（警告は標準エラーに出て関数呼び出しでは観測しにくく、パスワードと鍵は SSH の接続で初めて使われるため）。前の回の `config_loading_cli.rs` の `Workspace` と同じく、`env_clear` のうえで HOME と XDG の変数を一時ディレクトリに向け、SSH の試験サーバに対して `status --left local --right develop` を起動する。実行ファイルは `Command::output()` で起動し、標準入力を引き継がない（ask のもとでの確認が入力を待たないようにするため）。ただしこの三つは、既存の隔離の確認（`TestDirs::assert_isolated_config`）がそのまま通らない設定を使う: REQ-config-018 は strict_host_key_checking に知らない値、REQ-config-019 は正しくない password、REQ-config-020 は auth が "key"。そこでこの計画では、既存の確認を変えずに、次の確認だけを行う別の補助を足してよい: host が "127.0.0.1"、port がテスト自身の試験サーバのもので 22 でない、user が "fixture-user"、サーバの root_dir と [local] の root_dir が一時ディレクトリの下、[agent] の enabled が false。strict_host_key_checking は REQ-config-018 のテストを除いて "no" を求める（接続はホスト鍵の確認の後に認証へ進むため、REQ-config-019・020 は "no" でないとパスワードにも鍵にも届かない）。password は値を問わない。auth が "key" のときは、補助が起動に使う HOME を引数で受け取り、key を省いたときはその HOME の ".ssh/id_rsa"、"~/" で始まるときはその HOME で展開したパス、絶対パスのときはそのパスが、どれも一時ディレクトリの下にあることを確かめる。起動は `env_clear` の後に HOME・XDG の変数・PATH とテストが明示的に設定するものだけを渡すので、SSH エージェントの変数も渡らない。接続先がテスト自身の試験サーバで、利用者の鍵を読まないことは REQ-testing-001・002 と同じく保たれる。

- REQ-config-018: 知らない値（例 "maybe"）を書いた設定で起動し、出力（標準出力と標準エラーをつないだもの）に "Unknown strict_host_key_checking value: 'maybe', falling back to 'ask'" が含まれることを確かめる。終了コードとその後の接続の結果は確かめない（ask のもとでの未知のホスト鍵の扱いは REQ-ssh-001 の範囲）。値の読み方（"ask"・"yes"・"true"・"no"・"false" と大文字小文字の混ざった形）は `load_config_from_paths` で確かめる。
- REQ-config-019: サーバ名 "develop"、strict_host_key_checking が "no" の設定で、(a) 設定の password が正しくなく環境変数 REMOTE_MERGE_PASSWORD_DEVELOP が "fixture-password" のとき接続できる、(b) 設定の password が "fixture-password" で同じ環境変数が正しくない値のとき接続できない、(c) 同じ環境変数が空で設定の password が "fixture-password" のとき接続できる、(d) 小文字の REMOTE_MERGE_PASSWORD_develop だけが "fixture-password" で設定の password が正しくないとき接続できない、ことを確かめる。status は --format json で起動し、接続できたことは JSON の "files" にローカル側に置いたファイルが出ることで、接続できなかったことは JSON が "error" を持ち "files" を持たないこと（REQ-cli-018 の、失敗しても JSON で返す契約）で確かめ、終了コードと認証のエラーの文言は確かめない（IR が契約にしていない）。否定側の (b) は (a) と、(d) は (c) と、環境変数だけが違う組にして同じテストの中で並べ、否定がパスワードと関係のない理由で成り立たないようにする。環境変数は `env_clear` の後にテストが明示的に設定するものだけにする。
- REQ-config-020: auth が "key"、strict_host_key_checking が "no" のサーバで、key を省いた設定では出力に "Failed to load SSH private key: ~/.ssh/id_rsa" が含まれ、key を "~/keys/missing" と書いた設定では "Failed to load SSH private key: " に続けて一時ディレクトリの HOME の下の "keys/missing" の絶対パスが含まれることを確かめる。どちらも鍵ファイルは置かない。

新しく書くテストは FLAG-config-009 から 012 の挙動を確かめない。具体的には、3 桁でないパーミッション（FLAG-config-009）、auth が "key" のサーバの password の警告（FLAG-config-010）、平文のパスワードの警告と、パスワードがどこにもないときの扱い（FLAG-config-011）、sudo と agent の組み合わせ（FLAG-config-012）を検証の対象に含めない。REQ-config-019 の (c) と (d) では設定の password を使うため平文の警告が出るが、その警告の有無は確かめない。パスワードが環境変数にも設定にもない組み合わせは FLAG-config-011 に当たるため使わない。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(parse_permissions|resolve_file_permissions|resolve_dir_permissions|validate_max_scan_entries|validate_badge_scan_max_files|resolve_max_entries|parse_strict_host_key_checking|convert_server_config|convert_defaults_config|resolve_password|authenticate\b)' src/config.rs src/ssh/client.rs`。`parse_permissions` はキー名を付けるラッパーの `parse_permissions_field` にも一致し、それも対象に含める。`authenticate\b` は `authenticate_or_disconnect` を含めないための形である。鍵を読めないときのエラーを返す `load_secret_key_with_passphrase` はパスフレーズの扱いが中心で、この回の要件は鍵のパスの表示だけを求めるため対象に含めない。`convert_server_config` と `convert_defaults_config` は前の回に回しているため、その見逃しは `docs/testing/config-loading-test-cleanup.md` の記録と比べる。前の回で値の検査の回に回した src/config.rs の 886 行の二件（auth が "key" のときの password の警告）は、この回の取り込みで FLAG-config-010 になったため、その範囲として記録する。`authenticate` の平文のパスワードの警告とパスワードがないときの見逃しは FLAG-config-011 の範囲として記録する。`authenticate` の認証の成否の分岐のうち REQ-config-019・020 の観測で落とせないもの（試験サーバがパスワード認証だけを受け付けるため届かない鍵認証の成否など）は、IR に認証の成否の要件がないため記録だけする。それ以外の見逃しは全て決着の対象にする。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、対象の関数の中のものは決着の対象に含め、他の関数のものは記録だけする。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。既存の FLAG の範囲としての記録は、前の回の `docs/testing/config-loading-test-cleanup.md` と同じ扱いである。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の設定用のモジュール（新規作成。関数呼び出しのものと実行ファイルを起動するものの二つ。前の回の `config_loading_cli.rs` の補助を共有するためにその可視性や置き場所を変えることは含む。既存のテストの中身は変えない）
- `tests/common/mod.rs`（Approach and why の別の隔離の確認の補助の追加だけ。既存の関数は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/config-values-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着の記録）

## Step order and prerequisites

S1 は関数呼び出しの根拠テスト、S2 は実行ファイルの根拠テストで、同じ記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補や verification の見直しの候補、決着の対象でない見逃しが前の回の記録より増えたものがあれば、`docs/testing/config-values-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/config-values`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-config-014, REQ-config-015, REQ-config-016, REQ-config-017, REQ-config-018 | — |
| S2 | REQ-config-018, REQ-config-019, REQ-config-020 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- 前の回の `Workspace` を共有するか、同じ形の補助を新しいモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-config-001 から 012 の挙動を確かめる必要が生じた
- 要件の verification が要件の性質に合わないと判断した（IR は直さず、S3 で候補として手渡す）
- 要件を確かめるために製品コードの挙動や公開範囲を変える必要が生じた
- 変異テストの見逃しや新しいテストが不具合の疑いを示した（見逃しは未決着のまま報告する）
- 変異テストが WSL のメモリを使い切りそうな兆候（スクリプトの上限に当たって失敗し続ける）があり、並列数を下げても解消しない

## Test command

```sh
cargo nextest run --all-features
```

変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない。並列数を 2 より上げない。

## Out of scope

- FLAG-config-001 から FLAG-config-012 の決着と、その挙動の修正
- フィルターの一致の規則（次の回で取り込む）
- 既存の単体テストの書き換えと削除
- ssh_options の中身と、sudo = true の接続（ssh の話題）
- 面の検査（`surface`）の導入

## Steps

### S1: 値の読み方・検査・権限の選び方・走査の上限の根拠テストを書いて印を付ける

- Purpose: 設定の値の読み方と止まる値、新しく作るものの権限、走査の上限の範囲に、公開の関数とコマンドの関数呼び出しを通す根拠テストを置く
- Specification: docs/ir/config/values.md#REQ-config-014, docs/ir/config/values.md#REQ-config-015, docs/ir/config/values.md#REQ-config-016, docs/ir/config/values.md#REQ-config-017, docs/ir/config/values.md#REQ-config-018
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, docs/testing/config-values-test-cleanup.md
- Done when: REQ-config-014 から 017 の `kotowari query` の tests が空でなく、REQ-config-018 には値の読み方を確かめる印付きのテストがある。REQ-config-014 の印付きテストは TBL-config-002 の三行を、それぞれ "Invalid config value: servers.サーバ名.キー - 理由" の全体の文言で確かめる。REQ-config-015 の印付きテストは四つのキーのそれぞれで三つの書き方が 0o664 として読まれることと三つの理由のエラーを、キー名付きの全体の文言で確かめる。REQ-config-016 の印付きテストはファイル（merge）とディレクトリ（sync）のそれぞれで三通りの選び方を、作られたものの mode で確かめる。REQ-config-017 の印付きテストは TBL-config-003 の三行を、設定の行は範囲の両端が読まれることと両外がその行の全体の文言のエラーになることで、--max-entries の行は四つのコマンドのそれぞれで 0 と 1,000,001 がその行の全体の文言のエラーになることで確かめる。REQ-config-018 の印付きテストは "ask"・"yes"・"true"・"no"・"false" と大文字の混ざった形と知らない値の読み方を確かめる。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/config-values-test-cleanup.md にある
- Shown by: test — src/config.rs の test_invalid_port・test_empty_root_dir・test_password_auth・test_error_messages_are_english・test_parse_permissions_* ・test_invalid_server_file_permissions_rejected・test_invalid_defaults_file_permissions_rejected・test_resolve_file_permissions_*・test_validate_max_scan_entries_*・test_validate_badge_scan_max_files_*・test_resolve_max_entries_*・test_parse_strict_host_key_checking_values、tests/contract/merge_paths.rs の new_file_uses_destination_configured_mode を手本にし、load_config_from_paths とコマンドの関数呼び出しを通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで決定表の複数の行を確かめるか分けるか
- Stop and hand back if: merge か sync の関数呼び出しで、設定の値が新しく作るものの mode に現れない（ローカルの書き込み先で権限を付けない経路だった）

### S2: 警告・パスワード・鍵のパスの根拠テストを実行ファイルで書いて印を付ける

- Purpose: 標準エラーに出る警告と、SSH の接続で初めて使われるパスワードと鍵のパスに、実行ファイルを通す根拠テストを置く
- Specification: docs/ir/config/values.md#REQ-config-018, docs/ir/config/values.md#REQ-config-019, docs/ir/config/values.md#REQ-config-020
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, tests/common/mod.rs（別の隔離の確認の補助の追加だけ）, docs/testing/config-values-test-cleanup.md
- Done when: REQ-config-018 から 020 の `kotowari query` の tests が空でない。REQ-config-018 の印付きテストに知らない値の警告の文言を出力で確かめるものがある。REQ-config-019 の印付きテストは Approach and why の (a) から (d) の四通りを、--format json の出力で、否定側を肯定側と組にして確かめる。REQ-config-020 の印付きテストは key を省いたときの "Failed to load SSH private key: ~/.ssh/id_rsa" と、"~/keys/missing" を書いたときの一時ディレクトリの HOME の下の絶対パスを確かめる。どの起動も Approach and why の別の隔離の確認を通してから行う。どのテストを根拠にしたかが docs/testing/config-values-test-cleanup.md にある
- Shown by: test — tests/contract/config_loading_cli.rs の Workspace と status の起動、src/ssh/client.rs の test_resolve_password_* を手本にし、実行ファイルを通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 別の隔離の確認では試験サーバ以外に接続しうる、HOME を一時ディレクトリに向けても鍵のパスが利用者の HOME を指す、または鍵を読めないときのエラーのパスが REQ-config-020 と違う

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の設定用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/config-values-test-cleanup.md
- Done when: 最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの FLAG の範囲か）、決着の対象の見逃しの全てへの決着が、実行したコミットとともに docs/testing/config-values-test-cleanup.md にある。convert_server_config と convert_defaults_config の見逃しは前の回の記録と比べた結果がある。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は名前とともに記録されている。REQ-config-014 から 020 の verification とそれが要件の性質に合う理由が一行ずつある
- Shown by: external — `scripts/mutants.sh --re '(parse_permissions|resolve_file_permissions|resolve_dir_permissions|validate_max_scan_entries|validate_badge_scan_max_files|resolve_max_entries|parse_strict_host_key_checking|convert_server_config|convert_defaults_config|resolve_password|authenticate)' src/config.rs src/ssh/client.rs` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/config-values-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、verification の見直しの候補がある、または決着の対象でない見逃しが前の回の記録より増えた（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/config/values.md#REQ-config-014, docs/ir/config/values.md#REQ-config-015, docs/ir/config/values.md#REQ-config-016, docs/ir/config/values.md#REQ-config-017, docs/ir/config/values.md#REQ-config-018, docs/ir/config/values.md#REQ-config-019, docs/ir/config/values.md#REQ-config-020, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-config-014 から REQ-config-020 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-config-%03g' 14 20); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
