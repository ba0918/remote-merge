# CLI diff の symlink を経由する root_dir の修正の記録

設定の root_dir が symlink を経由するとき、diff が root_dir の中の通常のファイルまで root_dir の外として比べなかった不具合を直した過程の記録。
判定の基準は[決定記録 A1](../decision/records/2026-09-30-diff-root-symlink.md#A1)のとおり、root_dir を側ごとにファイルと同じ inspect_path で実パスに直した場所にした。
要件は [REQ-cli-026](../ir/cli/symlink-diff.md#REQ-cli-026) で、変えていない。

## 足したテスト

どれも `// @kotowari[REQ-cli-026]` を付け、左右の file.txt の中身を違え、root_dir の中のファイルの差分（hunks に左右の中身）が出て、"outside root_dir" の報告がなく、終了コードが 1 であることを確かめる。

| テスト | 経路 | 構成 |
|---|---|---|
| local_root_dir_through_a_symlink_compares_files_inside_it（tests/cli_diff.rs） | ローカル | 左の root_dir を実ディレクトリへの symlink にして file.txt を指定 |
| remote_root_dir_through_a_symlink_compares_files_inside_it（tests/cli_diff.rs） | SSH（エージェントは無効） | 右の root_dir を実ディレクトリへの symlink にして file.txt を指定 |
| link_inside_a_root_dir_through_a_symlink_is_followed_without_follow_flag（tests/cli_diff.rs） | ローカルと SSH | 左右それぞれで root_dir を symlink にし、root_dir の中の file.txt を指す link.txt を指定 |
| remote_root_dir_through_a_symlink_compares_files_inside_it_via_the_agent（tests/contract/diff_root_link_cli.rs） | エージェント | 右の root_dir を symlink にして file.txt を指定 |

tests/cli_diff.rs のテストは、既存の `TestDirs` の試験 SSH サーバを使い、設定を一時ディレクトリに書いて `assert_isolated_config_at` を通してから `--config` で渡す。
エージェントの経路は scan_listing_cli の `AgentFixture` に diff を起動する `diff_via_agent` を足して使った（status の起動の挙動は変えていない）。
パスの確かめがエージェントを通ったことは、試験サーバが受けたコマンドにエージェントの起動があり、SSH の経路のパスの確かめのコマンド（`resolve_existing_prefix` を含む）がないことで確かめる。

修正の前は、四つとも "content not compared (outside root_dir; use --follow-external-links)" のエラーと空の hunks で落ちた。
エージェントの経路のテストは修正の後に書いたため、修正の前の src/cli/diff.rs に一時的に戻して落ちることを確かめた。

## root_dir がないときの扱い

root_dir がない（inspect_path が Missing を返す）ときは、設定に書いた root_dir をそのまま基準にする。
A1 はこの場合を決めていないため、修正の前と挙動が変わらないことだけを確かめた。
root_dir を存在しない場所を指す symlink にして、左右それぞれで修正の前と後の実行ファイルの出力を比べると同じだった（ローカルは "No such file or directory" のエラー、SSH は "outside root_dir" のエラーで、どちらも終了コード 2）。

## 変異テスト

修正のコミット 6d8450d で、変えた関数と足した関数に絞って一度回した（作業ツリーの変更は、この記録の新しいファイルだけ）。

```sh
scripts/mutants.sh \
  --re ' in (resolved_path_outside_root|path_escapes_root|real_root_dir)$' \
  --re 'replace (resolved_path_outside_root|path_escapes_root|real_root_dir) -> ' \
  src/cli/diff.rs
```

実行の前に同じ `--re` とファイルで `cargo mutants --list` をメモリ上限と低い CPU 優先度の中で実行すると 7 件だった（real_root_dir 1 件、resolved_path_outside_root 4 件、path_escapes_root 2 件）。
結果は `mutants: caught=7 survived=0 timeout=0 unviable=0 equivalent=0`（約 3 分）で、決着の対象になる見逃しはない。

`execute_diff` の中の symlink の参照先が root_dir の外かを見る部分も、実パスに直した root_dir を使うように変えたが、500 行を超える関数全体に広げないため、この回では回していない。
この部分の腕を消す変異は、既に同等変異として登録されている（.kotowari/mutants-equivalents.yaml）。登録の理由（同じループの先頭の確かめが同じパスと同じ root_dir で同じ比較をする）は、どちらも実パスに直した root_dir を使うようになった後も成り立つ。

## 機密の連鎖の判定の修正

### 指摘

上の修正の後のレビューで、機密の連鎖の判定（`sensitive_link_chain`）が漏れの原因になりうるという指摘を受けた。
この判定は、連鎖の段のリンク文字列が絶対パスのとき、設定に書いた root_dir の文字列で前置きを外して次の段を辿り、外せなければ機密ではないとして辿るのをやめていた。
root_dir が symlink を経由し、リンク文字列が実パスの形の絶対パスで書かれていると前置きを外せず、途中の段の名前が機密パターンに当たっても見ない。
上の修正の前はこのファイルを root_dir の外として比べなかったが、修正の後は中として比べるため、--force なしの diff で最終参照先の中身が出る。
これは [REQ-cli-023](../ir/cli/symlink-diff.md#REQ-cli-023)（入れ子の各段のリンク文字列が機密パターンに当たれば、root_dir の内外を問わず --force なしでは中身を表示しない）に反する。

### テストと RED

symlink にした側の root_dir の中だけに、連鎖 a.txt → "<root_dir の実パス>/mid" → "secret.pem" → "plain.txt" を置き、もう一方の側の a.txt は通常のファイルにして、--force なしの JSON で diff する。
標準出力に plain.txt の中身の目印が出ないことと、a.txt の項目が出ることを確かめる（何も出さずに失敗した場合と区別するため）。文言と終了コードの値は IR が定めないため確かめない。

| テスト（tests/cli_diff.rs、`// @kotowari[REQ-cli-023]`） | 構成 |
|---|---|
| sensitive_chain_link_under_a_local_root_dir_through_a_symlink_hides_contents | 左（ローカル）の root_dir を symlink にし、リンク文字列を実パスの形にする |
| sensitive_chain_link_under_a_remote_root_dir_through_a_symlink_hides_contents | 右（SSH、エージェントは無効）の root_dir を symlink にし、リンク文字列を実パスの形にする |
| sensitive_chain_link_written_through_the_configured_root_dir_hides_contents | 左右それぞれで、リンク文字列を設定に書いた root_dir（symlink）の形にする |

修正の前（コミット 928fa99）は、上の二つが目印の確かめで落ちた。どちらも a.txt の項目の "sensitive" が false で、hunks に目印の行が出ており、漏れが実際に起きることを確かめた。

### 修正

`sensitive_link_chain` は、絶対パスのリンク文字列の前置きを、実パスに直した root_dir（execute_diff で側ごとに求めたもの）で外し、外せなければ設定に書いた root_dir で外す。どちらかで外せれば次の段を辿る。
設定に書いた root_dir の形のリンク文字列は修正の前から辿れていた。三つ目のテストは、この扱いが修正の後も残ることを確かめるために後から足したもので、足したときから通った。このテストが設定の root_dir で外す部分を壊すと落ちることは、その部分を一時的に消して確かめてはいない（安全の確かめを一時的にも弱めないため）。下の変異テストもこの部分の変異を作らない。

### 変異テスト

修正のコミット 76a46f8 で、`sensitive_link_chain` と、それが使う `real_root_dir` に絞って回した（`execute_diff` の変更は呼び出しの引数だけのため含めていない）。

```sh
scripts/mutants.sh \
  --re ' in (sensitive_link_chain|real_root_dir)$' \
  --re 'replace (sensitive_link_chain|real_root_dir) -> ' \
  src/cli/diff.rs
```

事前の `cargo mutants --list` は 8 件（sensitive_link_chain 7 件、real_root_dir 1 件）だった。
結果は `mutants: caught=8 survived=0 timeout=0 unviable=0 equivalent=0`（約 4 分）で、決着の対象になる見逃しはない。
cargo-mutants は `.or_else(|_| candidate.strip_prefix(root))` の中の変異を作らなかった。この部分は上の三つ目のテストで確かめる。
