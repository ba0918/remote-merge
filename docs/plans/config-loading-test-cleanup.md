# Plan: 設定の読み込みと合成のテストを要件の根拠に整理する

## Goal

設定の読み込みと合成の取り込みで加えた各要件に、要件を十分に確かめる印付きのテストが kotowari の検査範囲にあり、変異テストの見逃しの決着が記録で裏付けられている。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/config/loading.md#REQ-config-005`、`#REQ-config-006`、`#REQ-config-007`、`#REQ-config-008`、`#REQ-config-009`、`#REQ-config-010`
- `docs/ir/config/precedence.md#REQ-config-011`、`#REQ-config-012`
- `docs/ir/config/defaults.md#REQ-config-013`（`- definition:` から決定表 TBL-config-001 を辿れる）
- 手順の方針: `docs/ir/testing/methods.md#REQ-testing-009`、`#REQ-testing-010`、`#REQ-testing-012`（いずれも review）

判断の出典は `docs/decision/records/2026-09-28-adopt-config-loading.md`（取り込み、FLAG-config-001 から 008、テストの仕分けの件数）、`docs/decision/records/2026-09-27-test-method-selection.md`（手法の選び方）、`docs/decision/records/2026-09-28-mutation-scope.md`（変異テストを決着の対象の関数に絞ること、テストを削除しない整理で最初の実行を省くこと、`scripts/mutants.sh --re`）。要件の本文は `kotowari query REQ-config-0nn` で読む。同じ手順を merge で行った記録 `docs/testing/merge-hunks-test-cleanup.md` が記録の書き方の手本になる。既に印の付いた REQ-config-001 と REQ-config-002 は対象にしない。

## Approach and why

取り込みの仕分けでは、この範囲のテスト 47 件のうち 8 件（`src/config.rs` の `test_merge_raw_defaults_*` 5 件、`test_load_config_delegates_to_load_config_with_project_override`、`test_defaults_config_default_values`、`test_ssh_config_default_is_ask`）が実装の中身をなぞるだけのテストだった。これまでの整理と同じく元の単体テストは消さない方針とし、この計画は削除を行わない。そのため REQ-testing-012 の改訂（`docs/decision/records/2026-09-28-mutation-scope.md` の A2）により最初の変異テストは省き、最後に一度だけ回す。`src/config.rs` はこれまで変異テストを回していないため、見逃しは比べる相手なしに新しい見逃しとして決着させる。既存のテストは消さず、書き換えない。製品コードは変えない。

設定ファイルの場所・--config・読み込みのエラー・ホームの展開（REQ-config-005 から 010）は、カレントディレクトリと環境変数に依存するため、実行ファイルを起動して確かめる。テストのプロセスの中でカレントディレクトリや環境変数を変えると `cargo test` の並列実行で互いに干渉するため、関数呼び出しでは確かめない。実行ファイルは `env_clear` したうえで `HOME` を一時ディレクトリに向け、`XDG_CONFIG_HOME` を `HOME` の下の ".config" に向け、`XDG_DATA_HOME` も一時ディレクトリに向ける（`tests/common/mod.rs` の `CliEnv::cmd` と同じ考え方の隔離だが、`CliEnv::cmd` は `XDG_CONFIG_HOME` を別の一時ディレクトリに向けている点が違う）。こうすると利用者のマシンの設定を読まず、グローバル設定の場所は "HOME/.config/remote-merge/config.toml" になる。この場所は旧資料と Linux の実装で一致するが macOS では異なる（FLAG-config-001）ため、グローバル設定を置くテストは `#[cfg(target_os = "linux")]` にする。`CliEnv::cmd` は常に --config を付けるため、--config を付けない実行には使えない。`tests/common` の SSH の試験サーバ（`ssh_server::TestServer::filesystem_without_agent`）と設定の生成（`gen_config`）を使い、設定ファイルを置く場所とカレントディレクトリをテストごとに選べる補助を作る。この補助は、起動の前にテストが書いた全ての設定ファイル（カレントディレクトリ、--config の指定先、グローバル設定）に `TestDirs::assert_isolated_config` と同じ確認（host が "127.0.0.1"、port がテスト自身の試験サーバのもので 22 でない、auth が "password" で key を持たない、サーバの root_dir が一時ディレクトリの下）をかける（REQ-testing-001・002 の安全確認を外さないため）。試験サーバは起動時に渡したディレクトリの外を指すリモートコマンドを受け付けないため、どの設定のサーバの root_dir もそのディレクトリの下に置く。この補助で起動する実行ファイルは全て `status --left local --right develop` にする（サブコマンドなしでは TUI が立ち上がって止まらないため）。この起動のテストは SSH の試験サーバを使うので、`tests/contract.rs` の `#[cfg(feature = "test-utils")]` の下のモジュールに置き、S2 の関数呼び出しのテストは feature の要らない別のモジュールに置く。

どの設定が読まれたかは、`status --left local --right develop --format json` の出力で観測する。status の JSON のトップレベルの "files"（"path" と "status"）と、"left"・"right" の "label" と "root" は `docs/ir/cli/status-output.md` の要件が契約にしているため、これを使う。たとえば二つの候補の設定で `[local]` の root_dir を別々のディレクトリにし、そのどちらに置いたファイルが "files" に出るかで、読まれた設定を区別する。ファイルはローカルの側にだけ置くため "left_only" になり、status の終了コードは 1 になる（TBL-cli-002）。成功の確かめ方に終了コード 0 を求めない。rollback --list の JSON の "target" は IR が契約にしていないため使わない。エラーの行は --format を付けずに起動し、終了コード 2 と、標準出力と標準エラーをつないだものに文言が含まれることで確かめる（IR はエラーの出力先を契約にしていないため、出力先は固定しない）。REQ-config-007 は --config に "./" を付けない相対パスを渡し、"絶対パス" はそのパスをカレントディレクトリから解決したものと比べる（絶対パスを渡すと、相対かどうかの判定を変えた変異を落とせないため）。REQ-config-007 と 008 のパスは、一時ディレクトリが symlink を含む環境に備えてカレントディレクトリを正規化して比べる。REQ-config-008 の探した二つのパスは、"Config file not found." とプロジェクト設定のパス（カレントディレクトリの ".remote-merge.toml"）が含まれることで確かめ、グローバル設定のパスの形は確かめない（グローバル設定の場所は IR が契約にしておらず、FLAG-config-001 の範囲のため）。

セクションごとの合成・キーごとの合成・既定値（REQ-config-011 から 013）はカレントディレクトリにも環境変数にも依存しないため、`tests/contract/config_precedence.rs` と同じく公開の関数 `remote_merge::config::load_config_from_paths` にグローバル設定とプロジェクト設定のパスを渡し、返った `AppConfig` の値で確かめる。REQ-config-011 は [ssh]・[backup]・[agent] のそれぞれについて、グローバル設定でそのセクションのキーを既定値と違う値にし、プロジェクト設定の同じセクションではそのキーを省く構成で、省いたキーが既定値になる（グローバルの値を引き継がない）ことと、プロジェクト設定にそのセクションがないときはグローバル設定の値が使われることを確かめる。REQ-config-013 は TBL-config-001 の 14 行を全て確かめる。実装は、セクションがないときと、セクションはあるがキーを省いたときとで別の経路で既定値を決め、[backup] はさらにプロジェクト設定だけ・グローバル設定だけ・両方で別の箇所を通るため、[ssh]・[backup]・[agent]・[defaults] の行は「セクションなし」と「セクションはあるがキーを省く」の両方で確かめ、[backup] の「キーを省く」はプロジェクト設定だけ・グローバル設定だけ・両方の三通りで確かめる。

新しく書くテストは FLAG-config-001 から 008 の挙動を確かめない。具体的には、グローバル設定の場所のディレクトリ（FLAG-config-001）、[local] がないときや root_dir のない [local]（FLAG-config-002、テストの設定には常に root_dir のある [local] を書く）、max_scan_entries・badge_scan_max_files と [scan]（FLAG-config-003）、相対パスの [local] の root_dir（FLAG-config-004、"~/" で始まるもの以外は絶対パスにする）、root_dir が存在しないとき（FLAG-config-005・006）、知らないキー（FLAG-config-007）、init・logs・events の --config の警告（FLAG-config-008）を検証の対象に含めない。値の検査（port・auth・パーミッションの書き方など）とフィルターの一致の規則は後の回で扱うため、テストの設定には正しい値だけを書く。

変異テストは決着の対象の関数に絞って一度だけ回す: `scripts/mutants.sh --re '(global_config_path|project_config_path|load_config_with_project_override|load_config_from_paths|load_raw_config|merge_configs|convert_agent_config|convert_raw_ssh_config|merge_raw_defaults|convert_defaults_config|convert_server_config|expand_tilde)' src/config.rs`。このうち、`merge_configs` の max_scan_entries と badge_scan_max_files の見逃しは FLAG-config-003、[local] がないときのエラーの見逃しは FLAG-config-002 の範囲として記録する。それ以外の見逃しは全て決着の対象にする。`convert_server_config` の値の検査（port が 0、auth の不明な値、root_dir が空、パーミッション、password の警告）や `merge_configs` のフィルターの重複除き・バックアップ領域の除外・include の正規化は、値の検査とフィルターの回で要件になる振る舞いだが、IR にまだ要件がないため、見逃しが残ったら要件に基づくテストを足さず、後の回に回す候補として記録の文書に書いて作業を返し、利用者の判断を待つ。cargo-mutants 27.1.0 では構造体のフィールドを消す変異が `--re` で除かれないため、結果にはそれらが混ざる。対象の関数の中のものは決着の対象に含め、他の関数のものは記録だけする。

最後に残った決着の対象の見逃しは一件ずつ、テストの追加、同等変異の登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれかで決着させ、その記録を人が確かめられる文書に残す。見逃しを落とすためにテストを足したら、その状態で変異テストを回し直してから記録する。同等変異の登録は、別の文脈のエージェントにその変異を落とすテストを書かせて書けなかったときに限り、その試みを登録の why に書く。変異の一時的な書き換えで確かめるときは本体の作業ツリーで行い、確かめた後に `git diff --stat src/` が空に戻ることを確かめる。作業ツリーやブランチは作らない。

## Scope of change

- `tests/contract.rs`（モジュール宣言の追加だけ）
- `tests/contract/` の設定用のモジュール（新規作成。実行ファイルを起動するものと関数呼び出しのものの二つ。補助関数もここに置く）
- `tests/common/mod.rs`（--config を付けずに実行ファイルを起動するための、既存の隔離と同じ補助の追加だけ。既存の関数は変えない）
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録だけ）
- `docs/testing/config-loading-test-cleanup.md`（新規。根拠にしたテスト、変異テストの結果と見逃しの決着の記録）

## Step order and prerequisites

S1 は実行ファイルを通す根拠テスト、S2 は関数呼び出しの根拠テストで、同じ記録の文書を書き換えるため順に行う。S3 で変異テストを一度だけ回して見逃しを決着させる。S4 で全体を確かめる。

S3 には利用者の判断を待つ区切りがある。新しい FLAG の候補や verification の見直しの候補があれば、`docs/testing/config-loading-test-cleanup.md` に書いてコミットしたところで作業を返し、利用者の判断が書き足されてから完了させる。

作業ブランチは `adopt/config`。取り込みの決定と IR は同じブランチにコミット済みで、テストが紐づくまで `kotowari check` が requirement_without_test を報告するため、main へのマージはこの計画の完了後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-config-005, REQ-config-006, REQ-config-007, REQ-config-008, REQ-config-009, REQ-config-010 | — |
| S2 | REQ-config-011, REQ-config-012, REQ-config-013 | — |
| S3 | REQ-testing-012（review）, REQ-testing-009（review）, REQ-testing-010（review） | — |
| S4 | 上記すべて | — |

## Left to the implementer

- 新しいテストのモジュールの名前と、テストの分け方
- 実行ファイルを起動する補助を `tests/common/mod.rs` に置くか、設定用のモジュールに置くか

## Stop conditions

- テストを書く途中で、実装が IR の要件と食い違うことが分かった（FLAG 候補として報告し、実装は直さない）
- 要件を確かめるために FLAG-config-001 から 008 の挙動を確かめる必要が生じた
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

- FLAG-config-001 から FLAG-config-008 の決着と、その挙動の修正
- 値の検査とフィルターの一致の規則（後の回で取り込む）
- 既存の単体テストの書き換え（`load_config_with_project_override` を呼ぶ単体テストが利用者のマシンのグローバル設定を読むことも含む。気づいたことは記録の文書に書くだけにする）
- テストの削除
- 面の検査（`surface`）の導入

## Steps

### S1: 設定ファイルの場所・--config・読み込みのエラー・ホームの展開の根拠テストを書いて印を付ける

- Purpose: どの設定ファイルが読まれ、読めないときにどう止まるかに、実行ファイルを通す根拠テストを置く
- Specification: docs/ir/config/loading.md#REQ-config-005, docs/ir/config/loading.md#REQ-config-006, docs/ir/config/loading.md#REQ-config-007, docs/ir/config/loading.md#REQ-config-008, docs/ir/config/loading.md#REQ-config-009, docs/ir/config/loading.md#REQ-config-010
- Prerequisites: none
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, tests/common/mod.rs（補助の追加だけ）, docs/testing/config-loading-test-cleanup.md
- Done when: REQ-config-005 から 010 の `kotowari query` の tests が空でない。REQ-config-005 の印付きテストは、カレントディレクトリの ".remote-merge.toml" が読まれること（status の JSON に、その [local] の root_dir に置いたファイルが出る）と、カレントディレクトリになく親のディレクトリにだけ ".remote-merge.toml" があってグローバル設定もないとき、親のものを読まずに設定が見つからないエラーで止まることを確かめる。REQ-config-006 の印付きテストは、カレントディレクトリからの相対パスで --config を渡し、カレントディレクトリにも別の root_dir の ".remote-merge.toml" を置いた構成で、--config の設定の root_dir のファイルが出てカレントディレクトリのものは使われないことと、--config の設定にないサーバをグローバル設定から使えること（グローバル設定が合成されること）を確かめる。REQ-config-007 の印付きテストは、"./" を付けない相対パスで存在しないパスとディレクトリを --config に渡したときの終了コード 2 と、出力の "Config file not found: 絶対パス" と "Config path is not a regular file: 絶対パス" を確かめる。REQ-config-008 の印付きテストは、どちらの設定もないときの終了コード 2 と、出力の "Config file not found." とカレントディレクトリの ".remote-merge.toml" のパスを確かめる（グローバル設定のパスの形は確かめない）。REQ-config-009 の印付きテストは、TOML として読めない設定での終了コード 2 と、出力の "Failed to parse config file: " を確かめる。REQ-config-010 の印付きテストは、[local] の root_dir を "~/" で始め、HOME の下のそのディレクトリに置いたファイルが status の JSON に出ることを確かめる。どのテストを根拠にし、どの既存テストの組み方を手本にしたかが docs/testing/config-loading-test-cleanup.md にある
- Shown by: test — src/config.rs の test_load_config_with_project_override_uses_specified_file・test_load_config_with_project_override_nonexistent_file・test_load_config_with_project_override_directory_rejected・test_config_not_found、tests/cli_error_handling.rs の test_missing_config_exits_with_code_2・test_invalid_toml_exits_with_code_2、src/ssh/client.rs の test_expand_tilde_home_dir、tests/contract/backup_rollback_cli_e2e.rs の実行ファイルの起動の組み方を手本にし、実行ファイルを通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: 一つのテストで複数の要件を確かめるか分けるか
- Stop and hand back if: 設定ファイルの安全確認が既存の補助から流用できず新しく書く必要がある、HOME と XDG_CONFIG_HOME を一時ディレクトリに向けても実行ファイルが利用者のマシンの設定を読む、または status が SSH の試験サーバに接続できず設定の違いを観測できない

### S2: セクションごとの合成・キーごとの合成・既定値の根拠テストを書いて印を付ける

- Purpose: グローバル設定とプロジェクト設定の合成の単位と、省いたキーの既定値に、公開の読み込み関数を通す根拠テストを置く
- Specification: docs/ir/config/precedence.md#REQ-config-011, docs/ir/config/precedence.md#REQ-config-012, docs/ir/config/defaults.md#REQ-config-013
- Prerequisites: S1
- May change: tests/contract.rs, tests/contract/ の設定用モジュール, docs/testing/config-loading-test-cleanup.md
- Done when: REQ-config-011 から 013 の `kotowari query` の tests が空でない。REQ-config-011 の印付きテストは [ssh]・[backup]・[agent] のそれぞれで、グローバル設定で既定値と違う値にしたキーをプロジェクト設定の同じセクションで省くと既定値になることと、プロジェクト設定にそのセクションがなければグローバル設定の値になることを確かめる。REQ-config-012 の印付きテストは [defaults] の file_permissions と dir_permissions で、プロジェクト設定にあるキー・プロジェクト設定になくグローバル設定にあるキー・どちらにもないキーの三通りを確かめる。REQ-config-013 の印付きテストは TBL-config-001 の 14 行の全てを、Approach and why に書いた経路の全て（セクションなし、セクションはあるがキーを省く、[backup] はプロジェクト設定だけ・グローバル設定だけ・両方）で確かめる。どのテストを根拠にしたかが docs/testing/config-loading-test-cleanup.md にある
- Shown by: test — src/config.rs の test_agent_config_project_overrides_global・test_defaults_merge_field_level_via_config_load・test_defaults_merge_field_level_file_only_override・test_minimal_config・test_agent_config_defaults_when_absent・test_defaults_absent_uses_hardcoded_fallback・test_ssh_config_strict_host_key_checking_default_when_omitted、tests/contract/config_precedence.rs の load_pair を手本にし、load_config_from_paths を通すテストを新しく書いて印を付ける（手本にした元のテストは残し、書き換えない）
- Left to the implementer: none
- Stop and hand back if: 省いたキーがグローバル設定の値を引き継ぐなど、実装が REQ-config-011 と食い違う

### S3: 変異テストを一度回して見逃しを決着させる

- Purpose: 決着の対象の関数の見逃しを一件ずつ決着させる
- Specification: docs/ir/testing/methods.md#REQ-testing-012, docs/ir/testing/methods.md#REQ-testing-009, docs/ir/testing/methods.md#REQ-testing-010
- Prerequisites: S2
- May change: tests/contract/ の設定用モジュール（テストの追加）, tests/contract.rs, .kotowari/mutants-equivalents.yaml, docs/testing/config-loading-test-cleanup.md
- Done when: 最後のテストの状態で回した Approach and why の変異テストの集計、見逃し一件ずつの位置と変異の内容、決着の対象かどうか（外したものはどの範囲か）、決着の対象の見逃しの全てへの決着（足したテスト、同等変異の理由付きの登録、既存の FLAG の範囲としての記録、新しい FLAG の候補としての報告のどれか）が、実行したコミットとともに docs/testing/config-loading-test-cleanup.md にある。負荷の下で落ちるテスト（tui_merge や agent_ssh）だけに検知された変異は名前とともに記録されている。REQ-config-005 から 013 の verification とそれが要件の性質に合う理由が一行ずつあり、合わないと判断したものは IR を直さず候補として挙がっている
- Shown by: external — `scripts/mutants.sh --re '(global_config_path|project_config_path|load_config_with_project_override|load_config_from_paths|load_raw_config|merge_configs|convert_agent_config|convert_raw_ssh_config|merge_raw_defaults|convert_defaults_config|convert_server_config|expand_tilde)' src/config.rs` をバックグラウンドで実行して完了を待ち（結果の読み方は `target/mutants-run/mutants.out/outcomes.json` と `log/`）、その結果を docs/testing/config-loading-test-cleanup.md に書き、利用者がそれを読んで見逃しの決着を確かめる
- Left to the implementer: 文書の見出しと表の形
- Stop and hand back if: 見逃しが不具合の疑いを示した、値の検査かフィルターの見逃しが残った、または verification の見直しの候補がある（文書に書いてコミットして作業を返し、利用者の判断が書き足されるまで S3 は完了しない）、実行がメモリ上限で失敗し続ける

### S4: 計画の対象が全て揃ったことを確かめる

- Purpose: 対象の要件に印付きのテストがあり、検査とテストが通ることを示す
- Specification: docs/ir/config/loading.md#REQ-config-005, docs/ir/config/loading.md#REQ-config-006, docs/ir/config/loading.md#REQ-config-007, docs/ir/config/loading.md#REQ-config-008, docs/ir/config/loading.md#REQ-config-009, docs/ir/config/loading.md#REQ-config-010, docs/ir/config/precedence.md#REQ-config-011, docs/ir/config/precedence.md#REQ-config-012, docs/ir/config/defaults.md#REQ-config-013, docs/ir/testing/methods.md#REQ-testing-012
- Prerequisites: S3
- May change: none
- Done when: REQ-config-005 から REQ-config-013 の全てで `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に error を出さず、テストと静的検査が通る
- Shown by: check — `for id in $(seq -f 'REQ-config-%03g' 5 13); do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "missing $id"; done`、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo nextest run --all-features`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た
