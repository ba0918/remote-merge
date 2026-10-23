# Plan: --format json の出力を空白のない一行の JSON にする

## Goal

--format json を付けた status・diff・merge・sync・rollback の結果と、どのコマンドの失敗のエラーの JSON も、空白のない一行と LF 一つで標準出力に出るようになり、logs の一件一行と、何も出さない場合の今の振る舞いがテストで守られる。

## Specification

IR は "docs/ir/"、承認済み仕様のコミットは "3d36e087"、実装するブランチは "feat/json-one-line" とする。

- `docs/ir/cli/results.md#REQ-cli-102`（新しい要件。この計画でテストを足す）
- `docs/ir/cli/results.md#REQ-cli-018`（今回二文を足した前からの要件。前からのテストはそのまま残る）
- Examples: EX-cli-116, EX-cli-117, EX-cli-118, EX-cli-119, EX-cli-120, EX-cli-121, EX-cli-122, EX-cli-123, EX-cli-124
- 判断の根拠は `docs/decision/records/2026-10-24-json-one-line.md` にある。手引きと CHANGELOG の扱いは同記録の A9・A10・D1、作らないものは P1〜P3。

## Approach and why

整形した JSON を作っているのは二か所だけで、どちらも serde_json の to_string_pretty を呼んでいる。

- `crates/remote-merge/src/service/output.rs` の format_json: status・diff・merge・sync・rollback の結果のすべてがここを通る（merge の format_merge_outcome_json もこれを呼ぶ）。
- `crates/remote-merge/src/main.rs` の handle_with_format: --format json を持つ六つのコマンドの失敗のエラーの JSON をここで作る。JSON を作れなかったときの固定の文言 {"error": "internal serialization error"} もここにある。

この二か所を serde_json の to_string に替え、固定の文言から空白を除く。to_string は文字列の外に空白・タブ・CR・LF を入れず、文字列の中の制御文字（LF と CR を含む）を "\n" のように書き、U+2028・U+2029・U+0085 はそのまま出す。これは REQ-cli-102 の求める形そのものなので、自前の書き方は作らない（決定の記録の A2・A4。Cargo.lock の 1.0.154 で確かめてある）。出力の最後の LF は今の println! が付けているので、そのまま使う。

REQ-cli-102 は標準出力のバイト列についての約束なので、テストは実行ファイルを起動して標準出力の生のバイト列を確かめる契約テストにする。関数を呼ぶテスト（cli_results.rs の execute_diff など）は出力を書き出さないので、この約束を確かめられない。既存の CliEnv（試験 SSH サーバとエージェント）、agent_support の AgentFixture（故障を入れられる試験サーバ）、isolated_command（診断ログの置き場を一時ディレクトリの home/cache/remote-merge/debug.log にする）を使う。記録の置き方の手本は `crates/remote-merge/tests/contract/logs_level.rs` の logs 関数にある。

一行かどうかは生のバイト列で確かめる。最後のバイトが LF で、LF がちょうど一つ（logs では行の数だけ）あること。str::lines() で数えると、最後の LF がないときや CR LF を見逃すので使わない。「文字列の外に空白がない」は、文字列の中か外かをエスケープ（\" と \\）を含めて追い、外にある空白・タブ・CR を探す小さな確かめの道具を、新しい契約テストのファイルに一つ作って確かめる。今の複数行の出力は LF の確かめで先に落ちるので、この道具が空白を見つけられることは、道具そのもののテストで別に示す。読み直した JSON を serde_json の to_string で書き直して比べる方法は使わない。serde_json に preserve_order の機能を入れていないので、書き直すと欄の順が変わり、正しい出力でも一致しない。

新しい契約テストは一つのファイル `crates/remote-merge/tests/contract/json_one_line.rs` にまとめ、`crates/remote-merge/tests/contract.rs` に `#[cfg(feature = "test-utils")]` を付けて登録する。中には SSH の試験サーバを使うテスト（EX-cli-116・118・121・122）と使わないテスト（EX-cli-117・119・120・123・124 と logs の失敗）が混ざるが、ファイルを分けずにモジュールごと囲む。CI と pre-push はどちらも --all-features で走るので、囲んだテストもそこで必ず走る。

logs の一件一行、記録の残らない logs、引数の解析で止まる場合は、今の振る舞いを仕様に書いたもの（決定の記録の A5・A7・A11）で、コードは変えない。そのテストは最初から通る。

手引きについて、この計画で二つ決めた（どちらも承認のときに利用者に示した）。一つは、決定の記録の D1 が実装に任せた一文の置き場所を、SKILL.md と json-schemas.md の両方に一文ずつと決めたこと。JSON の例はすべて json-schemas.md にあり、SKILL.md は --format json を使うよう勧める側なので、どちらを読んでも分かるようにする。もう一つは、json-schemas.md の Error Response 節の「--format json でエラーになると、すべてのサブコマンドが JSON のエラーを返す」に、引数の解析で止まったときは JSON を出さないことを書き足すこと。REQ-cli-018 に足した文（決定の記録の A11）と手引きを合わせるためで、振る舞いは変えない。

## Scope of change

- `crates/remote-merge/src/service/output.rs`
  - format_json だけ
- `crates/remote-merge/src/service/output_tests.rs`
- `crates/remote-merge/src/main.rs`
  - handle_with_format だけ
- `crates/remote-merge/tests/contract.rs`
  - json_one_line モジュールの登録だけ。`#[cfg(feature = "test-utils")]` の後に `#[path = "contract/json_one_line.rs"]` と `mod json_one_line;` を、ほかのモジュールと同じくアルファベット順の位置に置く
- `crates/remote-merge/tests/contract/json_one_line.rs`（新しい契約テストと、一行の確かめの道具）
- `skills/remote-merge/SKILL.md`、`skills/remote-merge/references/json-schemas.md`
- `CHANGELOG.md`
  - `## [Unreleased]` の「変更（互換性に注意）」だけ

既存のテストが整形した形（複数行や ": " の空白）に頼って落ちたら、そのテストを一行の形で確かめるように直してよい。計画の前に探したときは、そうしたテストは見つからなかった（JSON を読んでから欄を見るものばかりだった）。

新しいテストには、`#[test]`（または `#[tokio::test]`）の直前に `// @kotowari[REQ-cli-102, EX-cli-116]` の形の印を置く。`crates/remote-merge/tests/contract/` の下のテストは、印がないと kotowari check が test_without_id を出すので、確かめの道具のテストにも印（REQ-cli-102）を付ける。シナリオのテストの印には、必ずその要件の ID も入れる（REQ-cli-102 は、S5 の確かめが要件の ID を直接書いた印を数える。REQ-cli-018 には merge_cli.rs と sync_cli.rs に前からテストがあるが、EX-cli-120・123・124 の印も同じ形にそろえる）。

一つのテストファイルだけを走らせるときは `cargo nextest run -p remote-merge --all-features --test contract json_one_line` を使う。--all-features がないと、囲んだモジュールはコンパイルされない。

## Step order and prerequisites

S1 で一行の確かめの道具と結果の JSON の変更を作り、S2 と S3 はその道具を使う。S2 はエラーの JSON の変更で、S1 と別の関数を直すので S1 の後ならいつでもよい。S3 は今の振る舞いを固定するテストだけで、コードを変えない。S4 の手引きと CHANGELOG は、S1 と S2 で出力の形が決まってから書く。S5 は全体の確かめで、テストを後から足す段ではない。

S1 と S2 は、新しい振る舞いの契約テストを先に書き、今の複数行の出力で落ちること（RED）を見てからコードを変える。S3 のテストは今の振る舞いなので最初から通り、RED の証拠には数えない。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-102 | EX-cli-116, EX-cli-118, EX-cli-121, EX-cli-122 |
| S2 | REQ-cli-102 | EX-cli-117 |
| S3 | REQ-cli-102, REQ-cli-018 | EX-cli-119, EX-cli-120, EX-cli-123, EX-cli-124 |
| S4 | なし（決定の記録の A9・A10・D1。IR ではない） | なし |
| S5 | REQ-cli-102, REQ-cli-018 | EX-cli-116〜EX-cli-124 のすべて |

REQ-cli-102 の「logs が失敗したらエラーの JSON の一行だけを出す」にはシナリオがないので、S2 で REQ-cli-102 の印を付けたテストを一つ足して確かめる。

## Left to the implementer

- 一行の確かめの道具と、新しいテストの名前
- 手引きの二つの一文の、それぞれのファイルの中での位置と文言（決定の記録の D1）。手引きは英語で書く
- CHANGELOG の文言

## Stop conditions

- 既存のテストが、出力の空白と改行以外の理由で落ちる
- format_json か handle_with_format のほかに、--format json の出力を整形して書いている場所が見つかる
- serde_json の to_string の出力が、Approach and why に書いた形（U+2028 などをそのまま出す、文字列の外に空白を入れない）と違う

## Out of scope

- 整形した JSON を出す指定（決定の記録の P1）
- 欄の名前・順・中身を変えるトークン削減（P2）と、取り消し・中止が JSON で何も出さない件の見直し（P3）
- 引数の解析で止まったときに JSON を出すこと、--help と --version の扱いを REQ-cli-018 に書くこと
- 版を上げることとリリース
- --format text など、JSON 以外の出力

## Steps

### S1: 結果の JSON を空白のない一行で出す

- Purpose: status・diff・merge・sync・rollback の結果の JSON を、文字列の外に空白のない一行と LF 一つで標準出力に出す
- Specification: `docs/ir/cli/results.md#REQ-cli-102`
- Prerequisites: ブランチ feat/json-one-line の 3d36e087 の上で作業すること
- May change: `crates/remote-merge/src/service/output.rs`（format_json だけ）, `crates/remote-merge/src/service/output_tests.rs`, `crates/remote-merge/tests/contract.rs`（json_one_line の登録だけ）, `crates/remote-merge/tests/contract/json_one_line.rs`
- Done when: EX-cli-116・EX-cli-118・EX-cli-121・EX-cli-122 の場面で実行ファイルを --format json で起動すると、標準出力の生のバイト列が LF で終わり、LF をちょうど一つだけ含み、JSON として読め、文字列の外に空白・タブ・CR がない（EX-cli-118 では生のバイト列に E2 80 A8・E2 80 A9・C2 85 があり、"\u2028"・"\u2029"・"\u0085" という六文字の並びがない。EX-cli-121 では status・merge・sync・rollback のどれもそうなる。EX-cli-122 では終了コードが 0 でない）; 確かめの道具は {"a": 1}、{"a":1} の後にタブのあるもの、{ の直後に CR のあるものを拒み、文字列の中に空白とエスケープの \t（バックスラッシュと t の二文字）だけを持つ {"a":" \t"} と、文字列の中にエスケープした二重引用符と空白を持つものを通す
- Shown by: test — EX-cli-116, EX-cli-118, EX-cli-121, EX-cli-122 の契約テスト（印は REQ-cli-102 とそれぞれの EX の ID）、確かめの道具のテスト（印は REQ-cli-102）、format_json が JsonError { error: "a b" } から {"error":"a b"} とちょうど同じ文字列を返す単体テスト（output_tests.rs に assert_eq で書く）。RED は、契約テストと単体テストが今の to_string_pretty の複数行の出力で落ちること
- Left to the implementer: EX-cli-121 で使う各コマンドの引数（sync も rollback も確認の問いで止まらないようにする。テストの標準入力は空なので、問いを出すと取り消しになって何も出ない。sync は --force か --dry-run、rollback は --list か、--force か --dry-run を付けた取り消しにする）
- Stop and hand back if: diff が U+2028・U+2029・U+0085 を含む中身をテキストとして扱わず、差分の行に出さない; EX-cli-122 を、AgentFixture::new に refuse_agent の故障を付けた書き込み先と、add_server で足した故障のない書き込み先への sync（`crates/remote-merge/tests/contract/sync_partial_status.rs` と agent_support.rs の add_server が手本）で起こせない

### S2: エラーの JSON を空白のない一行で出す

- Purpose: どのコマンドの失敗のエラーの JSON も一行で出し、JSON を作れなかったときの固定の文言からも空白を除く
- Specification: `docs/ir/cli/results.md#REQ-cli-102`
- Prerequisites: S1（一行の確かめの道具）
- May change: `crates/remote-merge/src/main.rs`（handle_with_format だけ）, `crates/remote-merge/tests/contract/json_one_line.rs`
- Done when: 設定が無効な diff --format json（EX-cli-117）と、記録のある置き場での logs --level bogus --format json が、どちらも標準出力にエラーの JSON 一つだけを、LF で終わり LF をちょうど一つ含む文字列の外に空白のない一行で出して終了コード 2 で終わり、logs は記録の行を出さない; handle_with_format の固定の文言が {"error":"internal serialization error"} になっている
- Shown by: test — EX-cli-117 の契約テストと、logs の失敗の契約テスト（印は REQ-cli-102。EX-cli-117 のテストには EX-cli-117 も）。RED は今の複数行のエラーの JSON でこれらが落ちること。固定の文言は実際には出せない（中身が String 一つの型は必ず JSON にできる）ので、テストを書かずに差分で見る
- Left to the implementer: none
- Stop and hand back if: handle_with_format のほかに、エラーの JSON を書いている場所が見つかる

### S3: 今の振る舞いを仕様にしたシナリオを固定する

- Purpose: logs の一件一行、記録の残らない logs、引数の解析で止まる場合の今の振る舞いを、契約テストで固定する
- Specification: `docs/ir/cli/results.md#REQ-cli-102`, `docs/ir/cli/results.md#REQ-cli-018`
- Prerequisites: S1（一行の確かめの道具）
- May change: `crates/remote-merge/tests/contract/json_one_line.rs`
- Done when: 記録が三件の logs --format json の標準出力が LF をちょうど三つ含み、LF で区切ったどの行も JSON 一つとして読め、最後が LF で終わる（EX-cli-119）; 記録のない logs と、INFO の記録だけを置いて --level error で絞った logs が、標準出力を空にして終了コード 0 で終わる（EX-cli-120, EX-cli-123）; パスを付けない merge --format json が、標準出力を空にして終了コード 2 で終わる（EX-cli-124）
- Shown by: test — EX-cli-119（印は REQ-cli-102 と EX-cli-119）, EX-cli-120, EX-cli-123, EX-cli-124（印は REQ-cli-018 とそれぞれの EX の ID）の契約テスト。今の振る舞いなので最初から通り、RED は求めない
- Left to the implementer: none
- Stop and hand back if: どれかが今のコードで落ちる（仕様に書いた「今の振る舞い」が違っていたことになるので、仕様に戻す）

### S4: 手引きと CHANGELOG に一行の出力を書く

- Purpose: 手引きに実際の出力は一行だと書き足し、CHANGELOG に利用者に見える変更として書く
- Specification: `docs/decision/records/2026-10-24-json-one-line.md#A9`, `docs/decision/records/2026-10-24-json-one-line.md#A10`, `docs/decision/records/2026-10-24-json-one-line.md#D1`（IR ではない）, Error Response 節の書き足しは `docs/ir/cli/results.md#REQ-cli-018`（同記録の A11）に手引きを合わせるもの
- Prerequisites: S1, S2
- May change: `skills/remote-merge/SKILL.md`, `skills/remote-merge/references/json-schemas.md`, `CHANGELOG.md`
- Done when: SKILL.md と json-schemas.md のそれぞれに、--format json の実際の出力は空白のない一行の JSON（最後に LF）で、手引きの例は読みやすさのために整形してあると分かる一文があり、例そのものは整形のまま残っている; json-schemas.md の Error Response 節の「すべてのサブコマンドがエラーを JSON で返す」に、引数を読む部品（clap）が止めたとき（必須の引数がない、--tail に数でない値を渡したなど、引数の型に合わない値）は JSON を出さずに終了コード 2 で終わることが書き足されていて、コマンドが自分で確かめる値の誤り（logs --level bogus など）はこれまでどおりエラーの JSON になると読める; CHANGELOG の `## [Unreleased]` の「変更（互換性に注意）」に、--format json の出力が一行になったこと（欄の名前・順・中身は同じこと、整形された形を行ごとに読んでいた使い方は動かなくなること）を書いた一項目がある
- Shown by: artifact — `skills/remote-merge/SKILL.md`, `skills/remote-merge/references/json-schemas.md`, `CHANGELOG.md` の差分を、Done when の三点についてレビューで読む
- Left to the implementer: 二つの一文のそれぞれのファイルの中での位置と文言（D1）, Error Response 節に足す文言, CHANGELOG の文言
- Stop and hand back if: 手引きか README に、出力が整形されていることや、JSON を行ごとに読むことを前提にした説明が見つかり、直し方が一文の書き足しを超える

### S5: 全体を確かめる

- Purpose: この計画の変更とテストが、リポジトリのゲートと仕様の検査を通ることを確かめる
- Specification: `docs/ir/cli/results.md#REQ-cli-102`, `docs/ir/cli/results.md#REQ-cli-018`
- Prerequisites: S1, S2, S3, S4
- May change: なし（直すときは該当する段に戻る）
- Done when: 整形の検査・Clippy・全体のテストが通り、REQ-cli-102 と EX-cli-116〜EX-cli-124 のどれも kotowari query で tests が空でなく、kotowari check がこのブランチで変えたファイルとこの計画の ID にエラーを出さない
- Shown by: check — `cargo fmt --all --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo nextest run --workspace --all-features`; `for id in REQ-cli-102 EX-cli-116 EX-cli-117 EX-cli-118 EX-cli-119 EX-cli-120 EX-cli-121 EX-cli-122 EX-cli-123 EX-cli-124; do kotowari query $id | jq -e '.items[0].tests | length > 0' > /dev/null || echo "no test: $id"; done`（何も出ないこと）; `kotowari check --format json | jq -r '.findings[] | select(.severity == "error") | "\(.kind) \(.path) \(.detail)"'` の出力に、このブランチで変えたファイルとこの計画の ID が出ないこと
- Left to the implementer: none
- Stop and hand back if: この計画と関係のないテストが落ちる（直さずに、落ちたテストと出力を報告する）
