# diff の参照先と競合のテスト整理の記録

diff の --ref の参照先と競合の取り込みで加えた要件（REQ-cli-062 から REQ-cli-066）と、既存の要件 REQ-cli-016 の根拠テストを整理した過程の記録。
変異テストの結果、各要件の根拠にしたテスト、削除候補と利用者の判断、整理後の見逃しの決着を残す。

## 整理前の変異テスト

整理を始める前のコミット b5355c6（作業ツリーに変更のない状態）で、次のコマンドを実行した。
参照先と競合の規則を持つ関数に絞り、四つのファイルを一回の実行にまとめ、同じ条件（`--all-features`、テストの実行は cargo nextest、並列数 2）で整理後と比べられるようにした。
実行中は作業ツリーに触れていない。

```sh
scripts/mutants.sh \
  --re ' in (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source)$' \
  --re 'replace (build_diff_output|detect_conflicts|extract_changes|merge_overlapping_regions|merge_ranges|validate_ref_side|resolve_ref_source) -> ' \
  src/service/diff.rs src/diff/conflict.rs src/cli/ref_guard.rs src/service/source_pair.rs
```

回す前に同じ `--re` を付けた `cargo mutants --list --all-features --file src/service/diff.rs --file src/diff/conflict.rs --file src/cli/ref_guard.rs --file src/service/source_pair.rs` で件数を確かめ、計画と同じ 93 件だった。
構造体のフィールドを消す変異は混ざらなかった。

全体の集計は `mutants: caught=57 survived=19 timeout=4 unviable=13 equivalent=0`（93 件、実行時間は約 26 分）。
スクリプトの終了コードは 1 で、これは cargo-mutants が最後まで走った後に kotowari mutants が見逃しを error として返したもの（93 件の全てに結果がある）。
一度目の起動は、呼び出し側の待ち時間の設定の誤りに気づいて数秒で止め、結果を読まずに捨てた。上の集計は二度目の起動のもの。
関数ごとの内訳は cargo-mutants の結果ファイル（`outcomes.json`）から数えた。

| ファイル | 関数 | caught | survived | timeout | unviable |
|---|---|---|---|---|---|
| src/service/diff.rs | build_diff_output | 5 | 2 | 0 | 1 |
| src/diff/conflict.rs | detect_conflicts | 17 | 13 | 0 | 0 |
| src/diff/conflict.rs | extract_changes | 28 | 1 | 4 | 3 |
| src/diff/conflict.rs | merge_overlapping_regions | 2 | 2 | 0 | 0 |
| src/diff/conflict.rs | merge_ranges | 0 | 1 | 0 | 7 |
| src/cli/ref_guard.rs | validate_ref_side | 3 | 0 | 0 | 1 |
| src/service/source_pair.rs | resolve_ref_source | 2 | 0 | 0 | 1 |

タイムアウトの四件は全て src/diff/conflict.rs の extract_changes の `replace += with *=`（120・129・162・180 行）で、添字が進まずに止まらなくなる変異である。

### 見逃し

位置は変異が入る行、変異は kotowari mutants の出力の文言のまま。
「決着の対象」は計画の区別による。FLAG-cli-058 から 070 に当たる見逃しはその FLAG の範囲として記録だけにする。

| 位置 | 変異 | 決着の対象 |
|---|---|---|
| src/diff/conflict.rs:153 | replace + with * in extract_changes | 対象 |
| src/diff/conflict.rs:219 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with == in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace < with > in detect_conflicts | 対象 |
| src/diff/conflict.rs:224 | replace >= with < in detect_conflicts | 対象 |
| src/diff/conflict.rs:225 | replace == with != in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace && with \|\| in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with == in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace < with > in detect_conflicts | 対象 |
| src/diff/conflict.rs:227 | replace >= with < in detect_conflicts | 対象 |
| src/diff/conflict.rs:230 | replace < with <= in detect_conflicts | 対象 |
| src/diff/conflict.rs:265 | replace < with <= in merge_overlapping_regions | 対象 |
| src/diff/conflict.rs:265 | replace < with == in merge_overlapping_regions | 対象 |
| src/diff/conflict.rs:286 | replace merge_ranges -> Option<Range<usize>> with None | 対象 |
| src/service/diff.rs:47 | delete match arm engine::DiffResult::Equal in build_diff_output | 対象（参照先との差の組み立て） |
| src/service/diff.rs:68 | replace && with \|\| in build_diff_output | 対象外（バイナリと symlink の競合。FLAG-cli-059・063 の範囲） |

- detect_conflicts の 219 行から 227 行は、左右の一方か両方が挿入だけ（参照先の行を消さない変更）のときに重なりを決める分岐で、整理前のテストは両方が同じ位置に挿入する場合と空の参照先に両方が足す場合しか通らない。230 行は両方が参照先の行を変えるときの重なりの判定で、`<=` にすると隣り合うだけの変更も重なりと見る。
- src/diff/conflict.rs:153 は挿入だけの変更の位置（直前の参照先の行の次）の計算である。
- merge_overlapping_regions と merge_ranges は、重なる競合の範囲を一つにまとめる処理である。merge_ranges の結果は "conflict_regions" の要素の中の TUI 用の範囲に入り、要素の形は FLAG-cli-058 の範囲のため、決着の仕方は整理後に判断する。
- src/service/diff.rs:47 は、左と参照先が同じときに "ref_hunks" を空の配列にする分岐で、この腕を消しても後ろの `_ => Some(vec![])` の腕が同じ値を返すため、同等変異の候補である。
