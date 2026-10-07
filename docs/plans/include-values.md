# Plan: include の値をどの経路でも同じに扱う

## Goal

include に書いた値が、ローカル・SSH・エージェントのどの経路でも同じ一覧になる。存在しない値と root_dir の外を指す値は、警告が利用者に見えて飛ばされる。ファイルの値はそのファイルが、ディレクトリ symlink の値はリンクの名前の下に配下が載る。"a/./b" や "a//b" は "a/b" として扱われる。値がすべて無効なときは、全体に広げずにエラーで止まる。

## Specification

IR は `docs/ir/` にある。この計画が扱うのは次のとおり。

- `docs/ir/config/include.md#REQ-config-032`、`#REQ-config-033`、`#REQ-config-034`、`#REQ-config-035`、`#REQ-config-036`、`#REQ-config-037`
- Examples: EX-config-010、EX-config-011、EX-config-012、EX-config-013、EX-config-014、EX-config-017
- 前提として読むもの（変えない）: `docs/ir/config/filters.md#REQ-config-023`、`#REQ-config-024`、`docs/ir/paths/resolution.md#REQ-paths-001`、`#REQ-paths-002`、`docs/ir/paths/routes.md#REQ-paths-008`、`#REQ-paths-010`、`#REQ-paths-011`、`docs/ir/scan/listing.md#REQ-scan-006`、`#REQ-scan-015`、`docs/ir/scan/directory-links.md#REQ-scan-002`、`#REQ-scan-011`
- 根拠の決定: `docs/decision/records/2026-10-05-quality-config.md` の A1・A4・A9、`docs/decision/records/2026-09-29-adopt-config-filters.md` の A4・A10

## Approach and why

include の扱いを三つに分ける。

一つ目は、設定の値だけで決まる部分（REQ-config-035 と REQ-config-037）。値の正規化と、REQ-config-024 の無効な値かどうかは、ファイルシステムを見ずに決まる。そこで core の `crates/remote-merge-core/src/filter.rs` の `normalize_include_paths` の周りに純粋関数として置き、設定の読み込み（`crates/remote-merge-config/src/lib.rs` の include の和集合をとった後）で一度だけ通す。結果の型は「使える値の一覧」「"." を含むので制限なし」「空でない値がすべて無効」を区別する。最後の場合は設定のエラーとして止める。

"." を制限なし（空の include）に読み替えるのはここなので、各経路の走査も、少ないパスの diff が使う `fetch_children` の include の絞り込み（`filter.rs` の `is_path_included`）も、正規化した値だけを受け取る。先頭の "./" を取り除いてから空の値を無視する扱い（REQ-config-024）と、今の実装が末尾の "/" を取り除く扱い（A10 で要件の外）は変えない。だから "./" だけの値は、今までどおり空の値として無視される。

走査する側にない値と root_dir の外を指す値（REQ-config-032・033）は、走査の時点で分かることなので、REQ-config-037 の無効には数えない。

二つ目は、値ごとに走査する側を確かめる部分（REQ-config-032 と REQ-config-033）。status・diff・merge・sync の全体の走査は、どれも `CoreRuntime::fetch_tree_recursive`（`crates/remote-merge/src/runtime/side_io.rs`）を通る。ここで include の値ごとに、既存のパスの確認 `CoreRuntime::inspect_path` を呼ぶ。この確認は、ローカル・SSH（`runtime/remote_path/` のスクリプト）・エージェント（InspectPath）で同じ結果を返すと REQ-paths-008 が保証している。確認の結果と、空のパスを確認して得た root_dir の実パスから、「走査する」「存在しないので飛ばす」「root_dir の外なので飛ばす」を決める。

root_dir の外かどうかの判定は、diff がすでに持つ `resolved_path_outside_root`（`crates/remote-merge/src/cli/diff.rs`）と同じ規則にする。実パスか、存在しないときは親の実パスが、root_dir の実パスの下にないとき外とみなす。だから途中の要素が外を指す symlink の値も外になる。判定を二つ作らないよう、diff と include の確認が同じ関数を使う形にまとめる。

警告は CLI のプロセスで、REQ-config-024 の警告と同じ出し方で標準エラーに出す。こうすると、エージェントの警告が利用者に見えない問題が、通信の形を変えずに解ける。SSH でも存在しない値が find に渡らなくなるので、走査全体が失敗しなくなる。各経路の走査には、確かめて残った値だけを渡す。残った値が一つもないときは、走査を呼ばずに空の一覧にする。各経路の走査は空の include を制限なしとして扱うので、そのまま渡すと全体に広がり、A4 の趣旨に反するからである。

三つ目は、残った値を各経路の走査がどう載せるかの部分（REQ-config-034 と REQ-config-036）。これは走査の実装そのものなので、経路ごとに直す。

- ローカル: `crates/remote-merge-engine/src/local/mod.rs`
- SSH: `crates/remote-merge-ssh/src/tree_parser.rs` の `build_find_command`
- エージェント: `crates/remote-merge-agent/src/agent/tree_scan.rs` の `resolve_include_roots` と走査

起点がディレクトリでなければ、その 1 件を載せる。起点がディレクトリ symlink なら、リンク先の配下をリンクの名前の下に載せる。今のローカルとエージェントは起点を実パスに置き換え、名前を実パスから作るので、リンクの名前で載らない。また、起点の重複を実パスで除いているので、"current" と "releases/v2" を両方書くと片方が消える。REQ-config-036 のとおり両方をそれぞれの名前で載せるため、重複は設定の読み込みの正規化と同じく名前だけで除く。

エージェントとの通信の形（`AgentRequest::ListTree.include`）は変えない。中身が確かめ済みの値になるだけである。エージェントの中の載せ方は変わる。配置済みのエージェントは、製品の版が変わるリリースで置き直される（版の表示に製品の版を含むため）。

## Scope of change

- `crates/remote-merge-core/src/filter.rs`（正規化と分類の純粋関数とその単体テスト）
- `crates/remote-merge-config/src/lib.rs`（include の分類の結果を使い、すべて無効なら設定のエラーにする）
- `crates/remote-merge/src/runtime/side_io.rs`、`crates/remote-merge/src/runtime/target_io.rs`、`crates/remote-merge/src/runtime/core.rs`
  - 全体の走査の前の値ごとの確認
  - 確認して残った値を各経路の走査に渡す部分。今 include を設定から読んでいて全体の走査に使う箇所は、ローカルが `target_io.rs` で走査用に設定を写している箇所（153 行・298 行付近）、SSH が `core.rs` の 858 行付近、エージェントが `side_io.rs` の 800 行付近で、どれも確かめて残った値に置き換える
  - `fetch_children`（`target_io.rs` の 265 行・445 行付近）は全体の走査ではないので置き換えない
- `crates/remote-merge/src/cli/diff.rs`（`resolved_path_outside_root` を include の確認と共有できる場所へ移す）
- `crates/remote-merge-engine/src/local/mod.rs`、`crates/remote-merge-engine/src/local_io.rs`（ローカルの走査の起点と名前）
- `crates/remote-merge-ssh/src/tree_parser.rs`、`crates/remote-merge-ssh/src/client.rs`（SSH の走査の起点）
- `crates/remote-merge-agent/src/agent/tree_scan.rs`、`crates/remote-merge-agent/src/agent/dispatch.rs`（エージェントの走査の起点と名前）
- 契約テスト: `crates/remote-merge/tests/contract/` の include を扱うファイル（`config_filters_cli.rs` など。新しいファイルを足してもよい）と `crates/remote-merge/tests/contract.rs`
- 今の動きを固定していて、要件に合わせて書き換えてよい既存のテスト
  - `filter.rs` の `test_normalize_all_invalid`（すべて無効なら空を返す。分類の結果の型に合わせて書き換える）
  - `tree_parser.rs` の `test_build_find_command_with_include`・`test_build_find_command_include_single`・`test_build_find_command_include_with_special_chars`・`test_build_find_command_root_trailing_slash`（起点の値をそのまま連結する形を固定している。コマンドの形を変えたときだけ）
  - `config_filters_cli.rs` の 68 行目付近の「すべて無効な場合を避ける」という注記
  - `local/mod.rs` と `tree_scan.rs` の起点の重複を実パスで除くことを固定しているテスト（あれば）

## Step order and prerequisites

S1 → S2 → S3 の順にする。

- S2 と S3 は、S1 で正規化した値だけを受け取る前提で書く
- S3 は、S2 で存在しない値と root_dir の外の値が除かれた後の値だけを扱う前提で書く

S2 を S3 より先にするのは、S3 の走査の層に存在や root_dir の外の判定を足さずに済ませるためである。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-config-035、REQ-config-037 | EX-config-017、EX-config-014 |
| S2 | REQ-config-032、REQ-config-033 | EX-config-010、EX-config-013 |
| S3 | REQ-config-034、REQ-config-036 | EX-config-011、EX-config-012 |

## Left to the implementer

- 純粋関数と結果の型の名前、置くモジュール（正規化と分類は core の中に限る）
- `resolved_path_outside_root` を移す先（diff と include の確認の両方から呼べる製品の runtime か service の中）
- 確認して残った値を各経路の走査に渡す方法（引数を足す、設定を写した値を渡すなど）
- root_dir の外へ解決される値の警告の文言（REQ-config-033 は文言を決めていない。英語で値を含めて書く。テストは値を含む警告が出ることだけを確かめ、文言を固定しない）
- REQ-config-037 のエラーの文言（IR は決めていない。英語で書き、テストは文言を固定しない）
- 走査の層に残る、存在と root_dir の外の確かめ（S2 の後は通常は当たらない）を残すか消すか。ただしエージェントは別のプロセスで受け取る値を確かめる境界なので、root_dir の外を走査しない確かめは残す。どの層でも、同じ値に二度目の警告を出さない
- 契約テストの fixture の作り方とテストの名前

## Stop conditions

- `CoreRuntime::fetch_tree_recursive` を通らずに include を当てて全体を走査する製品コードの経路が見つかった（`fetch_tree_for_subpath` と `fetch_children` は Out of scope のとおり除く）
- REQ-config-036 のとおりリンクの名前で載せると、ディレクトリ symlink の配下の走査（REQ-scan-002・REQ-scan-011）の経路差が広がることが分かった
- 同じディレクトリを指す二つの起点（"current" と "releases/v2"）を走査すると、エージェントの循環の検出（REQ-scan-011）が循環として不完全な一覧を報告することが分かった
- エージェントとの通信の形を変えないと要件を満たせないことが分かった
- 既存のテストが、今回の変更と関係のない理由で失敗した

## Test command

```sh
systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features
```

## Out of scope

- サブパスの走査（`fetch_tree_for_subpath`）が include を当てないこと、ディレクトリのパスを指定した merge と sync が include を当てないこと（FLAG のとおり未決）
- `fetch_children` の include の絞り込みの規則そのもの。S1 で正規化した値を受け取るようになる以外は変えない
- root_dir の外の中身が diff --ref で見える件（品質の台帳の sl-2、FLAG-cli-070）
- include の値の途中の要素の symlink の辿り方。パスの確認の規則（REQ-paths-001）に従うだけで、新しい規則は足さない（root_dir の外かどうかの判定は S2 で行う）

## Steps

### S1: include の値を設定の読み込みで正規化し、すべて無効なら止める

- Purpose: REQ-config-035 の正規化と REQ-config-037 の判定を core の純粋関数一か所に置き、設定の読み込みで使う
- Specification: `docs/ir/config/include.md#REQ-config-035`, `docs/ir/config/include.md#REQ-config-037`
- Prerequisites: none
- May change: `crates/remote-merge-core/src/filter.rs`, `crates/remote-merge-config/src/lib.rs`, `crates/remote-merge/tests/contract/` の include を扱うファイル, `crates/remote-merge/tests/contract.rs`
- Done when: 正規化は "a/./b" と "a//b" を "a/b" にし、"." を制限なしにする。"." と "src" を書いたときも制限なしになる。先頭の "./" と末尾の "/"、空の値（"./" だけの値を含む）、REQ-config-024 の三つの警告の扱いは変わらない。include に "/etc" だけを書いた status は、"Absolute path is not allowed in include filter: /etc" の警告を出した後、設定のエラーで止まって 0 でない終了コードを返し、一覧を出さない。空の値と無効な値だけ（"" と "/etc"）のときも同じく止まる。空の値だけ（"" だけ、"./" だけ）の include は、今までどおり制限なしになる。include に "." だけを書いた status は、root_dir の直下と "src" の下の、左右で違うファイルをどちらも比べる
- Shown by: test — 正規化と分類の表駆動の単体テスト `// @kotowari[REQ-config-035]`・`// @kotowari[REQ-config-037]`（"."・"." と "src"・"a/./b"・"a//b"・無効だけ・空と無効・無効と有効の混在・空だけ）、契約テスト `// @kotowari[EX-config-014]`（"/etc" だけでエラーで止まり、全体を走査しない）、`// @kotowari[EX-config-017]`（"." だけで root_dir 全体を比べる。ローカルと SSH の組で確かめる。SSH の側は今 "." を find の起点にそのまま連結するので RED になる）。どれも変更前の HEAD で RED になることを確かめる
- Left to the implementer: none
- Stop and hand back if: 設定の読み込みの後で include を受け取る箇所のどこかが、正規化されていない生の値を設定ファイルから読み直していた

### S2: 走査の前に include の値を走査する側で確かめる

- Purpose: 全体の走査の前に値ごとに既存のパスの確認を呼び、存在しない値と root_dir の外を指す値を、利用者に見える警告を出して飛ばす
- Specification: `docs/ir/config/include.md#REQ-config-032`, `docs/ir/config/include.md#REQ-config-033`
- Prerequisites: S1
- May change: `crates/remote-merge/src/runtime/side_io.rs`, `crates/remote-merge/src/runtime/target_io.rs`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge/src/runtime/` か `crates/remote-merge/src/service/` の root_dir の外の判定を移す先, `crates/remote-merge/src/cli/diff.rs`, `crates/remote-merge-engine/src/local/mod.rs`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge-agent/src/agent/tree_scan.rs`, `crates/remote-merge/tests/contract/` の include を扱うファイル, `crates/remote-merge/tests/contract.rs`
- Done when: include が空のときは確認を呼ばない。空でないときは、まず空のパスを確認して root_dir の実パスを得る。root_dir が存在しなければ値ごとの確認をせず、今までどおり走査の層の root_dir のエラー（REQ-scan-015）で止まる。値ごとの扱いは次のとおり。存在しない値は "Include path does not exist, skipping: 値" を標準エラーに出して飛ばす（ローカルの今の文言の末尾の " (err)" は付けない）。実パス（存在しないときは親の実パス）が root_dir の実パスの外にある値は、途中の要素の symlink によるものも含めて、値を含む警告を出して飛ばし、root_dir の外を走査しない。それ以外の値（リンク切れの末尾の symlink でリンク先が root_dir の中のものを含む）は走査に渡す。途中の祖先がディレクトリでない値と確認がエラーになった値は、飛ばさずにそのエラーで止める。sudo を使うサーバで古いエージェントのときは、REQ-paths-010 の置き直しのエラーで止める。残った値が一つもないときは、走査を呼ばずに空の一覧にする。include に "src" と "docs" があり、ローカルにだけ "docs" がある status は、SSH の側の警告を出し、"src" の配下を比べる。走査全体は失敗させず、ローカルの "docs" の配下はローカルにだけある項目として出る。エージェントの経路でも同じ警告が標準エラーに出る
- Shown by: test — 契約テスト `// @kotowari[EX-config-010]`（SSH の側にない "docs" の警告と "src" の比較）、`// @kotowari[REQ-config-032]`（エージェントの経路とローカルの経路で同じ文言の警告が標準エラーに出る。値がすべて存在しないとき一覧が空になり、全体を走査しない）、`// @kotowari[EX-config-013]`（SSH の側で root_dir の外を指す symlink の値が警告され、外のファイルが載らない）、`// @kotowari[REQ-config-033]`（エージェントの経路でも同じ。途中の要素が外を指す symlink の値も外として飛ばされる）。root_dir の外の判定を移した後も、diff の既存の root_dir の外のテストが通ること。どれも変更前の HEAD で RED になることを確かめる
- Left to the implementer: none
- Stop and hand back if: 確認の回数が、一回の全体の走査につき include の値の数 + 1 を超える形でしか作れなかった。diff の `resolved_path_outside_root` を共有すると diff の既存の振る舞いが変わることが分かった

### S3: ファイルの値とディレクトリ symlink の値を、どの経路でも書いた名前で載せる

- Purpose: 起点がディレクトリでなければその 1 件を、起点がディレクトリ symlink なら配下をリンクの名前の下に、三つの経路で同じに載せる
- Specification: `docs/ir/config/include.md#REQ-config-034`, `docs/ir/config/include.md#REQ-config-036`, `docs/ir/scan/listing.md#REQ-scan-006`
- Prerequisites: S2
- May change: `crates/remote-merge-engine/src/local/mod.rs`, `crates/remote-merge-engine/src/local_io.rs`, `crates/remote-merge-ssh/src/tree_parser.rs`, `crates/remote-merge-ssh/src/client.rs`, `crates/remote-merge-agent/src/agent/tree_scan.rs`, `crates/remote-merge-agent/src/agent/dispatch.rs`, `crates/remote-merge/src/runtime/core.rs`, `crates/remote-merge/tests/contract/` の include を扱うファイル, `crates/remote-merge/tests/contract.rs`
- Done when: include に "config/app.php" があり左右で中身が違うとき、ローカル・SSH・エージェントのどの経路でも "config/app.php" が modified として一覧に出る。include に "current"（root_dir の中の "releases/v2" へのディレクトリ symlink）があるとき、どの経路でも "current" の下に配下のファイルが載り、片側だけのファイルと判定されない。ディレクトリでないものを指す symlink の値（ファイルへのリンク、リンク切れ）は、include のない全体の走査がその名前で載せるのと同じく、symlink の項目 1 件として載る。"current" と "releases/v2" を両方書いたときは、どちらもそれぞれの名前の下に載る
- Shown by: test — 契約テスト `// @kotowari[EX-config-011]`（ファイルの値が modified で出る。ローカルと SSH の組とエージェントの経路）、`// @kotowari[EX-config-012]`（"current" の下に両側で載る。ローカルと SSH の組）、`// @kotowari[REQ-config-036]`（エージェントの経路でも "current" の下に載る。"current" と "releases/v2" を両方書くと両方載る）、`// @kotowari[REQ-scan-006]`（include に書いたファイルへの symlink とリンク切れの symlink が、どの経路でも symlink の項目 1 件として載る）。どれも変更前の HEAD で RED になることを確かめる
- Left to the implementer: SSH の find で起点ごとに深さを変える書き方、ローカルとエージェントで起点の名前から相対パスを作る書き方
- Stop and hand back if: SSH の find で、ファイルへの symlink の起点を symlink の項目として載せる方法が、走査の対象のサーバ（古い find を含む）で使えなかった。リンクの名前で載せると、include のない全体の走査で同じリンクを辿ったときの名前と食い違うことが分かった
