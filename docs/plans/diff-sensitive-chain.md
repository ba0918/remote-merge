# Plan: diff の機密の連鎖の判定で字面で辿れない段を機密でないとみなす

## Goal

CLI diff で、機密パターンに当たる名前を含まない symlink の連鎖は、リンク文字列が字面で root_dir の中に辿れない形でも、--force なしで参照先の内容差が表示される。

## Specification

IR は `docs/ir/` にある。この計画が扱うのは次のとおり。

- `docs/ir/cli/symlink-diff.md#REQ-cli-023`（判定の範囲を書き足した要件）
- 例: EX-cli-067、EX-cli-068（`docs/ir/cli/symlink-diff.md`。どちらも REQ-cli-020 と REQ-cli-023 についての例）

判断の出典は `docs/decision/records/2026-09-30-diff-sensitive-chain.md`。A6（字面で辿れる範囲で判定する）、A7（確かめられない段は機密でないとみなす）、A8（前の記録の A4 の撤回）、A10（字面で辿る規則は今のまま）、A12（テストの扱い）がこの計画の直接の根拠になる。撤回された決定は `docs/decision/records/2026-09-30-diff-root-symlink.md` の A4。要件と例の本文と印付きのテストは `kotowari query REQ-cli-023` や `kotowari query EX-cli-067` で読む。

## Approach and why

`src/cli/diff.rs` の `sensitive_link_chain` は、各段のリンク文字列を字面で親と結んで次の段を求め、辿れなくなったとき（絶対パスのリンク文字列が実パスに直した root_dir と設定に書いた root_dir のどちらの下にもない、または相対のリンク文字列が root_dir より上に出る）に、連鎖の最初の段の実パスが root_dir の中かどうか（`entry_inside_root`）を返している。`entry_inside_root` は連鎖の最初の段を最後まで解決した参照先の実パス（inspect_path の `TargetPath::Symlink` の `real_path`）で決まる。これが撤回された前の記録の A4 の実装で、辿れない連鎖を root_dir の中なら機密として扱う。A7 に合わせ、`return inside_root` の二か所（絶対パスのリンク文字列を root_dir で剥がせないときと、要素を畳む途中で root_dir より上に出るなどして `_` の腕に落ちるとき）を両方 false を返すように変え、`entry_inside_root` とそれを求める処理を取り除く。字面で辿る規則そのもの（A10）、各段でリンク文字列と実パスを機密パターンに当てること、inspect_path が失敗したとき false を返すことは変えない。関数の説明のコメントも、辿れないときに機密として扱うという記述を A7 に合わせて直す。

変更は diff の判定の関数一つに閉じ、ローカル・SSH・エージェントの三つの経路の inspect_path には触らない（A6 で三つの経路の作り直しをやめたため）。

テストは A12 に従う。`tests/cli_diff.rs` の `assert_unfollowable_chain_hides_contents` を使う 6 本（`chain_link_through_the_parent_of_the_local_root_dir_hides_contents` から `chain_link_through_an_alias_of_the_remote_root_dir_hides_contents` まで）は撤回した決定を確かめており、変更の後は必ず失敗するため、補助の関数 `assert_unfollowable_chain_hides_contents` と列挙 `ChainLinkText` と一緒に消す（`ChainLinkText` を使うのはこの補助だけ）。新しいテストの補助は別に書く。代わりに EX-cli-067（リンク文字列が "../<root_dir の実ディレクトリ名>/target.txt" の形）と EX-cli-068（root_dir を指す別名の symlink を通る絶対パス）の印付きのテストを、ローカルの側と SSH の側の両方について足す。消す 6 本の組み立て（`CliEnv`、root_dir のディレクトリ名、`temp_root()` の下の別名の symlink）が手本になる。機密の名前を含む連鎖が字面で辿れないときに内容が表示されることはテストで固定しない。

新しいテストは例ごとに一本とし、左（ローカル）と右（試験 SSH サーバ）の両側に、それぞれの側の root_dir の中の target.txt を指す symlink link.txt を、例の形のリンク文字列で置く（左右の target.txt の中身は違える。リンク文字列の root_dir のディレクトリ名や別名はそれぞれの側のものにする）。--force なしで link.txt を diff し、左右の target.txt の中身がどちらも出力に出て、終了コードが 1（差分あり）であることを確かめる。一本でローカルと SSH の両方の経路を通る。印はテストの直前に `// @kotowari[EX-cli-067]`（または `// @kotowari[EX-cli-068]`）の形で、例の ID だけを付ける（手本は `link_text_with_a_dot_component_still_shows_changed_contents` の `// @kotowari[EX-cli-040]`）。変更の前は両側の中身が隠れるため、このテストは落ちる。

変異テストは、変えた関数 `sensitive_link_chain` に絞って一度だけ回す（`docs/decision/records/2026-09-29-mutation-rerun-and-load.md` の決め方）。cargo-mutants 27.1.0 は `--re` を名前に正規表現として当て、関数の戻り値を丸ごと置き換える変異は "replace sensitive_link_chain -> bool with ..." の形で " in sensitive_link_chain$" に当たらないため、`--re ' in sensitive_link_chain$'` と `--re 'replace sensitive_link_chain -> '` の二つを付けて `src/cli/diff.rs` を渡す。回す前に同じ `--re` とファイルで `cargo mutants --list` を実行し（systemd-run のメモリ上限と CPUWeight=idle・Nice=19 の中で実行してよい）、`sensitive_link_chain` の外の変異が混ざっていないことを確かめる。構造体のフィールドを消す変異は `--re` で除かれずに混ざることがあるため（`docs/decision/records/2026-09-28-mutation-scope.md#A3`）、混ざったら件数と見逃しから分けて記録だけにする。

## Scope of change

- `src/cli/diff.rs`
  - `sensitive_link_chain` とその説明のコメントだけ
- `tests/cli_diff.rs`
  - 6 本のテストとその補助の削除、EX-cli-067・EX-cli-068 のテストの追加
- `.kotowari/mutants-equivalents.yaml`（同等変異の登録が要るときだけ）
- `docs/testing/diff-sensitive-chain.md`（新規。変えたこと、消したテストと足したテスト、変異テストの結果の記録）

## Step order and prerequisites

S1 の後に S2、S2 の後に S3。S2 の変異テストは S1 の変更を対象にし、S3 は全体の確かめなので最後に置く。S1 を始める前に、ブランチ `fix/diff-sensitive-chain` が決定記録のコミット 718e848 を含むことを確かめる。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-cli-023 | EX-cli-067, EX-cli-068 |
| S2 | REQ-cli-023 | EX-cli-067, EX-cli-068 |
| S3 | REQ-cli-023 | EX-cli-067, EX-cli-068 |

REQ-cli-023 の印付きのテストのうち、消す 6 本以外（`sensitive_chain_link_under_a_local_root_dir_through_a_symlink_hides_contents` など字面で辿れる機密の連鎖を確かめるもの）は変えずに残し、変更の後も通ることを確かめる。

## Left to the implementer

- 新しいテストの名前と、新しく書く補助の関数の組み立て方
- `sensitive_link_chain` の中の書き方（辿れなくなったときに false を返すことが保たれる限り）
- コミットの分け方。ただし pre-commit のフックが fmt・clippy・全テストを流すため、落ちるテストを含むコミットはできない。コミットできる並びは「6 本とその補助の削除」の後に「関数の変更と EX-cli-067・EX-cli-068 のテスト」を一緒にコミットするか、全てを一つのコミットにするかのどちらか。`--no-verify` は使わない

## Stop conditions

- EX-cli-067 か EX-cli-068 のテストが、変更の前から通る（字面で辿れない形になっていないか、別の理由で内容が出ている）
- 変更の後に、消す 6 本以外の既存のテストが落ちる
- SSH の側で、例の形のリンク文字列を試験 SSH サーバに置けない、または置いても inspect_path が別の結果を返す
- 変異テストで `sensitive_link_chain` の見逃しが残り、決定記録にない振る舞い（要件が決めていない挙動）を固定しないと殺せない

## Test command

```sh
cargo nextest run --all-features
```

cargo のテストとビルドは systemd-run のメモリ上限（MemoryMax=40%、MemorySwapMax=0）と CPUWeight=idle・Nice=19 の中で実行する。形は `scripts/mutants.sh` の systemd-run の呼び出しにそろえ、次のように実行する（mise の shim が使う版を決める変数を渡す。値が空の変数は渡さなくてよい。認証情報を含む変数は渡さない）。

```sh
systemd-run --user --wait --pipe --quiet --same-dir \
  --setenv=PATH="$PATH" --setenv=RUSTUP_TOOLCHAIN="$RUSTUP_TOOLCHAIN" --setenv=RUSTUP_HOME="$RUSTUP_HOME" \
  --setenv=CARGO_HOME="$CARGO_HOME" --setenv=MISE_GLOBAL_CONFIG_FILE="$MISE_GLOBAL_CONFIG_FILE" \
  -p MemoryMax=40% -p MemorySwapMax=0 -p CPUWeight=idle -p Nice=19 \
  cargo nextest run --all-features
```

一つのテストだけを流すときは、最後の `cargo nextest run --all-features` の後にテストの名前を付ける。変異テストは `scripts/mutants.sh` から実行し、cargo-mutants を直接実行しない（例外は Approach and why の `cargo mutants --list` だけで、これも上と同じ systemd-run の中で実行してよい）。コミットは `nice -n 19 git commit` で行う（pre-commit のフックが fmt・clippy・全テストを流す。これまでのコミットと同じやり方）。

## Out of scope

- ローカル・SSH・エージェントの三つの経路の inspect_path とエージェントのプロトコル
- 字面で辿る規則の変更、root_dir の外を通る連鎖や OS の解決と字面がずれる書き方の迂回（A6 で受け入れた限界）
- 指定したパスや子パスの途中の要素にあるディレクトリ symlink の判定（A11）
- status・merge・sync・TUI の機密の扱い（A5）
- 消す 6 本と新しいテスト以外の既存のテストの書き換え

## Steps

### S1: 字面で辿れない連鎖を機密でないとみなす

- Purpose: 機密の名前を含まない連鎖が字面で辿れない形でも、--force なしで参照先の内容差を表示するようにする
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-023`
- Prerequisites: ブランチ `fix/diff-sensitive-chain` が 718e848 を含む
- May change: `src/cli/diff.rs`, `tests/cli_diff.rs`
- Done when: EX-cli-067 と EX-cli-068 の印付きのテスト（どちらも左がローカル、右が SSH）が、変更の前に落ち、変更の後に通る。`sensitive_link_chain` に `entry_inside_root` がなく、辿れなくなったときに false を返す。撤回した決定を確かめる 6 本とその補助がない。ほかの既存のテストは全て通る
- Shown by: test — EX-cli-067 と EX-cli-068 の印付きのテストを書いて関数を変える前に Test command の形で実行し、落ちる出力を RED の証拠にする（コミットはしない）。関数を変えて通ることを GREEN とし、Test command で全体を流す
- Left to the implementer: テストの名前と補助の組み立て方、関数の中の書き方
- Stop and hand back if: 新しいテストが変更の前から通る、6 本以外の既存のテストが変更の後に落ちる、SSH の側で例の形のリンクを置けない

### S2: 変えた関数の変異テストを回して記録する

- Purpose: `sensitive_link_chain` の変異が、残ったテストと新しいテストで検知されることを確かめ、結果を記録する
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-023`
- Prerequisites: S1 がコミット済みで、作業ツリーに src/ と tests/ の変更がない
- May change: `tests/cli_diff.rs`（見逃しを殺すテストの追加だけ）, `.kotowari/mutants-equivalents.yaml`, `docs/testing/diff-sensitive-chain.md`
- Done when: `docs/testing/diff-sensitive-chain.md` に、変えたこと、消したテストと足したテストの一覧、`cargo mutants --list` の件数、`scripts/mutants.sh` の結果と、見逃しが 0 であること（または同等変異として登録したものとその理由）がある
- Shown by: check — `cargo mutants --list --re ' in sensitive_link_chain$' --re 'replace sensitive_link_chain -> ' --file src/cli/diff.rs` で件数を確かめてから、`scripts/mutants.sh --re ' in sensitive_link_chain$' --re 'replace sensitive_link_chain -> ' src/cli/diff.rs` を実行する
- Left to the implementer: 記録の書き方（`docs/testing/diff-root-symlink-fix.md` を手本にする）
- Stop and hand back if: 見逃しを殺すのに、決定記録と要件が決めていない振る舞いを固定するテストが要る、変異テストがメモリ上限で止まる

### S3: 計画の範囲の確かめ

- Purpose: 計画が扱う要件と例に印付きのテストがあり、変えたファイルに kotowari の指摘がないことを確かめる
- Specification: `docs/ir/cli/symlink-diff.md#REQ-cli-023`
- Prerequisites: S2
- May change: なし
- Done when: `kotowari query EX-cli-067` と `kotowari query EX-cli-068` の tests が空でなく、`kotowari check --format json` に、このブランチで変えたファイルと REQ-cli-023・EX-cli-067・EX-cli-068 についての error がない
- Shown by: check — `kotowari query EX-cli-067 | jq -c '.items[0].tests'`、`kotowari query EX-cli-068 | jq -c '.items[0].tests'`、`kotowari check --format json | jq -c '.findings[] | select(.severity == "error")'`
- Left to the implementer: none
- Stop and hand back if: このブランチの前からある指摘以外の error が出る
