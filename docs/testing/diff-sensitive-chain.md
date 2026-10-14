# CLI diff の字面で辿れない機密の連鎖の扱いの修正の記録

CLI diff の機密の連鎖の判定（`sensitive_link_chain`）で、段を root_dir の下に字面で辿れなくなったときに、連鎖の最初の段の実パスが root_dir の中なら機密として内容を隠していた扱いをやめ、機密でないとみなすようにした過程の記録。
判断は[決定記録](../decision/records/2026-09-30-diff-sensitive-chain.md)の A6（判定は字面で辿れる範囲で行う）、A7（確かめられない段は機密でないとみなす）、A8（[前の記録の A4](../decision/records/2026-09-30-diff-root-symlink.md#A4) の撤回）、A12（テストの扱い）による。
要件は [REQ-cli-023](../ir/cli/symlink-diff.md#REQ-cli-023) の判定の範囲の書き足しと、例 EX-cli-067・EX-cli-068 の追加。

## 変えたこと

`src/cli/diff.rs` の `sensitive_link_chain` で、絶対パスのリンク文字列を実パスに直した root_dir と設定に書いた root_dir のどちらでも前置きを外せないときと、要素を畳む途中で root_dir より上に出るときに返していた `inside_root` を、どちらも false にした。
`inside_root` を求めていた `entry_inside_root` とその計算を取り除き、関数の説明のコメントを直した。
字面で辿る規則、各段でリンク文字列と実パスを機密パターンに当てること、inspect_path が失敗したときに false を返すことは変えていない。

## 消したテスト

撤回した前の記録の A4 を確かめていた次の 6 本と、その補助の関数 `assert_unfollowable_chain_hides_contents` と列挙 `ChainLinkText`（tests/cli_diff.rs）を消した。
どれも機密の名前（secret.pem）を含む連鎖で、変更の後は内容が表示されるため必ず失敗する。
機密の名前を含む連鎖が字面で辿れないときに内容を表示することは、A12 のとおりテストで固定していない。

| テスト | 側 | a.txt のリンク文字列 |
|---|---|---|
| chain_link_through_the_parent_of_the_local_root_dir_hides_contents | ローカル | "../<root_dir のディレクトリ名>/mid" |
| chain_link_through_the_parent_of_the_remote_root_dir_hides_contents | SSH | 同上 |
| chain_link_through_dot_dot_after_the_local_real_root_hides_contents | ローカル | "<root_dir の実パス>/../<root_dir のディレクトリ名>/mid" |
| chain_link_through_dot_dot_after_the_remote_real_root_hides_contents | SSH | 同上 |
| chain_link_through_an_alias_of_the_local_root_dir_hides_contents | ローカル | root_dir を指す別名の symlink を通る絶対パス "<別名>/mid" |
| chain_link_through_an_alias_of_the_remote_root_dir_hides_contents | SSH | 同上 |

字面で辿れる機密の連鎖を確かめる REQ-cli-023 の既存のテスト（sensitive_chain_link_under_a_local_root_dir_through_a_symlink_hides_contents など）は変えずに残し、変更の後も通った。

## 足したテスト

どちらも左（ローカル）と右（試験 SSH サーバ、エージェントは無効）の両側に、その側の root_dir の中の target.txt を指す link.txt を例の形のリンク文字列で置き（左右の target.txt の中身は違える）、--force なしで link.txt を diff して、左右の target.txt の中身がどちらも出て終了コードが 1 になることを確かめる。
補助は `assert_unfollowable_link_shows_contents`。

| テスト（tests/cli_diff.rs） | 印 | link.txt のリンク文字列 |
|---|---|---|
| link_through_the_real_name_of_the_root_dir_shows_contents | `// @kotowari[EX-cli-067]` | "../<root_dir のディレクトリ名>/target.txt" |
| link_through_an_alias_of_the_root_dir_shows_contents | `// @kotowari[EX-cli-068]` | root_dir を指す別名の symlink（一時ディレクトリの "<root_dir のディレクトリ名>-alias"）を通る絶対パス "<別名>/target.txt" |

変更の前は、二つとも "Content hidden (sensitive file). Use --force to show." と出て中身の確かめで落ちた（2 tests run: 0 passed, 2 failed）。SSH の側にも例の形のリンク文字列を置けたことは、出力のリンク文字列（"../remote/target.txt" と "<一時ディレクトリ>/remote-alias/target.txt"）で確かめた。
変更の後は二つとも通り、コミット 7220bcf の pre-commit で全テスト（3097 件）が通った。

## 変異テスト

コミット 7220bcf で、変えた関数に絞って回した。

```sh
scripts/mutants.sh \
  --re ' in sensitive_link_chain$' \
  --re 'replace sensitive_link_chain -> ' \
  src/cli/diff.rs
```

事前の `cargo mutants --list` は 7 件で、全て `sensitive_link_chain` のもの（戻り値を true と false に置き換える 2 件、`||` を `&&` にする 1 件、Normal と CurDir の腕を消す 2 件、`next.pop()` のガードを true と false にする 2 件）だった。構造体のフィールドを消す変異は混ざらなかった。
結果は `mutants: caught=7 survived=0 timeout=0 unviable=0 equivalent=0`（約 4 分）で、決着の対象になる見逃しはない。
