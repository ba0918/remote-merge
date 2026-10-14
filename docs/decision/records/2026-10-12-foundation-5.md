# 土台の段 5: 各コマンドを比べる流れに載せ替え、古い実装を消す

## Context

[土台の作り直し](./2026-10-12-foundation.md) の最後の段である。段 4 で、比べる流れ（両側の状態 → 判定 → 必要な分だけ読む → もう一度判定）を純粋な関数と経路の事実の集め方で作ったが、diff・status・merge・sync・rollback はまだ古い比べる実装（古い木、コマンドごとの判定と読み込み、その他の種類の暫定の外し方）で動いている。段 5 では各コマンドをこの流れに載せ替え、古い実装を消し、段 4 で段 5 に送った要件・例・出力の変更を CLI の出力で示す。[foundation A10](./2026-10-12-foundation.md#A10) のとおり、初めに段 5 が扱う IR（cli・merge・backup と、比べる流れのうちコマンドに出る部分）を読み直す。最後に、品質レビュー 1 回目の台帳の項目が消えたかを確かめる（[foundation A7](./2026-10-12-foundation.md#A7)）。

Position: 段 5 の仕様と計画を決めた（F1〜F20）。その後、利用者がファイルの操作の経路をローカルとエージェントの二つにすると決め（[foundation A12](./2026-10-12-foundation.md#A12)）、それに合わせて F23〜F29 を決めた。実装は [段 5 の計画](../../plans/foundation-5.md) の S0 から。

## Agreements

- F1 段 5 が扱う IR のうち、経路の名前を挙げて同じことを繰り返す文から経路の名前を外す。新しいディレクトリの権限（[REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012)）、新しく作るものの持ち主（[REQ-merge-042](../../ir/merge/permissions.md#REQ-merge-042)）、新しいファイルを作る書き込み（[REQ-merge-043](../../ir/merge/concurrency.md#REQ-merge-043)）、root_dir が消えたときの止め方（[REQ-merge-044](../../ir/merge/concurrency.md#REQ-merge-044)）、退避した記録を読み戻せないとき（[REQ-backup-046](../../ir/backup/rollback-path.md#REQ-backup-046)）の「どの経路でも」と、消えた削除の対象（[REQ-merge-041](../../ir/merge/concurrency.md#REQ-merge-041)）の「リモートの削除を使う」、比べるための読み込み（[REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011)）の「経路ごとに」である。経路によらないことは [REQ-paths-008](../../ir/paths/routes.md#REQ-paths-008)・[REQ-paths-013](../../ir/paths/operations.md#REQ-paths-013)・[REQ-compare-002](../../ir/compare/flow.md#REQ-compare-002) に任せる。REQ-merge-042 の「（ローカルの経路では手元の利用者）」は、用語 `ログインした利用者`（操作を行うアカウント。リモートではつないだ利用者、ローカルではコマンドを動かした利用者）を用語集に置いて言い換える。
  - why: [段 3 D12](./2026-10-12-foundation-3.md#D12) と [段 4 E17](./2026-10-12-foundation-4.md#E17) が段 5 に残したものと、段 3 の後に足した要件に同じ書き方が入ったものである。振る舞いは変えない。
  - decided_by: 利用者（[foundation A10](./2026-10-12-foundation.md#A10)・[段 3 D12](./2026-10-12-foundation-3.md#D12) から導いた。自走の指示により書き下ろした）
- F2 [REQ-merge-043](../../ir/merge/concurrency.md#REQ-merge-043) の「エージェントが分けて送られた中身を書くときは、後の分も最初の分で開いたファイルに書き」を、経路によらない振る舞い「書き込みは、書き込み先を開いてから書き終えるまで、最初に開いたファイルに書き続け、書いている間にパスが別のファイルに差し替えられても差し替えたファイルに書かない」に書き直す。
  - why: 分けて送るのはエージェントの通信の仕組み（[段 3 D38](./2026-10-12-foundation-3.md#D38)）で、利用者に見える振る舞いは三つの経路とも「最初に開いたファイルに書き続ける」である（ローカルと SSH の経路は一度開いて書き終える）。仕組みの名前で書くと、ほかの経路の振る舞いが決まっていないように読める。
  - decided_by: Claude（[foundation A10](./2026-10-12-foundation.md#A10)・[段 3 D38](./2026-10-12-foundation-3.md#D38) から導いた。自走の指示により決めた）
- F3 古い木の構造を写した言葉を、比べる流れの言葉に書き直す。[REQ-merge-036](../../ir/merge/permissions.md#REQ-merge-036)・[REQ-merge-037](../../ir/merge/permissions.md#REQ-merge-037) の「走査したツリーの値」「走査したツリーに権限の値がない」は「状態を集めたときに走査で得た権限の値」「その値がない」に、[REQ-merge-035](../../ir/merge/basic.md#REQ-merge-035)・[REQ-cli-071](../../ir/cli/sync.md#REQ-cli-071) の「接続かツリーの取得に失敗」は「接続か状態の収集（走査）に失敗」にする。
  - why: 段 5 で古い木を消すので、IR がそれを前提にした言い方のままだと、合格の基準が消える実装を指す。意味は変えない。
  - decided_by: Claude（[foundation A10](./2026-10-12-foundation.md#A10) の「今の実装の都合を書いた文」に当たるため。自走の指示により決めた）
- F4 次のものは書き直さない。[REQ-cli-070](../../ir/cli/status-output.md#REQ-cli-070)（-v で右の Agent の接続状態を "connected" か "fallback" で示す）は、経路で振る舞いを分けるのではなく、選んだ経路を利用者に知らせる出力そのものなので残す。経路の名前を挙げる例（EX）は、経路で同じになることを確かめる例なので残す（[段 4 E17](./2026-10-12-foundation-4.md#E17) と同じ）。「文言は IR で決めず、実装者が決める」の文は、決めないことを決めた文で今の実装の都合ではないので残す。[REQ-cli-078](../../ir/cli/diff-output.md#REQ-cli-078)・[REQ-merge-007](../../ir/merge/symlink.md#REQ-merge-007)・[REQ-cli-096](../../ir/cli/directory-paths.md#REQ-cli-096) は段 4 で経路の名前を外し終えている。[REQ-compare-002](../../ir/compare/flow.md#REQ-compare-002) の「ローカル・SSH・エージェントのどの経路でも」は、経路によらないことを述べる要件そのものなので残す。[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001) の「ただし、ディレクトリと、ディレクトリへの symlink の組は」は、判定（種類の違い）を merge・sync がどう扱うかの写し方で、[段 4 E2](./2026-10-12-foundation-4.md#E2) が判定と扱いを分けると決めた形どおりなので残す。
  - why: [foundation A10](./2026-10-12-foundation.md#A10) は、書き直さないときは理由を残すことを求める。
  - decided_by: Claude（foundation A10・[段 4 E2](./2026-10-12-foundation-4.md#E2)・[段 4 E17](./2026-10-12-foundation-4.md#E17) から導いた。自走の指示により決めた）
- F5 判定できない理由の文を一つの形にする。読み込みの失敗は [REQ-compare-014](../../ir/compare/reading.md#REQ-compare-014) のとおり "read failed: " に続けて失敗した側ごとの "側の名前: 原因" を "; " でつなぎ、その他の種類（[REQ-compare-009](../../ir/compare/judgement.md#REQ-compare-009)）は "not a regular file: " に続けて、その他の種類だった側の名前を同じ順に "; " でつなぐ。側の名前は "left"・"right"・"ref" の三つで、この順に並べる。rollback は戻す先の一つの側だけを比べるので、側の名前を "target" とする。rollback が書き戻す前の内容を退避するための読み込みの失敗は、今までどおり [REQ-backup-027](../../ir/backup/rollback-path.md#REQ-backup-027) の "backup failed: " の形にする（同じ読み込みの失敗に二つの形を当てない）。二つの symlink を辿った先の組（[REQ-compare-008](../../ir/compare/judgement.md#REQ-compare-008)）も同じ形にする。サーバの名前（"develop: cannot read …"）や参照先のラベルを側の名前に使う今の三つの形はやめる。
  - why: 段 4 で古いコマンドに入れたその他の種類の理由は、左右は "not a regular file"、参照先は "参照先の名前: not a regular file"、読み込みの失敗は "側の名前: cannot read …" で、段 4 の計画が段 5 に送ったとおり揃っていない（引き継ぎは [段 5 の計画](../../plans/foundation-5.md) の Verification map にある）。どちらの理由も、どの側の問題かが分からないと利用者は直せない。その他の種類は中身を読まないので、"read failed" の見出しは使わない。今の例（EX-compare-010・011）は理由が "not a regular file" で始まることを確かめているので、この形でも成り立つ。
  - decided_by: Claude（[REQ-compare-014](../../ir/compare/reading.md#REQ-compare-014) と [段 4 E7](./2026-10-12-foundation-4.md#E7) から導いた。自走の指示により決めた）
- F6 コマンドは status → diff → merge → sync → rollback の順に載せ替える。初めに、行の競合の数え方（[REQ-cli-098](../../ir/cli/conflicts.md#REQ-cli-098)）を今の diff の出力でテストに固める。古い実装は、それを使う最後のコマンドを載せ替えた段で消す（status だけが使うものは status の段、diff だけが使うものは diff の段、merge と sync が共有するものは sync の段、rollback だけが使うものは rollback の段、どのコマンドにも属さない構造の残り（古い木とその他の種類の暫定の扱い・Side 単位の文字列のパスの API）は最後の片付けの段）。どの段の終わりにも全テストが通る。
  - why: status と diff は読むだけなので、書き込みを変える前に、流れの判定から CLI の出力への訳し方を確かめられる。merge と sync は書き込みと変更の直前の判定を共有し、sync は merge の後なら薄い段になる。rollback は書き込みの部品と段 3 の書き戻しの確かめを使うので最後に置く。最後の使い手の段で消せば、新旧の二重の実装を残さず（[foundation A7](./2026-10-12-foundation.md#A7)）、各段でテストが通る。行の競合の数え方は比べる流れではなく行の差分の数え方なので（[段 4 E27](./2026-10-12-foundation-4.md#E27)）、載せ替えの前に今の出力で固めれば、載せ替えで変わったかを測れる。
  - decided_by: Claude（[foundation A7](./2026-10-12-foundation.md#A7) と [段 4 E27](./2026-10-12-foundation-4.md#E27) から導いた。自走の指示により決めた）
- F7 段 5 で残す境界と読んでよいものを決める。残して読んでよいのは、`remote_merge::cli::*` の `execute_*` の形、`RuntimeTargets`、出力の型（engine の service/types.rs）と、出力の型を文字列にするだけの書き出し（service/output.rs）、行の差分と行の競合を求める関数（core の diff/engine.rs・diff/conflict.rs。入力は二つか三つの中身、出力は hunk と競合の範囲）、バックアップの記録と集約先（backup_store）、コマンドの入口のパスの検査（path_resolver の引数を `RelPath` にする関数）、cli のうち判定をしない部分（引数の扱い・書き込む前の確認の問い・結果の書き出しの呼び出し）と、段 1〜4 で書いた部品である。書き出しや cli の中に判定（件数・状態・終了コードを比べた結果から決めるもの）があれば、それは読まずに新しい扱いの層に置き直す。読まない・写さないのは、段 4 の計画が挙げた古い比べる実装（[段 5 の計画](../../plans/foundation-5.md) の Scope of change に引き継いだ）（engine と製品の service の diff・status・merge・sync・rollback の比べる部分、three_way、古い木 core の tree.rs）と、runtime の古い読み込み（side_io の Side 単位の文字列のパスの API・side_reads・side_hashes）である。
  - why: [foundation A1](./2026-10-12-foundation.md#A1) は差し替える境界と合格の基準のテストだけを読ませる。出力の型と書き出しは、[foundation A6](./2026-10-12-foundation.md#A6) のとおり作り直しの間は変えない物差しである。行の差分の関数は判定ではなく行の数え方で、[段 4 E27](./2026-10-12-foundation-4.md#E27) が今の示し方を要件にしたので残す。
  - decided_by: Claude（[foundation A1](./2026-10-12-foundation.md#A1)・[foundation A6](./2026-10-12-foundation.md#A6) から導いた。自走の指示により決めた）
- F8 出力の型は、段 4 で決めた出力の変更（[段 4 E24](./2026-10-12-foundation-4.md#E24)〜[E28](./2026-10-12-foundation-4.md#E28)）に要る分だけ足す。足すのは、symlink のリンク文字列の参照先の値（"link_targets" の "ref"）、バイナリの参照先のハッシュ（"ref_hash"）、ファイル全体の競合の範囲（"ref_range" の {"whole_file": true}）である。今ある欄は消さず、名前と意味を変えない。
  - why: [foundation A6](./2026-10-12-foundation.md#A6) が出力の型を変えないとしたのは、今のテストを物差しに使うためである。欄を足すだけなら今のテストはそのまま物差しになり、段 4 で決めた出力の変更を入れられる。消すか名前を変えると物差しが壊れる。
  - decided_by: Claude（foundation A6 の why と [段 4 E28](./2026-10-12-foundation-4.md#E28) から導いた。A6 の読み方を Claude が確かめて決めた）
- F9 段 4 で段 5 に送ったものをすべて段 5 の計画に引き継ぐ。[REQ-compare-005](../../ir/compare/flow.md#REQ-compare-005)・[REQ-compare-013](../../ir/compare/reading.md#REQ-compare-013)・[TBL-compare-005](../../ir/compare/reading.md#TBL-compare-005) と EX-compare-005・008・009・010・011・015、変更の直前の判定（[REQ-compare-015](../../ir/compare/reading.md#REQ-compare-015)）の merge・sync の結果としての確かめ（EX-compare-017・018）、コマンドの要件の書き直し（[TBL-cli-001](../../ir/cli/status.md#TBL-cli-001)・[REQ-cli-080](../../ir/cli/diff-strategy.md#REQ-cli-080)・[REQ-cli-088](../../ir/cli/unreadable.md#REQ-cli-088)〜[REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090)・[REQ-merge-017](../../ir/merge/read-failure.md#REQ-merge-017)・[REQ-merge-019](../../ir/merge/read-failure.md#REQ-merge-019)・[REQ-merge-020](../../ir/merge/comparison.md#REQ-merge-020)・[REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033)・[REQ-merge-034](../../ir/merge/concurrency.md#REQ-merge-034)）の CLI の出力での確かめ、[TBL-cli-005](../../ir/cli/status-targets.md#TBL-cli-005)・[REQ-cli-035](../../ir/cli/status-targets.md#REQ-cli-035)（EX-cli-099・100）、[REQ-cli-047](../../ir/cli/merge.md#REQ-cli-047)・[TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007)（EX-cli-101・102・103）、[REQ-cli-097](../../ir/cli/conflicts.md#REQ-cli-097)〜[REQ-cli-099](../../ir/cli/conflicts.md#REQ-cli-099)（EX-cli-104〜108）、[REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063)・[REQ-cli-100](../../ir/cli/reference.md#REQ-cli-100)・[REQ-cli-101](../../ir/cli/reference.md#REQ-cli-101)（EX-cli-109〜111）、その他の種類の理由の文の形（F5）とその指定の仕方とコマンドごとのテスト、新しいディレクトリの権限の要件の経路の名前（F1 で済ませた）である。段 4 の記録と計画の外にある引き継ぎ、[段 1 B6](./2026-10-12-foundation-1.md#B6) の Side 単位の文字列のパスの API を消すことと、PROJECT.md の構成の説明の「段 5 まで」の二か所（その他の種類の暫定の扱いと、比べる流れを古いコマンドが使わないこと）を直すことも引き継ぐ。段 4 の計画は、段 5 の計画に引き継いだ後に消す。
  - why: [段 4 の記録](./2026-10-12-foundation-4.md) の Position は段 5 に送ったものを段 4 の計画に置いており、計画を消すと引き継ぎが記録から辿れなくなる。
  - decided_by: Claude（[foundation A7](./2026-10-12-foundation.md#A7) の「各段は決定記録 1 本と計画 1 つ」から導いた。自走の指示により決めた）
- F10 段 5 で載せ替えると利用者に見えて変わる振る舞いは、段 4 で決めた次のものと、F12〜F15・F20 である。diff がディレクトリの展開と root_dir 全体でも通常ファイルを中身で比べる（[段 4 E4](./2026-10-12-foundation-4.md#E4)）。上限を超える同じ中身のファイルを読めないファイルにしない（[E5](./2026-10-12-foundation-4.md#E5)）。その他の種類の理由が F5 の形になる（[E8](./2026-10-12-foundation-4.md#E8)・[E29](./2026-10-12-foundation-4.md#E29)。判定できないことと終了コード 2 は今のまま）。merge・sync が、書き込み・symlink の作成と置き換え・削除・--hunks のどれでも、比べた後に中身が変わった書き込み先を変えずに failed に出す（[E9](./2026-10-12-foundation-4.md#E9)）。diff の子の一覧が件数の上限を超えたら errors に出して終了コード 2 を返し、merge・sync は書き込みを始めない（[E11](./2026-10-12-foundation-4.md#E11)）。diff の種類の違いの note を両側に項目があるときだけ出す（[E13](./2026-10-12-foundation-4.md#E13)）。files_with_changes にバイナリと symlink を数える（[E14](./2026-10-12-foundation-4.md#E14)）。--ref の diff がバイナリと参照先にないファイルと消すと変えるの組をファイル全体の競合として示す（[E16](./2026-10-12-foundation-4.md#E16)・[E26](./2026-10-12-foundation-4.md#E26)）。status の ref_badge の "all_equal" と "missing_in_ref" の付け方（[E24](./2026-10-12-foundation-4.md#E24)）。種類の違いでスキップした merge の終了コード 2 と sync の "partial"（[E25](./2026-10-12-foundation-4.md#E25)）。--ref の diff の参照先との差の出し方（[E28](./2026-10-12-foundation-4.md#E28)）。比べる前に消えたファイルを片側だけにしない（[E3](./2026-10-12-foundation-4.md#E3)）。読み込みの失敗の理由が "read failed: left: …" の形になる（[E7](./2026-10-12-foundation-4.md#E7)）。diff のテキストが errors のすべてを標準エラーに出す（[E12](./2026-10-12-foundation-4.md#E12)）。参照先の root_dir の外の中身を出さない（[E15](./2026-10-12-foundation-4.md#E15)）。名前で指定した起点の途中がディレクトリでない側では、その起点をないとして続ける（[E33](./2026-10-12-foundation-4.md#E33)）。行の競合の数え方（[E27](./2026-10-12-foundation-4.md#E27)）は今のまま変えない。どれがどの段の CLI のテストで示されるかは計画の Verification map に置く。
  - why: 段 4 は流れを契約テストで確かめ、CLI の出力での確かめを段 5 に送った（[段 4 E18](./2026-10-12-foundation-4.md#E18)）。利用者に見える変化を一か所に挙げておくと、載せ替えで変わったテストが、決めた変化か誤りかを見分けられる。
  - decided_by: Claude（[段 4 E18](./2026-10-12-foundation-4.md#E18)・[段 4 E19](./2026-10-12-foundation-4.md#E19) から導いた。自走の指示により決めた）
- F11 段 5 の実装で、今のテストが IR と違う動きを固めていると分かったら、[段 4 E19](./2026-10-12-foundation-4.md#E19) と同じ扱いで IR に合わせて直し、どの要件と違っていたかをこの記録に F 番号で残す。F10 に挙げた変化を固めていたテストは、その変化を決めた E か F を書いて直す。テストの確かめる中身を変えずに、古い部品を呼んでいたテストを新しい部品を呼ぶ形に直すのは、記録に残さなくてよい。
  - why: [foundation A6](./2026-10-12-foundation.md#A6) のとおり、物差しを書き換えずに作り直しが正しいかを測る。
  - decided_by: 利用者（[foundation A6](./2026-10-12-foundation.md#A6) から導いた。自走の指示により書き下ろした）
- F12 merge と sync の --dry-run は、削除する予定のファイルを削除したファイルと同じ形で出さない。JSON では deleted の status を "would delete" にし、merge のテキストでは "Would delete: パス"、sync のテキストでは書き込む予定と同じ "plan" で始めてパスの後に " (delete)" を添える。[FLAG-cli-013](../../ir/cli/FLAGS.md#FLAG-cli-013)・[FLAG-merge-013](../../ir/merge/FLAGS.md#FLAG-merge-013) はこれで閉じる。
  - why: 予定を実行済みと同じ "ok"・"Deleted" で出すと、プレビューを見た利用者も自動の処理も、消したかどうかを見分けられない。書き込む予定は "would merge"・"Would merge"・"plan" で区別しており（[REQ-cli-045](../../ir/cli/sync.md#REQ-cli-045)・[REQ-cli-049](../../ir/cli/merge.md#REQ-cli-049)・[REQ-cli-072](../../ir/cli/sync.md#REQ-cli-072)）、削除だけが揃っていなかった。載せ替えで扱いの表を書くときに、どれかに決めないと実装者が黙って決めることになる。
  - decided_by: Claude（利用者の品質の基準『エラーの握りつぶしは必ず直す』と書き込む予定の出し方から一通りに決まるため。自走の指示により決めた）
- F13 --hunks の merge で差分のないファイルは、今の JSON（FLAG の本文によると status "skipped (no changes)"。テストで固めておらず、JSON の形は変えない）に合わせ、テキストでもほかのスキップと同じ "Skipped: パス (no changes)" の行で出し、書き込んだときの "Merged: パス" を出さない。終了コードは 0 のままとする。[FLAG-merge-019](../../ir/merge/FLAGS.md#FLAG-merge-019) はこれで閉じる。
  - why: テキストだけが書き込んだように出ていて、JSON と食い違っていた。スキップの行の形は [REQ-cli-049](../../ir/cli/merge.md#REQ-cli-049) がすでに決めている。
  - decided_by: Claude（REQ-cli-049 と JSON の status から一通りに決まるため。自走の指示により決めた）
- F14 sync の書き込み先の status（[TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007)）は、書き込めたファイルと同じく消せたファイルも数え、書けたか消せたファイルがあり失敗もあれば "partial"、どちらもなく失敗があれば "failed" にする。[FLAG-cli-011](../../ir/cli/FLAGS.md#FLAG-cli-011) はこれで閉じる。
  - why: 削除だけの書き込み先で一部の削除に失敗すると、消せたファイルがあっても "failed" になり、[REQ-cli-019](../../ir/cli/results.md#REQ-cli-019) の部分的な成功と食い違っていた。
  - decided_by: Claude（REQ-cli-019 から一通りに決まるため。自走の指示により決めた）
- F15 sync は、接続か状態の収集に失敗した書き込み先の結果も、ほかの書き込み先と同じく --right に指定した順の位置に出す（[REQ-cli-039](../../ir/cli/sync.md#REQ-cli-039)）。[FLAG-cli-008](../../ir/cli/FLAGS.md#FLAG-cli-008) はこれで閉じる。
  - why: REQ-cli-039 は指定した順に処理して報告すると決めており、失敗した書き込み先だけを末尾に回すのはその例外の継ぎ足しになる。
  - decided_by: Claude（REQ-cli-039 から一通りに決まるため。自走の指示により決めた）
- F16 [REQ-merge-041](../../ir/merge/concurrency.md#REQ-merge-041) に契約テストを置かない理由は次のとおりとし、この決定を出典にする。ないパスの削除がないの失敗になることは三つの経路について EX-paths-044 の契約テストで確かめている。比べた後に書き込み先が消えることは、変更の直前の判定（[REQ-compare-015](../../ir/compare/reading.md#REQ-compare-015)）が見つけて failed に出すので、REQ-merge-041 が扱うのはその判定の後・削除の前の短い窓で消えたときだけで、その窓は契約テストで作れない。[FLAG-merge-020](../../ir/merge/FLAGS.md#FLAG-merge-020) はこれで閉じる。
  - why: FLAG-merge-020 は、理由の出典がなく、エージェントの経路も削除を行うようになったら確かめ方と合わせて決めると残していた。段 3 で三つの経路が同じ削除の操作を持ち、段 4 で変更の直前の判定が入ったので、今決められる。
  - decided_by: Claude（[段 3 D12](./2026-10-12-foundation-3.md#D12)・[段 4 E9](./2026-10-12-foundation-4.md#E9) から導いた。自走の指示により決めた）
- F17 段 5 の最後の工程で、品質レビュー 1 回目の台帳（利用者の作業場所の `.agents/artifacts/quality/round1/ledger.json` と `ledger.md`。リポジトリの外にあり、コミットしない）のすべての項目を、消えた・security の境界の外・残るの三つに分ける。消えたものは、消した決定（B〜F の番号）と、それを示すテストか消えたコードの場所を書く。security の境界の外のものは [foundation A11](./2026-10-12-foundation.md#A11) と [段 3 D46](./2026-10-12-foundation-3.md#D46) を書く。残るもののうち critical と warn は段 5 の中で直し（決めることのないものはテストを先に書いて直し、決めることがあれば利用者に返す）、info は後の単位の項目として残す。台帳の後の単位の項目（sk-・ui-・so-・pv- で始まるもの）も同じに分ける。結果は ledger.md に「foundation の結果」の節として足し、ledger.json の status を今の形（"fixed (単位, コミット)"）で書く。分けていない項目が一つも残らないことを終わりの条件にする。
  - why: [foundation A7](./2026-10-12-foundation.md#A7) は、台帳の項目を段 5 の後に消えたかを確かめる一覧として使うと決めた。利用者の品質の基準は critical と warn を残さないことで（[foundation R3](./2026-10-12-foundation.md#R3)）、範囲を狭めない。台帳の status は単位ごとに書き換えていないものが多く（132 件のうち 109 件が "open"）、各単位の結果の節と照らさないと今の状態が分からない。
  - decided_by: 利用者（[foundation A7](./2026-10-12-foundation.md#A7)・[foundation R3](./2026-10-12-foundation.md#R3) から導いた。自走の指示により書き下ろした）
- F18 段 5 は最後の段なので、終わりの条件は `kotowari check` の error が 0 であること（先の段に送るものはない）、コンテナの e2e が通ること、F17 の台帳の確かめが終わったことの三つとする。
  - why: 段 4 までは、後の段で示す要件のテストがないことによる error を残してよかった。段 5 にはその後がない。
  - decided_by: Claude（[foundation A7](./2026-10-12-foundation.md#A7) から導いた。自走の指示により決めた）
- F19 段 5 で開いたままにする FLAG は次のとおりで、載せ替えでは今の振る舞いを変えない。[FLAG-cli-007](../../ir/cli/FLAGS.md#FLAG-cli-007)（JSON の sync の確認を断ったとき）と [FLAG-cli-015](../../ir/cli/FLAGS.md#FLAG-cli-015)（確認の行の件数 0 の部分）は、書き込む前の確認の問いで、F7 で残す cli の部分にある。[FLAG-cli-032](../../ir/cli/FLAGS.md#FLAG-cli-032)（0 始まりの行番号）と [FLAG-cli-058](../../ir/cli/FLAGS.md#FLAG-cli-058)（conflict_regions の要素の形）は、F7 で残す行の差分の関数と出力の型が決める。[FLAG-backup-005](../../ir/backup/FLAGS.md#FLAG-backup-005) はバックアップの記録の連番で、段 5 で作り直さない。FLAG-merge-021 は、SSH の経路をなくしたので F23 で閉じた。
  - why: どれも比べる流れの判定の扱いではなく、載せ替えで触れない部分が決めている。今の振る舞いは FLAG の本文に書かれているので、実装者は今のコードを読まずに保てる。
  - decided_by: Claude（F7 から導いた。自走の指示により決めた）

- F20 バックアップが有効で集約先の場所が決まらない構成でも、書き込むファイルも消すファイルも一つもない merge と sync は、集約先の誤りで止めずに結果（スキップ・failed）を出して終える（--dry-run も同じ）。書き込むか消すファイルが一つでもあれば、今までどおり書き込む前にエラーで止まる（[REQ-backup-018](../../ir/backup/failure.md#REQ-backup-018)）。[FLAG-cli-026](../../ir/cli/FLAGS.md#FLAG-cli-026) はこれで閉じる。
  - why: 書くファイルがなければバックアップは要らない。今は同じ「書くものがない」でも、すべてのファイルがスキップで外れたときだけ止まり、参照先の確認で外れたときは止まらず、扱いが二通りに割れていた。止めない側に揃えると、すべてのファイルがスキップされた merge は、今のエラーではなく結果を出して終わる。
  - decided_by: Claude（利用者の品質の基準「経路差・扱いの割れをなくす」から導いた。自走の指示により決めた）

- F23 [foundation A12](./2026-10-12-foundation.md#A12) に合わせて IR を書き直す。SSH の経路にだけ関わる要件・例・表・FLAG は消し、エージェントへの切り替えの文は「使えなければ止める」の一文に縮め、新しい要件と例は足さない。消すのは、要件 7 件（REQ-ssh-019・REQ-ssh-023・REQ-ssh-025・REQ-ssh-026・REQ-ssh-027・REQ-paths-009・REQ-scan-013）、表 1 件（TBL-ssh-002）、例 14 件（EX-ssh-008・009・030・034・035・037・041・042・057・058・059、EX-paths-020、EX-merge-046・050）、FLAG 3 件（FLAG-merge-021・FLAG-scan-015・FLAG-paths-003）である。REQ-ssh-005 は、エージェントを使えないサーバを sudo の有無によらず読み書きの前に理由を示して止める一つの要件にし、配布のある環境（Linux の x86_64 と macOS。本番の CentOS 5 のカーネル 2.6.18 を含む）と、配布のないサーバ（ARM の Linux）を止めることを一文で足す。TBL-ssh-001 は「使う」と「止める」の二行にする。REQ-ssh-020 はエージェントを置いて起動するためのコマンドの規則にする。三つの経路を並べた要件と例は、ローカルとエージェントの二つにする。docs/remote-tools.md は、エージェントの配置と起動に要る事実だけを残す。
  - why: 利用者の補足（2026-10-13）のとおり、改訂は余計なものが消える形にする。切り替えの警告（REQ-ssh-027・REQ-ssh-019）、道具の表とその有無の扱い（TBL-ssh-002・REQ-ssh-025・026）、readlink がないとき（REQ-paths-009）、SSH の走査の時間切れ（REQ-scan-013）、読み取りのやり直しで別の経路に切り替えないこと（REQ-ssh-023。切り替える先がなくなり REQ-ssh-003 に含まれる）は、SSH の経路がなければ起きない。消した事実は git の履歴にあるので、文書の中に残さない。
  - decided_by: 利用者（[foundation A12](./2026-10-12-foundation.md#A12) と 2026-10-13 の補足「余計なものが消える形にする」）。消す項目の選び方は Claude
- F24 操作の失敗の種類から「道具がない」をなくし、七つ（ない・ディレクトリでない・権限がない・上限を超える・I/O・接続断・root_dir がない）にする（[REQ-paths-020](../../ir/paths/operations.md#REQ-paths-020)・[TBL-compare-004](../../ir/compare/reading.md#TBL-compare-004)）。エージェントの配置に要る道具がないことは、操作の失敗ではなく REQ-ssh-005 の止める理由になる。
  - why: 道具がないは、SSH の経路がリモートの道具を使う操作のための種類だった（[段 3 D1](./2026-10-12-foundation-3.md#D1)）。ローカルとエージェントは道具を使わないので、残すと使われない種類が扱いの表に残り、すべての値を挙げる表（F6）に意味のない行ができる。
  - decided_by: Claude（A12 から導いた。自走の指示により決めた）
- F25 status -v の Agent の接続状態（[REQ-cli-070](../../ir/cli/status-output.md#REQ-cli-070)）は "connected" だけにし、"fallback" と "Agent: fallback (SSH exec)" をなくす。JSON の "agent" の欄は残す（F8）。
  - why: エージェントを使えないサーバは比べる前に止まるので、"fallback" を出す場面がない。欄を消すと出力の型の今ある欄を消すことになる。
  - decided_by: Claude（A12・F8 から導いた。自走の指示により決めた）
- F26 段 5 の計画では、S0 の直後の S1 で SSH の経路を消す。消すのは、`remote-merge-ssh` のうち走査・パスの確認・中身・変更のスクリプトとその出力の読み方（scan_*・path_facts*・contents*・change_*・entry_links・command_limit とそのテスト）、製品の runtime の SSH の経路（ssh_inspect・ssh_contents・ssh_scan・ssh_changes・ssh_entry_links と SSH の経路の比較先）、経路の選び方のうち SSH の経路を選ぶ枝、RootWatch の SSH 向けの分け、偽の SSH サーバのうち SSH の経路の答えを作る部分、コンテナで SSH のスクリプトを試すテスト（REMOTE_MERGE_TEST_SCAN_IMAGE・REMOTE_MERGE_TEST_REQUIRE_SCAN_IMAGE・REMOTE_MERGE_TEST_REQUIRE_FISH で選ぶもの）、操作の失敗の種類の「道具がない」（F24）である。残すのは、SSH の接続・ホスト鍵・エージェントの配置と起動・エージェントとの通信、container e2e（エージェントの sudo を含む）である。契約テストの「三つの経路」は「ローカルとエージェントの二つ」にする。
  - why: 後の載せ替えで合わせる先が二つになり、作業が減る（利用者の指示）。載せ替えの前に消せば、載せ替えた扱いの層が SSH の経路を考えずに済む。
  - decided_by: 利用者（2026-10-13、計画の指示）
- F27 段 5 の確かめの一つに、エージェントを CentOS 5 のカーネル（2.6.18）で配置・起動・走査できることを、qemu で 2.6.18 を起動して確かめる検証を入れる。手順を PROJECT.md の試験の説明に置くかは、検証の結果が出てから決める。
  - why: [foundation A12](./2026-10-12-foundation.md#A12) は、今の版のエージェントが 2.6.18 で動くことを別に確かめている最中としている。SSH の経路をなくすと、そのサーバはエージェントで動かなければ扱えない。
  - decided_by: 利用者（2026-10-13、計画の指示）
- F28 台帳の確かめ（F17）では、SSH の経路にだけ関わる項目を「経路をなくしたので消えた」に分け、根拠に A12 と S1 を書く。
  - why: 利用者の指示による。SSH の経路のコードがなくなれば、その項目は直すものではなく、なくなったものである。
  - decided_by: 利用者（2026-10-13、計画の指示）
- F29 F23 で消した要件と例の印を持つ今のテスト（32 か所）は、S1 で SSH の経路と一緒に消すか、ほかの要件の印に付け替える。エージェントの経路を確かめていて REQ-ssh-023 の印を持つテストは、REQ-ssh-003 の印に付け替える。S1 が終わるまで、`kotowari check` はそれらを試験の側の unresolved_reference として出す。
  - why: 仕様の改訂でテストのコードは変えない。印を外す・付け替えるのは、テストを消すか直す実装の段の仕事である。
  - decided_by: Claude（F26 から導いた。自走の指示により決めた）

- F30 SSH の経路に切り替えて続けることを固めていた今のテストは、[REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005)（[foundation A12](./2026-10-12-foundation.md#A12)）に合わせ、読み書きの前に理由を示す接続のエラーで止まり終了コード 2 を返すことを確かめる形に直した。直したのは、sudo のない [agent] enabled が false・起動できない・通信の版が違うサーバ（ssh_route_choice、ssh_agent_version）、安全でない配置先（ssh_agent_deploy の EX-ssh-023・027・032 と、警告して SSH で続けていた EX-ssh-030 のテストは配置先のパスを示して止まる REQ-ssh-005 のテストに）、status -v の "fallback"（status_agent_cli は "connected" を確かめ、エージェントを使えないサーバの status -v は止まることを確かめる。[REQ-cli-070](../../ir/cli/status-output.md#REQ-cli-070)・F25）、sudo=true のときだけの文を固めていた単体テストである。止める理由の文は sudo の有無によらず "remote files are handled only through the agent, and the agent cannot be used: 理由" の一つの形にした。契約テストの共通の試験環境（CliEnv・TestDirs）はエージェントを置いた偽サーバに替え、エージェントを無効にしていた設定はエージェントを置いて有効にした。
  - why: 今のテストは A12 の前の TBL-ssh-001（sudo のないサーバは SSH で続ける）を固めていた。
  - decided_by: Claude（[F11](#F11) と A12 から導いた。自走の指示により決めた）
- F31 SSH の経路の答え（シェルのスクリプト・道具の有無・答えの目印・走査の時間切れ）だけを確かめていたテストは消し（F26）、同じ要件と例をエージェントの経路で示すテストがないものは、エージェントの経路のテストを書くか、今のエージェントのテストに印を付け替えた。EX-ssh-005・006・007・019 と REQ-ssh-003 は、エージェントの要求を切る偽サーバのテスト（ssh_connection_loss）へ移し、EX-ssh-048 は読み込みのまとめに入るファイルと入らないファイルの merge で示す。EX-merge-044 は、偽サーバが権限の変更の要求をエージェントに渡さずに権限がないで答える仕掛けで示す。REQ-ssh-023 の印は REQ-ssh-003 に付け替えた（F29）。ローカル・SSH・エージェントの三つの経路を並べた契約テストは、ローカルとエージェントの二つにした。
  - why: 消したテストの要件と例が試験のないものにならないようにする（[F18](#F18)）。
  - decided_by: Claude（F26・F29 から導いた。自走の指示により決めた）
- F32 エージェントの実行ファイルを置くときの書き込みは、sudo の有無によらず、標準入力の base64 を `openssl base64 -d -A -out` で復号して置き場所の一時ファイルに書く一つのコマンドにする（今までは sudo のないサーバだけ SSH の経路の書き込みのスクリプトを使っていた）。この事実を docs/remote-tools.md に足す。
  - why: SSH の経路の書き込みのスクリプトを消す（F26）と、配置の書き込みが使えなくなる。sudo のサーバの配置はすでにこのコマンドで動いており、新しい道具は要らない（[段 5 の計画](../../plans/foundation-5.md) S1 の止まる条件に当たらない）。置き場所の確かめ（symlink でないこと）は書き込みの前の確かめのコマンドが行う。
  - decided_by: Claude（F26 と S1 の止まる条件から導いた。自走の指示により決めた）

- F33 このマシンの走査（ローカルとエージェント）は、ナノ秒の部分が 0 の更新時刻を秒の精度（読めた桁 0）として載せ、それ以外をナノ秒の精度として載せる。同じ大きさで同じ秒を指す組は、[TBL-compare-001](../../ir/compare/judgement.md#TBL-compare-001) の「片側でも更新時刻が秒の精度しかない」に当たり、中身で決める。精度を固定の 9 桁で載せていたため、秒の精度しかないファイルシステム（CentOS 5 の既定の 128 バイトの inode の ext3。どの時刻もナノ秒の部分が 0 になる）どうしでは、大きさと秒が同じで中身の違うファイルをメタデータで同じとしていた。テストの日付を秒ちょうどにしていた status の判定の契約テスト（status_support の write_at）は、秒より細かい時刻を持つよう半秒を足し、SSH の find の書式を替えて秒の精度を作っていたテスト（EX-compare-006・EX-cli-089・EX-merge-041）は、秒ちょうどの時刻で作る形に直した。
  - why: IR は精度を「読んだ時刻が秒より細かい精度を持つか」で決めるが、その見分け方を決めていない。ナノ秒の部分が 0 の時刻は秒より細かい精度を持つことを示さないので、秒の精度とみなすのが IR の側に倒れる（誤って同じとせず、読む数が増えるだけ）。qemu で起動した 2.6.18 の ext3 で、すべての mtime_nanos が 0 になることを確かめた（F27 の検証）。
  - decided_by: Claude（TBL-compare-001 と F27 の検証の結果から導いた。自走の指示により決めた）

- F34 status を比べる流れに載せ替えたので、--ref の参照先との組も比べ方（[TBL-compare-003](../../ir/compare/judgement.md#TBL-compare-003) の status のメタデータ）で判定し、大きさか更新時刻で決まる組は参照先の中身を読まない。参照先を読めないことを確かめていた status の契約テスト（status_unreadable の二つ）は、参照先を読まなくても大きさで「違う」と決まる組を使っていたので、左と参照先を同じ大きさで更新時刻の違う組にして、参照先を読みに行く場合に直した。参照先の印の集計 "ref_only" は、左右になく参照先にだけあるディレクトリでない項目の数とする（IR は数え方を決めておらず、テストは数であることだけを確かめる）。
  - why: 今のテストは、印のために参照先の中身をいつも読む古い status の動きを固めていた。[REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011) は判定のために中身を読まないとし、[TBL-cli-005](../../ir/cli/status-targets.md#TBL-cli-005) の「三つのどれかが違う」は大きさの違いでも当たる。
  - decided_by: Claude（[F11](#F11) と REQ-compare-011 から導いた。自走の指示により決めた）

- F35 diff を比べる流れに載せ替え、判定を項目・参照先との差・競合・errors・summary に訳す。名前で指定したパスはどれも一つの比べる流れの走査（指定したディレクトリの下と、起点そのもの）で集め、起点そのものだけを集める起点には exclude を当てない（[REQ-cli-081](../../ir/cli/diff-strategy.md#REQ-cli-081)）。root_dir 全体と一緒に名前で指定したパスのうち全体の走査に載らなかったもの（exclude に当たるもの）は、名前で比べ足す。名前の下の一覧を取れなかった（読めないディレクトリ・辿れない symlink・途中で変わった項目など、止めた項目のパスを持つ走査の失敗）ときは、その名前を errors に出して除き、残りの名前で比べ直す（[REQ-cli-096](../../ir/cli/directory-paths.md#REQ-cli-096)）。件数の上限の超過は errors のパス "." に、循環はそのパスに載せる。参照先がない symlink は、項目を残して "the link target is unreadable: it does not exist" を errors に出し（[REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022)）、root_dir の外を通って状態を集めなかったパスは、名前で指定したか他方の側に比べるファイルがあるときに、比べなかった理由を errors に出す（[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026)）。参照先の種類が左右の通常ファイルと違う（ディレクトリ・symlink など）か、読めないときは "ref" だけを出す。種類の違いの note は "type mismatch: 左の種類 vs 右の種類" とし、種類は判定に使った状態（file・directory・symlink）で示す（[段 4 E13](./2026-10-12-foundation-4.md#E13)）。
  - why: [段 4 E4・E11・E12・E13・E14・E16・E26・E28](./2026-10-12-foundation-4.md#E4) と、計画 S4 の Verification map の行。IR は errors の件数の上限の超過のパス、note の文言、参照先のない symlink の理由の文言を決めておらず、テストは理由があることと "unreadable" を含むこと、"not compared" を含むことだけを確かめる。
  - decided_by: Claude（計画 S4 と上の IR から導いた。自走の指示により決めた）

- F36 diff の載せ替えで、IR と違う動きか古い実装の都合を固めていた今のテストを、次のとおり直した。名前で指定した "dir/a.txt" の途中の "dir" が右で通常ファイルのとき "cannot resolve path: not a directory" で diff 全体を止めることを確かめていた diff_file_kinds のテストは、右にないとして左だけの差分を出す形に直し、印を [REQ-compare-003](../../ir/compare/flow.md#REQ-compare-003) に付け替えた（[段 4 E33](./2026-10-12-foundation-4.md#E33)。計画の Verification map は E33 を S4 の diff のテストで示すとしている）。三つの名前を指定した diff の二回目の ReadFiles の前に root_dir を消していた source_root_gone のテストは、中身を側ごとに一度の要求でまとめて読む（[REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011)）ので、一回目の前に消す形に直した。走査が二度切れた diff の誤りの文に指定したファイルの名前があることを確かめていた ssh_connection_loss のテストは、名前で指定したパスも走査で集めるので、誤りの JSON が返ることを確かめる形に直した。テキストの出力の標準出力に「比べなかった」理由があることを確かめていた cli_diff のテストは、標準エラーで確かめる形に直した（[段 4 E12](./2026-10-12-foundation-4.md#E12)）。
  - why: [F11](#F11)。[REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) は今も diff を「祖先がディレクトリでないパスをエラーとして扱う」ものに挙げており、REQ-compare-003（E33）と食い違う。計画に従って E33 を当て、IR の食い違いは段 5 の報告で利用者に返す。
  - decided_by: Claude（F11・E12・E33 と計画 S4 から導いた。自走の指示により決めた）

- F37 計画 S4 が消すとした engine の `fast_path.rs`・`content_reads.rs` と cli の `content_reads.rs` は、sync と merge がまだ使うので、使い手を載せ替える S5・S6 で消す（`ref_guard.rs` も merge が使うので S5 で消す）。S4 で消したのは、engine の `service/diff.rs`・`service/max_files.rs`、製品の同じ名前の再公開、cli の `diff_scans.rs`・`diff_children.rs`・`diff_kinds.rs`・`diff_listing.rs`・`diff_expansion.rs`（とそのテスト）、runtime の `scan_entry.rs` の `resolved_path_outside_root`。
  - why: 計画の止まる条件の「古いモジュールを消せない」は、使い手が残る理由を説明できないときを指す。使い手は後の段で載せ替える merge と sync で、[F6](#F6) の「古い実装は、それを使う最後のコマンドを載せ替えた段で消す」に当てると、消す段は S5・S6 になる。
  - decided_by: Claude（F6 と計画 S4 から導いた。自走の指示により決めた）

- F38 F27 の検証（2.6.18・musl・root と一般の利用者）で落ちたテストを、確かめることを変えずに環境によらない形に直した。core のテストは、更新時刻を utimensat のないカーネルでも使える utimes で決め、OS の誤りの文言を C ライブラリの strerror と比べ（musl と glibc で文言が違う）、proptest の乱数の種を時計から取り（既定の種は 2.6.18 で /dev/random を待ち続ける）、root では読む権限のないディレクトリの下も一覧に載ることを許す。エージェントの配置先の検査のテストは、TMPDIR の祖先のモード（775 のとき）と root で動かすかによらず、/tmp の下に専用のディレクトリを作って確かめる。
  - why: 製品の振る舞いは変えず、テストが環境の都合で落ちていた。2.6.18 に utimensat がないことは docs/remote-tools.md に書いた（エージェントは更新時刻を変えないので、製品は影響を受けない）。
  - decided_by: Claude（F27 の検証の結果から導いた。自走の指示により決めた）

- F39 F27 の手順は、リポジトリの `scripts/kernel-2.6.18/`（reproduce.sh と、qemu の起動・initramfs・VM の中の手順・答えの比べ方・要求を作る道具 probe）に置き、PROJECT.md の Commands の表に一行足す。比べるのは、エージェントの答えを probe で読み、走査の一覧を一項目ずつにし、環境で変わる値（更新時刻・ディレクトリの大きさ・更新時刻の桁・作業ディレクトリの実パス）を伏せたものと、単体テストの終了の値である。カーネルの rpm と静的な BusyBox はリポジトリに入れず、作業ディレクトリに置く前提にする（rpm の sha256 を手順で確かめる）。
  - why: A12 でエージェントだけでリモートを扱うので、本番の CentOS 5 のカーネルで動くことは、エージェントの通信か使うシステムコールを変えるたびに確かめ直す必要がある。検証のときの手順は消える作業場所にしかなかった。probe は製品のワークスペースに入れない独立の小さな道具にし、製品のテストと検査に混ぜない（製品ではないので IR に要件を置かない）。
  - decided_by: Claude（F27 と A12 から導いた。自走の指示により決めた）

- F40 merge と sync を比べる流れに載せ替え、扱いを書き込み・スキップ・failed・status に訳す。走査の範囲は [TBL-scan-001](../../ir/scan/limits.md#TBL-scan-001) のとおりに選び、名前で指定したパスとその下だけを扱い、名前で指定したファイルは比べ方によらず中身で比べる（比べる流れの求めに、中身で比べるパスを渡す。[TBL-compare-003](../../ir/compare/judgement.md#TBL-compare-003)）。比べる流れが読むのは、書く読み込み元の通常ファイルの中身（--dry-run では読まない）と、書き換えるか消す書き込み先の通常ファイルのハッシュ（変更の直前の判定に使う）と、参照先の確認をする三つの通常ファイルのハッシュだけにする。参照先の確認（[REQ-cli-051](../../ir/cli/merge.md#REQ-cli-051)）は、三つとも通常ファイルなら比べ方によらずハッシュで、三つとも symlink ならリンク文字列で決め、参照先との組がその他の種類で判定できないときはその理由（"not a regular file: ref"）を failed に出す。変更の手順は、書き込み先のパスと祖先をまとめて確かめ、バックアップする書き込み先の中身をまとめて読み、パスごとに変更の直前の判定・バックアップ・変更・権限の順に行う。バックアップのための読み込みの失敗は "backup failed: " の形にし（[REQ-backup-017](../../ir/backup/failure.md#REQ-backup-017)）、まとめた読み込みが `接続断` で止まったら、まとめに入っていたファイルをすべて "backup failed: read failed: " で failed に出す（[REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003)）。書き換えるか消す書き込み先のハッシュを比べるときに読めなければ、読み込みの失敗（"read failed: right: …"）として書かず消さない。書き込み先の symlink のリンク先が UTF-8 でないか置換文字を含むなら、変わっていないことを確かめられないとして書かない（[REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033)）。新しいファイルと、書き込みが作ったディレクトリは、書き込み先に設定された権限にする（[REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012)）。変更の最中の `接続断` で書き込み先を止めたら、済んだファイルがあっても終了コード 2 にし、sync ではその書き込み先を失敗に数える（[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022)）。読み込み元の中身は比べる流れが側ごとにまとめて一度に読むので、読み込み元の root_dir がなくなったと分かるのはまとめた読み込みのときで、merge と sync はまだ何も書かずに root_dir の分かる誤りで止まる（[REQ-merge-044](../../ir/merge/concurrency.md#REQ-merge-044)）。sync は、読み込み元の一覧を打ち切ったか読み込み元を止めたら全体を止め、書き込み先の側だけの失敗はその書き込み先の失敗にする。種類の違いのスキップの理由は今までの "source and destination have different file types"、書き込み先だけの symlink のスキップは "destination is a symlink; deletion skipped" のままにする（[2026-10-04 の記録の A2・A3](./2026-10-04-merge-backup-flag-resolution.md)）。
  - why: 計画 S5・S6 と上の IR。読み込み元の root_dir が途中で消える場合の結果の出し方は、まとめて読む（[REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011)）ことで、どのファイルも書く前に分かる形に変わった。そのときに何も書かずに止めるのは、REQ-merge-044 の「その側への処理を止め、終了コード 2」と、読み込み元の状態の収集の失敗で書き込みを始めない [REQ-cli-071](../../ir/cli/sync.md#REQ-cli-071) に合う。
  - decided_by: Claude（計画 S5・S6 と上の IR から導いた。自走の指示により決めた）

- F41 merge と sync の載せ替えで、IR と違う動きか古い実装の都合を固めていた今のテストを、次のとおり直した。--dry-run で TBL-cli-009 の読み込みの失敗の行だけを当てていた merge_ref_kinds のテストは、--dry-run も参照先の確認を当てる形に直した（[段 4 E10](./2026-10-12-foundation-4.md#E10)・[REQ-compare-005](../../ir/compare/flow.md#REQ-compare-005)）。種類の違いのスキップのある書き込み先を "success" としていた sync_text のテストは "partial" に直した（[段 4 E25](./2026-10-12-foundation-4.md#E25)）。途中がディレクトリでない起点の merge を走査の誤りにしていた scan_partial_starts のテストは、どちらの側にもないパスとして止める形に直した（[段 4 E33](./2026-10-12-foundation-4.md#E33)）。--force の --dry-run で参照先を読めないファイルに印を出さないことを、大きさで違うと決まる組で確かめていた merge_results のテストは、三つとも同じ大きさにして中身を読ませる組に直した（大きさで決まる組は中身を読まずに印を決める。[F34](#F34) と同じ）。読めない書き込み先でバックアップの失敗を確かめていた backup_failure と merge_paths のテストは、比べるときのハッシュが読み込みの失敗になるので、上限を超える書き込み先（ハッシュには上限を当てず、バックアップのための読み込みに当てる）でバックアップの失敗を起こす形に直した。古い一件ずつの書き込みの関数を呼んで比べた後に書き込み先を変えていた merge_paths・merge_symlink_changed_routes・backup_storage のテストは、merge の比べる段と変える段（`plan_merge`・`apply_merge`）の間で書き込み先を変える形に直した。書き込み直前の読み直しを、比べた後に書き換えた中身が書き込まれバックアップされることで確かめていた backup_storage の二つのテストは、比べた後の書き換えを変更の直前の判定が止めるので（[段 4 E9](./2026-10-12-foundation-4.md#E9)）、書き込みの直前の内容がバックアップされて rollback で戻ることを確かめる形に直した。リストの後に symlink に替わった削除の対象をスキップとしていた merge_paths のテストは、変わったものとして failed に出す形に直した（E9）。エージェントの要求の何回目で切るかを古い一件ずつの読み込みの数で決めていた ssh_interrupted_changes と source_root_gone のテストは、比べるときのまとめたハッシュ・変更の直前の判定のハッシュ・まとめたバックアップの読み込みの数で決め直した。古い status の判定の関数を呼んでいた status_judgement のテストは、比べる流れの判定の関数で確かめる形に直した。
  - why: [F11](#F11)。
  - decided_by: Claude（F11 と上の決定から導いた。自走の指示により決めた）

- F42 計画 S6 が消すとしたもののうち、rollback が使う `service/source_pair.rs` と、runtime の `side_io.rs` が使う engine と製品の `service/content_reads.rs`・cli の `content_reads.rs` は、使い手を載せ替える S7・S8 で消す。S6 では、engine の `service/` の `merge.rs`・`merge_flow.rs`・`merge_reference.rs`・`merge_reference_reads.rs`・`kind_skip.rs`・`sync.rs`・`preview_paths.rs`・`fast_path.rs`・`status.rs`・`status_ref_badge.rs`、engine の `three_way.rs`、製品の同じ名前の再公開と `merge_flow.rs`・`preview_paths.rs`・`permission_copy.rs`、cli の `ref_guard.rs` を消した。
  - why: [F6](#F6) の「古い実装は、それを使う最後のコマンドを載せ替えた段で消す」。status の古い判定（`status.rs`・`status_ref_badge.rs`）は merge と sync も使っていたので、S6 で消した。
  - decided_by: Claude（F6 と計画 S6 から導いた。自走の指示により決めた）

- F43 rollback を比べる流れの集め方に載せ替える。計画 S2 は rollback の扱いの表（[TBL-compare-005](../../ir/compare/reading.md#TBL-compare-005) の rollback の行）を置かなかったので、engine の treat に、書き戻し・スキップ・failed を決める純粋な関数（`rollback`）と、セッションの選び方と終了コードの純粋な関数（`rollback_session`）を足す。戻す先の事実は、セッションのパスとその親の確認を経路に一度にまとめて尋ね、確かめられたパスだけを名前で指定したパスの走査（role は target）で集める（辿れない symlink の起点は走査を止める [REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015) ので、確かめられないパスは走査に入れない）。走査の一覧に載らなかったパスは戻す先にない（[REQ-compare-003](../../ir/compare/flow.md#REQ-compare-003)）。戻す先がその他の種類かディレクトリなら、理由 "not a regular file: target" で failed に出す。一件でも確かめられないパスがあれば、セッションのすべてをそのパスの原因で "cannot resolve path: " の failed にし、一件でも記録と違う場所に解決されるか第三者が変えた symlink があれば、書き戻す予定のパスをすべてその理由でスキップにする（それぞれのスキップと failed の理由はそのまま。[REQ-backup-003](../../ir/backup/rollback-path.md#REQ-backup-003)・[REQ-backup-010](../../ir/backup/rollback-path.md#REQ-backup-010)）。記録が通常ファイルで今は symlink のパスは記録と違う場所とし、記録が symlink 同士の更新で、今が merge の書いたリンク文字列の symlink でなければ "symlink changed after merge" とする。--session がないときは --force の有無によらず期限切れでない最新のセッションを選び、選べないときと指定したセッションがないときの誤りは "No backup sessions found" で始める。書き戻す前の退避は、書き戻す先にあるものがあるときだけ新しいセッションを作り、通常ファイルの中身をまとめて読み、まとめた読み込みそのものの失敗は "backup failed: read failed: " の形、ファイルごとの読み込みの失敗は "backup failed: " に続く原因にする（[REQ-compare-014](../../ir/compare/reading.md#REQ-compare-014)）。バックアップが無効なときにまとめて取るハッシュ（[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022)）がつなぎ直しても取れなければ、そのまとめに入った通常ファイルを "read failed: target: " の形の失敗にする。事実を集める問いがつなぎ直しても `接続断` なら、どのパスも "read failed: target: " の形の失敗にし、終了コード 2 にする。symlink の作り直しは merge と同じ手順の関数を使う。書き戻しの最中の `接続断` の確かめ（`change_check`・`restore_before`・`restore_saved`）の形は変えない。親がないとしてパスを飛ばす前に root_dir を確かめる経路の関数は、root_dir の状態を走査の初めの問いで決める（[段 4 E38](./2026-10-12-foundation-4.md#E38)）ので使い手がなくなり、消した。
  - why: 計画 S7 と上の IR。部品は、走査（`collect_side`・`state_table`）、パスの確認、まとめた読み込み、書き戻す前のハッシュ、変更の確かめ（`settle_change`）、記録と集約先（`backup_store`）、期限の境目（`is_session_expired`）を今のものから使い、扱いの表とセッションの選び方は、古い実装を読まない（[F7](#F7)）ので IR から書いた。
  - decided_by: Claude（計画 S7 と上の IR から導いた。自走の指示により決めた）

- F44 rollback の載せ替えで、古い実装の都合を固めていた今のテストを、次のとおり直した。古い関数 `restore_backup` を呼んでいた rollback_saved_record のテストは、同じ事実を `restore_session` の結果で確かめる形にした。パスを一件ずつ確かめる要求（`InspectPath`）の何回目かで切っていた ssh_interrupted_rollback の二つのテストは、パスの確認をまとめて尋ねる（`InspectPaths`）ので、最初の書き戻しの前で切るテストは書き戻す前にまとめて取るハッシュ（`HashFiles`）の 1 回目の答えの直後で切り、--dry-run のパスの確認を毎回切るテストは `InspectPaths` を切る形にした。
  - why: [F11](#F11)。確かめることは変えていない。
  - decided_by: Claude（F11 と F43 から導いた。自走の指示により決めた）

- F45 rollback だけが使っていた古い実装として、engine の `service/rollback.rs`・`service/rollback_skip.rs`、製品の `service/rollback.rs`（再公開）・`service/source_pair.rs`、runtime の `side_io.rs` の書き戻しの関数（`restore_backup` とその下の関数）と、`backup_store.rs` の古い記録の型への変換を消した。cli の `rollback.rs` の終了コードの単体テストは、engine の treat の `rollback_session` の単体テストに IR の行で書き直した。
  - why: [F6](#F6)・[F42](#F42)。
  - decided_by: Claude（F6 から導いた。自走の指示により決めた）

- F46 runtime の Side 単位の文字列のパスの API（`side_io.rs` の CoreRuntime の操作、`side_reads.rs`・`side_hashes.rs`）と、それだけが使っていた中身の読み込み（engine と製品の `service/content_reads.rs`、cli の `content_reads.rs`）、その他の種類と読めない名前の暫定の扱い（`service/other_kinds.rs`・`service/unreadable_names.rs`）、core の `diff/symlink.rs` を消し、それで使い手がなくなった経路の操作（比較先の `write_file`・`fetch_tree`・`scan_tree`・`fetch_children`・`list_children`・`entry_link_target`・`connect`・`disconnect`・`is_available`・`hash_entries` と、その下の経路ごとの関数）も消した。製品で残っていた使い手は、比較先の操作に直接載せ替えた。include の値の確認は、root_dir と include の値の確認をまとめて一度に尋ねる（[REQ-scan-020](../../ir/scan/listing.md#REQ-scan-020) の「側ごとに一度」は変わらない）。セッションの一覧は `core_backup` に置き、テストがセッションを作る `save_backup` は比較先の操作で作り直して test-utils の機能だけに置いた。リモートの root_dir の見せ方と、root_dir がないときの誤りは `remote_root.rs` に移した。
  - why: 計画 S8 と [F6](#F6)・[F7](#F7)。使い手のないことは、消した後の組み立て（未使用の警告を含む）で確かめた。
  - decided_by: Claude（計画 S8 から導いた。自走の指示により決めた）

- F47 F46 で消した API を使っていたテストを、次のとおり直した。確かめることを変えたテストはない。Side 単位の API を設定（書き込み・接続）にだけ使っていた rollback_paths・backup_cleanup・merge_symlink_changed_routes・rollback_saved_record のテストは、書き込み先のディレクトリへの `std::fs` の書き込みと `connect_side`・`save_backup` に替えた。文字列で渡した root_dir 全体を各操作が拒むことを確かめていた entry_path_routes のテスト（[EX-paths-040](../../ir/paths/operations.md#EX-paths-040)）は、どの経路の項目の操作も root_dir の下の項目の型だけを受け取るので、文字列からその型にするところで "root_dir itself is not an entry: パス" の理由で止まることを確かめる形にした。runtime の子の一覧に設定の件数の上限を当てることを確かめていた listing_limit のテストは、子の一覧を使うコマンドがなくなった（diff は比べる流れの走査で比べ、件数の上限は走査に当てる。[REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) は cli_diff のテスト、子の一覧の上限の扱いは engine の child_listing の単体テストが確かめる）ので消した。`side_io.rs` の単体テストのうち比較先の操作を通るもの（ローカルの経路の読み書き・symlink・大きさの上限、エージェントの経路のエラー応答・接続断・答えのないパス・走査の失敗）は `target_io_tests.rs` に移し、Side 単位の API そのものを確かめていたものは API と一緒に消した。root_dir と include の値の確認の要求を一件ずつ数えていた scan_partial_starts のテストは、まとめて一度に尋ねる要求を数える形にした。
  - why: [F11](#F11)。
  - decided_by: Claude（F11 と F46 から導いた。自走の指示により決めた）

- F48 古い木（core の `tree.rs` の `FileTree`・`FileNode`、`scan/tree_build.rs`、engine の `scan_result` の木にする関数と循環で止めた木の誤り、filter の木と子に include を当てる関数）を消す。段 1〜4 の部品のうち古い木の型を使っていた子の一覧（core の `scan::children` の `ChildListing`）は、子を走査の一覧の項目（`ListedEntry`。パスは root_dir からのもの）で持つ形にし、`接続断` の後に権限の変更を確かめるとき（`change_check::mode_in_listing`）は、親の子の一覧からそのパスの項目の権限を読む。子の一覧を確かめる契約テスト（path_operations_agree）と、走査の答えを経路で比べる契約テスト（scan_routes_metadata・scan_times_agree）は、木のノードの代わりに一覧の項目から同じ欄（種類・リンク先・リンク先がディレクトリか・大きさ・権限・更新時刻の秒とナノ秒と読めた桁）を比べる形にした。木だけを確かめていた単体テスト（core の tree と tree_build、filter の木の関数、engine の scan_result の木と循環の木）は木と一緒に消し、scan_result の単体テストは一覧にする関数で同じ誤りを確かめる形にした。コマンドの入口のパスの検査（engine の `service/path_resolver`）は `parse_path_arguments` と、diff・merge・sync が同じ文言で止まるための `paths_not_found` だけにした。使い手のない `merge/executor`（engine と製品）と runtime の `size_limit` を消し、大きさの上限は core の `MAX_FILE_SIZE` を使う。リモートのサーバを相手にする契約テストの名前の "over_ssh" と、ファイル名の `merge_cli_ssh.rs`・`rollback_path_ssh.rs` は、SSH の経路をなくした（[foundation A12](./2026-10-12-foundation.md#A12)）ので "on_the_remote_server" と `merge_cli_remote.rs`・`rollback_path_remote.rs` にした。
  - why: 計画 S8 の「古い木の型のうち、段 1〜4 の部品が使っているものをどの型に置き換えるか」。走査の一覧の項目は、木のノードが持っていた欄をすべて持ち、子の一覧と走査で同じ型になる。比べる欄は変わらないので、段 1〜4 の契約テストの確かめる中身は変わらない（計画 S8 の止める条件に当たらない）。
  - decided_by: Claude（計画 S8 から導いた。自走の指示により決めた）

- F49 段 5 の最後に F27 の手順を今のコードで走らせ直し、tmpfs と ext3 の二通りとも、エージェントの答えが手元と同じで、エージェント・core・protocol の単体テストが root と一般の利用者で通ることを確かめた（2026-10-13）。走らせ直して、F38 で直したとしたテストのうち二つの組が VM の中ではまだ落ちることが分かった。F38 の後に VM で確かめ直していなかった。一つは、配置先の検査のテストが root で落ちたことで、VM の根（initramfs の根）が作った手元の利用者の持ち物のままで、検査が祖先の持ち主を root か本人かで見るためだった。VM の init で根を root の持ち物にした（手順の側の直し）。もう一つは、走査の更新時刻のテストが ext3 で落ちたことで、ext3 が秒の精度しか持たないのにナノ秒を求めていた。ファイルシステムが秒より細かい時刻を持てないときは秒の精度として読むことを確かめる形にした（製品の振る舞いは [F33](#F33) のまま）。前の回が権限を外したまま残した作業ディレクトリで手順が止まることも直した。
  - why: [F27](#F27)・[F18](#F18)。テストの直しは確かめることを変えず、環境によらない形にしただけである（[F38](#F38) と同じ）。
  - decided_by: Claude（F27 の検証の結果から導いた。自走の指示により決めた）

- F50 バックアップのセッションを終えるときは、そのセッションのものだけを片付ける。何も保存しなかった領域とセッションそのものとその予約は除くが、集約先の共有の場所（sessions・targets・reservations と集約先そのもの）と書き込み先の記録（targets の下）は消さない。消すと、同時に動く別の merge・sync・rollback が予約の場所を作った後で自分の予約を作る前に場所を失い、集約先の誤りで止まる（品質レビュー 1 回目の台帳の backup_rollback-8）。片付けの失敗は、ないか空でないとき（消すものがないか保存したものがある）のほかは警告に残す。何も保存しなかった操作の後に集約先が空になることを確かめていた backup_storage の二つのテストは、[REQ-backup-015](../../ir/backup/storage.md#REQ-backup-015) のとおりセッションが残らないことを確かめる形にした。
  - why: PROJECT.md の Security boundary の「同時に動く別の作業で、ツールが…黙って失敗する正しさの問題」は直すもの（[foundation A11](./2026-10-12-foundation.md#A11)）。REQ-backup-015 が残さないとするのはセッションで、共有の場所ではない。
  - decided_by: Claude（foundation A11 と REQ-backup-015 から導いた。自走の指示により決めた）

- F51 利用者に向けた設定の警告（"../" を含む exclude、include の値、auth が key のときの password、走査の include の値）は、記録の細かさの指定（--log-level・RUST_LOG）によらず標準エラーに出す。これらの警告は tracing の target `remote_merge::warning` に出し、標準エラーの層はその target の警告を常に通す。--log-level off でも出る（品質レビュー 1 回目の台帳の config_cli-7）。
  - why: [REQ-config-038](../../ir/config/filters.md#REQ-config-038) と [REQ-config-032](../../ir/config/include.md#REQ-config-032) は警告を標準エラーに出すと決めていて、記録の細かさで消してよいとは決めていない。--log-level は診断の記録の細かさの指定である。
  - decided_by: Claude（REQ-config-038・032 から導いた。自走の指示により決めた）

- F52 init は、問いへの答えの入力が終わったら同じ問いを繰り返さずに誤りで止まり、設定を書かない。書くひな形の値は TOML の文字列としてエスケープして書き、表の名前は英数字・"_"・"-" だけなら裸のまま、ほかは引用符で囲む（品質レビュー 1 回目の台帳の config_cli-10）。
  - why: [REQ-cli-012](../../ir/cli/init.md#REQ-cli-012) の「設定のひな形を生成する」は、読める設定を書くことと、入力が終わったときに止まることを前提にしている。入力の終わりで問い続ける（終わらない）ことと、引用符や逆斜線を含む値で読めない設定を書くことは、どちらもひな形を生成しないので、IR を変えずに直す。止め方の選択肢は実質一つ（書かずに止める）。
  - decided_by: Claude（REQ-cli-012 から導いた。自走の指示により決めた）

- F53 sync は、読み込み元の走査の一覧と、読み込み元について読んだハッシュと中身を、書き込み先の数によらず一度だけ集めて、どの書き込み先との比べ方にも使い回す（品質レビュー 1 回目の台帳の merge_sync-15）。書き込み先ごとに比べることと、書き込み先の側を書き込み先ごとに集めることは変えない。そのため、二つ目からの書き込み先は、一つ目の書き込み先を比べるときに集めた読み込み元の状態と比べ、その中身を書く。
  - why: [REQ-compare-011](../../ir/compare/reading.md#REQ-compare-011) は側ごとにまとめて一度で読むと決めている。書き込み先ごとに読み込み元を集め直すと、走査と読み込みが書き込み先の数だけ増え、書き込み先ごとに違う時点の読み込み元を書きうる。どの書き込み先にも同じ読み込み元を書くほうが sync の目的に合う。変更の直前の判定（[REQ-compare-015](../../ir/compare/reading.md#REQ-compare-015)）は書き込み先の側だけを集め直すので変わらない。
  - decided_by: Claude（REQ-compare-011 から導いた。自走の指示により決めた）

- F54 strict_host_key_checking の知らない値を ask として読む古い動きを固めていたテストを、[REQ-config-018](../../ir/config/values.md#REQ-config-018) のとおり設定のエラーで止まることを確かめる形に直した。直したのは、remote-merge-config の `test_parse_strict_host_key_checking_unknown_falls_back_to_ask`（知らない値が誤りになることを確かめる形）、config_values の `unknown_strict_host_key_checking_value_is_read_as_ask`（設定を読むところで止まる形）、config_values_cli の `unknown_strict_host_key_checking_value_warns_and_falls_back_to_ask` と `ask_value_in_any_case_does_not_warn`（status が比べる前に終了コード 2 で止まる形と、ask の値は大文字と小文字によらず誤りにならない形）である（品質レビュー 1 回目の台帳の config_cli-6）。
  - why: [F11](#F11)。IR はすでに設定のエラーで止まると決めていた（2026-10-05 の quality-config A2）。
  - decided_by: Claude（F11 と REQ-config-018 から導いた。自走の指示により決めた）

- F55 diff は、--max-files の枠の外に落ちた変更のあるファイルも、今のまま中身を読んで比べる（品質レビュー 1 回目の台帳の diff-22 は「仕様が求める」として閉じる）。
  - why: [REQ-cli-055](../../ir/cli/diff-output.md#REQ-cli-055) の changed_files_total は枠の外を含む変更のあるファイルの総数で、読み込みの失敗を errors に載せ終了コード 2 を返す要件（[REQ-cli-089](../../ir/cli/unreadable.md#REQ-cli-089)・[REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090)）は、出すファイルの枠で分けていない。枠の外を読まないと、その失敗が errors と終了コードに出なくなる。
  - decided_by: 利用者（2026-10-13）

- F56 [REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) から diff を外す。diff は [段 4 E33](./2026-10-12-foundation-4.md#E33)（[REQ-compare-003](../../ir/compare/flow.md#REQ-compare-003)）のとおり、途中の祖先がディレクトリでない側ではそのパスはないとして示す。merge・sync とバックアップの作成は、今までどおり "cannot resolve path: not a directory" で failed にする（[EX-paths-022](../../ir/paths/routes.md#EX-paths-022) のまま）。[F36](#F36) で段 5 の報告に返した食い違いはこれで閉じる。
  - why: 読むだけの diff は、途中がディレクトリでないことをそのパスがないこととして集めた状態から決められる（E33）。書き込みとバックアップは、そのパスへ書けないので失敗として示す。
  - decided_by: 利用者（2026-10-13）

- F57 リンク先を辿れない symlink（ループしている・途中がディレクトリでない）で、読むだけのコマンド（diff・status）は止まらない。走査はその symlink をリンク先を辿れないものとして一覧に載せて下を辿らず、比べる流れは [TBL-compare-001](../../ir/compare/judgement.md#TBL-compare-001) の、その他の種類の次の行で判定できないとする（理由は "cannot follow the symlink target: " に続く側の名前。[F5](#F5) の形）。diff・status はそのパスと理由を errors に出し、ほかの差分を出して終了コード 2 を返す。merge・sync・rollback は、比べる範囲の一覧にリンク先を辿れない symlink があれば、今までどおり書き込みを始める前に "Cannot follow symbolic link (loop or non-directory in its target): パス" の誤りで止める（一覧に載せた後で、比べる流れの側の集め方が止める）。名前で指定した起点そのもの（指定したディレクトリの起点）がリンク先を辿れない symlink のときは、今までどおり走査の誤りにし、diff はその名前を一覧を取れなかったものとする（[REQ-cli-096](../../ir/cli/directory-paths.md#REQ-cli-096)）。[REQ-scan-018](../../ir/scan/directory-links.md#REQ-scan-018) をそう書き直し、REQ-cli-096 の「配下の走査がリンク先を辿れない symlink に当たったら指定したパス全体を一覧を取れなかったものにする」を外す。一覧の symlink のリンク先の種類が一つ増えるので、通信の版を 14 にする。走査が止まることを固めていた単体テスト（core の drive_tests）と契約テスト（scan_unresolvable_link_cli と、指定したディレクトリの配下の辿れない symlink で名前全体を errors に出すことを確かめていた diff_named_directory_scan。後者は [EX-cli-098](../../ir/cli/directory-paths.md#EX-cli-098) も書き直した）は、書き直した REQ-scan-018 のとおり、載せることと errors に出すことを確かめる形にした（品質レビュー 1 回目の台帳の pv-5）。
  - why: [REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022) は、読めない参照先の symlink があっても比べられた範囲を示し、そのパスと理由を errors に出してエラー終了するとしている。辿れない symlink 一つで diff 全体が止まるのはこれと食い違っていた。merge・sync は書き込む前に止めるほうが安全なので変えない。
  - decided_by: 利用者（2026-10-13）

- F58 diff で二つの symlink のリンク文字列が違い、辿った先の組が判定できない（辿った先の片方がその他の種類）ときは、そのパスを差分ありでなく判定できないとし、errors に "not a regular file: 側" を出す。辿った先がその他の種類の側を空のファイルとした差分は出さない。リンクの組と辿った先の組を合わせる関数の単体テスト（method_tests）は、リンク文字列が違えば辿った先によらず差分ありとする今までの動きを固めていたので、REQ-compare-009 のとおりに直した（段 5 のレビューの sd-1）。
  - why: [REQ-compare-009](../../ir/compare/judgement.md#REQ-compare-009) は [段 4 E34](./2026-10-12-foundation-4.md#E34) で、辿った先のどちらかがその他の種類なら読まずに判定できないと明示しており、[REQ-compare-008](../../ir/compare/judgement.md#REQ-compare-008) の「どちらかが同じでなければ差分あり」より具体的な規則である。差分ありにすると、読まなかった側を空として差分を出し、[REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022) の「読めない参照先を空ファイルと同一扱いしない」に反する。
  - decided_by: Claude（REQ-compare-009・E34・REQ-cli-022 から導いた。自走の指示により決めた）

- F59 --hunks の merge で当てた中身の末尾の改行の有無は、最後の行が変更でそれを含む hunk を選んだときは読み込み元の、そうでなければ書き込み先のものにする。全部の hunk を選べば読み込み元の中身に、一つも選ばなければ書き込み先の中身になる（段 5 のレビューの mwr-3）。
  - why: [REQ-merge-009](../../ir/merge/hunks.md#REQ-merge-009) は選んだ変更だけを当て、選ばなかった変更を残すとする。末尾の改行の有無は最後の行の違いとして hunk に入るので、その hunk を選べば当て、選ばなければ残すのがこの要件の読み方である。今までは書き込み先の有無をいつも使い、選んだ変更の一部を当てていなかった。
  - decided_by: Claude（REQ-merge-009 から導いた。自走の指示により決めた）

- F60 --hunks の merge を、失敗の示し方と終了コードでファイル全体の merge に揃える。左右の中身を読めなかったときの error は、読めなかった側をどれも [REQ-compare-014](../../ir/compare/reading.md#REQ-compare-014) の "read failed: left: 原因; right: 原因" の形に並べる（今までは側の名前がなく、両側とも読めないと左だけを出していた）。書き込みの最中の `接続断` の後につなぎ直して書けたと確かめても、書き込み先への処理を止めたので終了コード 2 を返す（[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022)）。結果の形を作る関数は、行数の約束のため `merge_hunks_result.rs` に分けた（段 5 のレビューの mwr-2・mwr-4）。
  - why: REQ-compare-014 はどのコマンドでもこの形にすると決め、REQ-ssh-022 は止めたら済んだファイルがあっても終了コード 2 と決めている。ファイル全体の merge はどちらもそうしていた。
  - decided_by: Claude（REQ-compare-014・REQ-ssh-022 から導いた。自走の指示により決めた）

- F61 読み込み元の symlink のリンク文字列が UTF-8 として読めないときは、merge・sync の扱いの並びを作る段で "the link target of the source is not valid UTF-8" の failed にする。--dry-run も同じ failed になり、書く予定に出さない（段 5 のレビューの mwr-7）。
  - why: そのリンクは作れないので、実行では必ず失敗する。[REQ-compare-005](../../ir/compare/flow.md#REQ-compare-005) は実行時に書かないファイルを書く予定に出さないとし、[REQ-cli-091](../../ir/cli/safety.md#REQ-cli-091) は --dry-run が実行時と同じ失敗を出すとする。書き込み先の側の確かめられないリンク（[REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033)）と同じく、集めた状態だけで分かるので計画の段で決める。
  - decided_by: Claude（REQ-compare-005・REQ-cli-091 から導いた。自走の指示により決めた）

- F62 --ref の diff のテキストで、バイナリのファイルのファイル全体の競合も、そのファイルの区間に "Conflicts: 1 region(s) …" の行を出す。出力の書き出し（service/output.rs）がバイナリのハッシュの行を出した後に戻り、競合の行を出していなかった（段 5 のレビューの sd-3）。
  - why: [段 4 E16](./2026-10-12-foundation-4.md#E16) はバイナリもファイル全体を一つの箇所として競合を示すとし、[REQ-cli-065](../../ir/cli/conflicts.md#REQ-cli-065) はテキストで各ファイルの競合と末尾の合計を示すとする。末尾の合計にだけ数えると、どのファイルの競合か分からない。書き出しは [F7](#F7) で残す物差しだが、判定ではなく書き出しの抜けなので直す。
  - decided_by: Claude（E16・REQ-cli-065 から導いた。自走の指示により決めた）

- F63 --ref の diff は、参照先の中身を、参照先が通常ファイルで左右がどちらも通常ファイルかないときだけ読む。参照先が左右と種類の違うもの（通常ファイルを指す symlink を含む）なら読まず、その読み込みの失敗で左と参照先の組を判定できないにして errors に出すこともしない（段 5 のレビューの sd-2）。
  - why: [REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090) は、参照先が左右のファイルと種類の違うものなら参照先を読まずに "ref" だけを出し、errors に載せないとする。参照先との差（[REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063)）を示すのもその組だけなので、ほかの組で読んだ中身は使われない。
  - decided_by: Claude（REQ-cli-090・REQ-cli-063 から導いた。自走の指示により決めた）

- F64 --ref の diff で参照先との組が判定できない（参照先を読めない・その他の種類）ときは、左右に差のないファイルでも、--max-files の枠の外に落ちたファイルでも、そのパスと理由を errors に出して終了コード 2 にする。errors だけに出し、左右に差のないファイルを files に加えることはしない（段 5 のレビューの sd-8・sd-9）。
  - why: [REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090) は、状態ではあった参照先のファイルの読み込みが失敗したら、そのパスと理由を errors に載せて終了コード 2 を返すとし、左右の差の有無や出す枠で分けていない。[REQ-cli-067](../../ir/cli/reference.md#REQ-cli-067) が左右に差のあるファイルに限るのは参照先との差（"ref_hunks"）で、errors ではない。status も同じ組を errors と終了コード 2 にしている（[REQ-cli-088](../../ir/cli/unreadable.md#REQ-cli-088)）。枠の外のファイルも読むことは [F55](#F55) で決めた。
  - decided_by: Claude（REQ-cli-090・REQ-cli-067・REQ-cli-088 と F55 から導いた。自走の指示により決めた）

- F65 配置先を直して置き直したときの警告（[REQ-ssh-018](../../ir/ssh/deploy.md#REQ-ssh-018)）と、リモートで SHA-256 を取れずに照合しないまま実行するときの警告（[REQ-ssh-016](../../ir/ssh/deploy.md#REQ-ssh-016)）も、[F51](#F51) と同じく tracing の target `remote_merge::warning` に出し、--log-level・RUST_LOG によらず標準エラーに出す。前者は --log-level error の契約テストで確かめた。後者は SHA-256 を取る道具のない試験サーバを用意していないので、同じ target に出すことをコードで揃えただけで、テストでは確かめていない。配置の契約テストのファイルは行数の約束のため、試験サーバと配置先の組み立てを `ssh_agent_deploy_support.rs` に分けた（段 5 のレビューの rt-2）。
  - why: F51 と同じ。これらは利用者に向けた警告で、IR は表示すると決めており、記録の細かさで消してよいとは決めていない。
  - decided_by: Claude（F51・REQ-ssh-016・REQ-ssh-018 から導いた。自走の指示により決めた）

- F66 期限切れのバックアップの整理は、同時に動く別の merge・sync が同じ期限切れのセッションを先に消していた（消そうとした領域・セッションがない）ときは、消すものがないとして続ける。集約先の sessions とその項目を読めないときと、ない以外の理由で消せないときは、黙って 0 件にせず、そのパスと理由を示す誤りにする（今までどおり merge・sync は書く前に止まる）。整理の関数は行数の約束のため `backup_store_cleanup.rs` に分けた（段 5 のレビューの mwr-5・mwr-8・rt-10）。同じ指摘が挙げたほかの握りつぶし（セッションの予約の `filter_map(Result::ok)`、本来の誤りを返した後の一時物の片付けの `let _`、rollback の書き戻す前のハッシュの失敗を「分からない」にする扱い）は、結果を変えないか安全側に倒れるので変えない。
  - why: PROJECT.md の Security boundary は、同時に動く別の作業でツールが壊す・黙って失敗する正しさの問題を直すとする（[foundation A11](./2026-10-12-foundation.md#A11)）。並行するデプロイで二つの merge が同じ期限切れのセッションを整理するのはふつうに起きる。[F50](#F50) でセッションを終えるときに同じ問題を避けたのと同じ考えである。読めないのを 0 件にすると、古いバックアップが残り続けることが利用者に分からない。ほかの段の集約先の読み込みの失敗も、パスを示して止めている（[REQ-backup-041](../../ir/backup/rollback-cli.md#REQ-backup-041)）。
  - decided_by: Claude（foundation A11・F50・REQ-backup-025 から導いた。自走の指示により決めた）

- F67 エージェントのまとめて読む答え（ReadFiles）を組み立てるときは、各片の more_to_follow を見る。続きがあると言った中身が最後の応答までに続かないときと、終わった中身にさらに片が来たときは、途中までの中身を読めた中身にせず、要求と合わない応答（`接続断`）にする（段 5 のレビューの rt-8）。
  - why: クライアントの答えの組み立ては、欠けや余りを要求と合わない応答にすると決めている（agent の client の ops.rs）。続きのない片を最後とみなすと、切れた中身で比べ・書き込みが進む。ネットワーク越しの応答の解釈は直す対象である（[foundation A11](./2026-10-12-foundation.md#A11)）。
  - decided_by: Claude（ops.rs の取り決めと foundation A11 から導いた。自走の指示により決めた）

- F68 エージェントにいくつものパスをまとめて確かめる要求（InspectPaths）も、読み込みとハッシュと同じく一つの要求で 2000 件までに分けて送り、答えを尋ねた順につなぐ（段 5 のレビューの rt-1）。書き込み先の確認（merge・sync）と rollback の事実を集める問いは、どちらもこの入口を通る。
  - why: [docs/remote-tools.md](../../remote-tools.md) は一度に送るパスを 2000 件まで、通信の枠を 16MB としている。分けないと、多くのファイルを変える merge で要求か答えが枠を超え、その書き込み先の変更がすべて失敗する。ローカルの経路は同じ入力で成功するので、[REQ-paths-008](../../ir/paths/routes.md#REQ-paths-008) の経路によらない結果にも反する。
  - decided_by: Claude（docs/remote-tools.md と REQ-paths-008 から導いた。自走の指示により決めた）

- F69 merge・sync で、比べた後・最初の変更の前に書き込み先の root_dir がなくなったと、変更の前にまとめて尋ねること（書き込み先のパスの確認か、バックアップする中身のまとめた読み込み）で分かったときは、root_dir がない誤りを最初のファイルにだけ出し、残りのファイルは手を付けていない失敗にする。バックアップのまとめた読み込みで分かったときの最初のファイルの error は、今までどおり "backup failed: read failed: " に続く root_dir がない誤りにする。まとめた読み込みが `接続断` で止まったときは、今までどおりまとめのファイルをすべて理由付きの読み込みの失敗にする（段 5 のレビューの mwr-1）。
  - why: [REQ-merge-044](../../ir/merge/concurrency.md#REQ-merge-044) は、そのファイルを root_dir がない error に、まだ処理していない残りを手を付けていない error にし、root_dir がない文言を一度だけ出すとする。変更の途中で消えたときはすでにそうしていて、変更の前のまとめた問いで分かったときだけ、全ファイルに同じ root_dir がない文言（しかも別のパスの名前）を出していた。`接続断` の扱いは [REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003) が別に決めている。
  - decided_by: Claude（REQ-merge-044・REQ-ssh-003 から導いた。自走の指示により決めた）

- F70 diff で名前で指定したディレクトリがいくつあっても、件数の上限は指定した比較全体に一度だけ当てる（起点を一つの組にする）。merge・sync は今までどおり指定したディレクトリごとの走査に当てる。比べ方の求めにその当て方（`EntryLimit`）を持たせた（段 5 のレビューの sd-4。レビュー役は IR の読み方に依るとしたが、上の契約テストでディレクトリごとに当てていたことを確かめた）。root_dir 全体の diff で、exclude に当たって全体の走査に載らなかった名前を比べ足す走査（[REQ-cli-081](../../ir/cli/diff-strategy.md#REQ-cli-081)）は、全体の走査とは別の走査なので、その上限は別に数えるままにした。
  - why: [REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) は、指定した比較全体で既存の最大走査件数を一回だけ適用するとし、[REQ-cli-084](../../ir/cli/diff-strategy.md#REQ-cli-084) はパスの数と指定の仕方によらず上限の超過を errors に載せるとする。ディレクトリごとに当てるのは merge・sync の表（[TBL-scan-001](../../ir/scan/limits.md#TBL-scan-001)）である。
  - decided_by: Claude（REQ-cli-021・REQ-cli-084・TBL-scan-001 から導いた。自走の指示により決めた）

- F71 コマンドが参照先との組を扱いに使わないパスでは、比べる流れは参照先との組を判定せず、そのためのハッシュも取らない。status は左右の両方にあり種類（通常ファイル・ディレクトリ・末尾の symlink）がそろうときだけ、diff は左右がどちらも通常ファイルかないときだけ参照先との組を使う（コマンドが告げる `Uses::judges_reference`）。merge・sync は今までどおりどのパスでも使う（段 5 のレビューの sd-10）。
  - why: [TBL-cli-005](../../ir/cli/status-targets.md#TBL-cli-005) は、左右の片方だけにあるときと三つの種類がそろわないときは中身を読まずに印を決めるとする。diff が参照先との差と参照先を読めないことを示すのも左右が通常ファイルかないときだけである（[REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063)・[REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090)）。使わない組のハッシュを取ると、その読み込みの `接続断` でコマンド全体が止まりうる。
  - decided_by: Claude（TBL-cli-005・REQ-cli-063・REQ-cli-090 から導いた。自走の指示により決めた）

- F72 merge・sync（--hunks を含む）が比べるときに書き込み先の中身のハッシュをまとめて取る処理が、つなぎ直しても `接続断` になったときは、コマンドのエラーで止めず（sync は path "" の一件の failed にせず）、そのまとめに入っていたファイルを、右との組を "read failed: right: 原因" の判定できないとして failed に出す。書き込み先への処理はそこで止めたので、残りの書き込む予定のファイルは手を付けていない失敗になり、終了コード 2 を返す。読み込み元と参照先の側のまとめた問いの `接続断` と、status・diff は、今までどおりコマンドを止める（段 5 のレビューの mwr-6。レビュー役は比べる段を「まとめて読む処理」に含めるかの読み方に依るとしたので、上の契約テストで今はコマンドのエラーで止まることを確かめてから直した）。
  - why: [REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003) は、merge・sync・rollback で止めたときは [REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022) と同じ形で結果を出し、書き込み先の中身をまとめて読む処理で止めたらまとめのファイルをすべて理由付きの読み込みの失敗として failed に出すとする。比べるときのハッシュも書き込み先の中身を読む処理で、REQ-ssh-003 は読み取りの操作にハッシュを挙げ、段を限っていない。rollback の事実を集める問いは [F43](#F43) で同じ扱いにした。
  - decided_by: Claude（REQ-ssh-003・REQ-ssh-022・F43 から導いた。自走の指示により決めた）

- F73 SSH の経路だけが使っていたパスの確認の部品を消す。集めた要素の事実から次に尋ねる要素を決める手順（core の `path_resolution/facts.rs` の `PathFact`・`FactBook`・`next_step` とその単体テスト）と、SSH の経路でだけ起きた確認の結果のエラー "readlink not available"（`ResolveError::ReadlinkNotAvailable`）である。[REQ-paths-002](../../ir/paths/resolution.md#REQ-paths-002) の原因の文言の一覧からも消し、PROJECT.md の説明も直した（段 5 のレビューの rt-3・rt-4）。
  - why: [foundation A12](./2026-10-12-foundation.md#A12) で SSH でファイルを操作する経路を消し、[F23](#F23) は SSH の経路がなければ起きないものを文書に残さないとした。ローカルとエージェントの read_link の失敗は "cannot read" に写すので、どの経路でもこの文言は出ない。使い手のない部品を残すと二重の実装になる。
  - decided_by: Claude（foundation A12・F23 から導いた。自走の指示により決めた）

- F74 SSH でファイルを操作する経路だけが使っていた SSH の接続の部品を消す。`SshClient` の `is_alive`・`run_streaming_with_input`・`run_bytes`・`exec_strict_bytes`（と ssh crate の同名の包み）と、core の `AppError::RemoteParse` である。`run_bytes` で SSH の一括読み込みの切断を確かめていた ssh_integration のテストは、今も使う経路（エージェントを置くコマンド）の `run_with_input` で、終了ステータスなしに切れた出力を接続断にすることを確かめる形にした（段 5 のレビューの rt-5）。
  - why: [foundation A12](./2026-10-12-foundation.md#A12)・[F23](#F23)。使い手がないことは、消した後の組み立てと lint（未使用の警告を含む）で確かめた。
  - decided_by: Claude（foundation A12・F23 から導いた。自走の指示により決めた）

- F75 scan/limits.md の冒頭の説明から、[F23](#F23) で消した SSH の走査の時間切れ（REQ-scan-013）を一覧が途中までになる理由に挙げていた語を消す（段 5 のレビューの rt-6）。
  - why: [F23](#F23)。その要件はもうない。
  - decided_by: Claude（F23 から導いた。自走の指示により決めた）

- F76 設定を渡さないテストの起動（tests/common の `remote_merge_cmd`）は、HOME と XDG_CONFIG_HOME も一時ディレクトリの下に向け、利用者の全体の設定・known_hosts・鍵を読まないようにする。merge が既定のサーバにつないで失敗することを確かめる cli_error_handling のテストは、開発機の localhost:22 の sshd に向けず、一度開いて閉じた手元のポートに向ける（段 5 のレビューの rt-11）。テストの道具の直しなので、落ちるテストは書けない（今までのテストは開発機に sshd がなければ同じに通っていた）。
  - why: テストは利用者のファイルに触れない（[foundation-1 B9](./2026-10-12-foundation-1.md#B9) と、この作業の約束）。
  - decided_by: Claude（foundation-1 B9 から導いた。自走の指示により決めた）

- F77 uname の答えが配布にないターゲット（ARM の Linux など）か、uname で OS とアーキテクチャを確かめられないサーバには、手元のターゲットのバイナリを置かず、その理由で配置できないエージェントの状態にする（読み書きの前に終了コード 2 で止まる）。今までは手元のターゲットのバイナリを書き込んでから、版の不一致の理由で止まっていた（段 5 のレビューの rt-7）。
  - why: [REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) は、配布にないサーバ（ARM の Linux を含む）を配置できないサーバとして止めるとする。確かめられないときに手元のバイナリを置くと、動かないバイナリを書き込み、止める理由も配布にないことと分からない。
  - decided_by: Claude（REQ-ssh-005 から導いた。自走の指示により決めた）

- F78 段 5 のレビューの info のうち、次のものは今の振る舞いを変えない。
  - sd-6（--ref の status で左右の片方だけにあるファイルの参照先がその他の種類でも "differs" にし、errors に出さない）: [TBL-cli-005](../../ir/cli/status-targets.md#TBL-cli-005) の「片方だけにある・ある・中身 -・differs」の行が当たり、中身を読まずに印を決めると表が決めている（[F71](#F71) で参照先との組も判定しない）。三つともにあるときに errors に出るのは、同じ表の「三つとも同じ種類で、どれかを読めない」の行による。
  - sd-5（左右がテキストで参照先だけがバイナリのとき、"ref" だけを出し "ref_hunks" も "ref_hash" も出さない）・sd-7（--follow-external-links なしの --ref の diff で、参照先の側だけが root_dir の外を指す symlink の下にあるパスを errors に出さない）・mwr-9（中身は書いたが権限の変更に失敗したファイルだけの書き込み先の sync の status を "failed" にする）・rt-9（エージェントを使えないサーバへの rollback が、確認の問いに答えた後につないで止まる）: IR が決めていない（sd-5 は [REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063)・[REQ-cli-100](../../ir/cli/reference.md#REQ-cli-100) がこの組を挙げていない、sd-7 は [REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026) の内容未検証の理由を参照先の側に出すかを決めていない、mwr-9 は [TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007) の「書き込めたファイル」に中身だけを書いたファイルを含めるかを決めていない、rt-9 は [REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) の読み書きの前に止めることは守っており、確認の問いとつなぐ順を決めていない）ので、利用者に返す。
  - why: 決めていないことを実装者が決めると、IR にない振る舞いを固めることになる。
  - decided_by: Claude（TBL-cli-005・F71 から導いた（sd-6）。ほかは IR が決めていないので決めない。自走の指示により決めた）

- F79 エージェントの書き込みの中継は、橋渡しの側が先に終わって最後のバイト列を送れなかったことを、利用者への警告でなく診断の記録（debug）にする。終わりの確かめで一度だけ落ちた診断のテスト（diagnostics の a_cli_run_continues_with_the_same_output_when_diagnostics_cannot_be_saved）は、二回の status の標準エラーをくらべ、この警告が片方にだけ出て落ちていた。
  - why: セッションの終わりに橋渡しと手元のどちらが先に終わるかは実行ごとに違い、同じ status でも標準エラーが変わっていた。本当の切断は手元の読み込みが `接続断` として知らせるので、この警告は利用者に知らせることを持たない。
  - decided_by: Claude（落ちたテストの原因から導いた。自走の指示により決めた）

- F80 比べた後・最初の変更の前に書き込み先の root_dir がなくなったときの扱い（[F69](#F69)）を、merge・sync・--hunks・rollback と --dry-run のすべてに、パスの深さによらず同じに当てる。パスをまとめて確かめる問い（比べる流れの起点の途中の確認・merge と sync の書き込み先の確認・rollback の事実を集める問い）は、どれも root_dir そのものを先頭に足して尋ね、その答えで決める（runtime の `root_checked`。`RootWatch` は問いに root_dir があれば確かめ直さない）。変更の前のまとめた問いで分かったときは、そのまとめの最初のファイル（書き込み先の確認ならパスの順の最初、バックアップや退避の読み込みならその読み込みの最初のファイル）にだけ、そのファイルの名前の root_dir がない error を出し、ほかは手を付けていない error にする。rollback は退避の読み込みでも同じにする。rollback が事実を集める最初の問いで root_dir がないときは、パスの深さと --dry-run によらず、走査の root_dir がない誤り（[REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015)）と同じにして止める（段 5 の 2 回目のレビューの mwr-r2-1・mwr-r2-3・mwr-r2-4）。
  - why: [REQ-merge-044](../../ir/merge/concurrency.md#REQ-merge-044) はどのコマンドでも同じ形を求める。今までは root 直下のパスだけを確かめるとき問いに root_dir が入るので `RootWatch` が止めず、--dry-run が全部を "would merge" にして終了コード 0 を返し、rollback は退避の読み込みで全ファイルに最初のファイルの名前の同じ文言を出し、rollback の事実の問いはパスの深さで誤りの文言が変わっていた。バックアップの読み込みで分かったのに、バックアップしない最初のファイルに別のファイルの名前の文言を付けていた。尋ねた段で root_dir の答えを見れば、深さによる差がなくなる。
  - decided_by: Claude（REQ-merge-044・REQ-scan-015・F69 から導いた。自走の指示により決めた）

- F81 merge・sync（--hunks・--dry-run を含む）で書き込み先へのまとめた問いがつなぎ直しても `接続断` になったときの扱い（[F72](#F72)）を、要求の種類によらず一か所（runtime の `ask_by_role`）で決め、比べるときのハッシュ（HashContents）にも中身（ReadFiles。--hunks が書き込み先の中身を読む）にも当てる。中身を失ったときも、そのまとめのファイルの右との組を "read failed: right: 原因" の判定できないとして failed に出す。変更の前の書き込み先のパスのまとめた確かめ（InspectPaths）がそうなったときも、どのファイルも同じ "read failed: right: 原因" で failed に出す（今までは原因だけで、読み込みの失敗と分からなかった）。書き込み先の走査（ListTree）と、外へのリンクの扱いのために起点の途中を確かめる問いは、ファイルのまとめではなくディレクトリの問いなので、今までどおり merge はコマンドを、sync はその書き込み先を止める（段 5 の 2 回目のレビューの mwr-r2-2）。
  - why: [REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003) は読み取りの操作を挙げて段を限らず、まとめのファイルをすべて理由付きの読み込みの失敗にするとする。どの問いで切れたかで、ファイルごとの失敗になるかコマンドのエラーになるかが変わっていた。rollback の事実を集める問いは [F43](#F43) で同じ扱いにしてある。
  - decided_by: Claude（REQ-ssh-003・F72・F43 から導いた。自走の指示により決めた）

- F82 リンク先を辿れない symlink は、名前の形によらず走査を止めない（[F57](#F57) の残り）。名前が UTF-8 として読めないものは、今までどおり読めない名前として報告し（[REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012)）、そのうちリンク先を辿れないものを一覧の `unresolvable_names` に残す。diff・status は読めない名前として errors に出してほかの差分を出し、merge・sync・rollback は名前の読める辿れない symlink と同じく書き込みを始める前に止める。走査を止めるのは、指定したディレクトリの起点そのものが辿れないとき（下を集められない）だけにした。一覧を運ぶ TreeChunk に `unresolvable_names` を足したので、通信の版を 15 にする（段 5 の 2 回目のレビューの sd-r2-1）。
  - why: [REQ-scan-018](../../ir/scan/directory-links.md#REQ-scan-018) は読むだけのコマンドを止めないとし、名前の形で分けていない。今までは待たせたリンクが一覧の位置を持つかだけを見ていて、起点と名前を読めないリンクを見分けられず、名前を読めないリンクで status・diff 全体が止まっていた。止めるコマンドに辿れないことを伝えないと、merge が REQ-scan-018 の止める扱いを外れる。
  - decided_by: Claude（REQ-scan-018・REQ-scan-012・F57 から導いた。自走の指示により決めた）

- F83 diff の件数の上限は、名前で指定したファイル・ディレクトリの下・root_dir 全体と一緒に名前で比べ足すパスのどれも、指定した比較全体に一度だけ当てる。名前で指定した起点そのものを集める走査と、比べ足す走査は、前の走査が側ごとに数えた件数（一覧の `counted`。通信の版 15 で TreeChunk に足した）の残りを上限にし、その打ち切り・循環・読めない名前を一覧に残す。名前で指定した起点のうち、ディレクトリとディレクトリへの symlink は下を展開する器なので数えず（今までの、ディレクトリへのリンクの下が上限ちょうどなら完全に比べるという振る舞いのまま）、それ以外の種類は数える。左右のどちらかの一覧が途中までのときは、載らなかった名前を "not found on either side" にしない。[F70](#F70) の、比べ足す走査の上限を別に数えるという部分はこれで変える。merge・sync はディレクトリごとに当てるまま（打ち切ったことは同じく残す）（段 5 の 2 回目のレビューの sd-r2-2）。
  - why: [REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021) は指定した比較全体で一回だけ当てるとし、[REQ-cli-084](../../ir/cli/diff-strategy.md#REQ-cli-084) はパスの数と指定の仕方によらず上限の超過を errors に載せるとする。今までは起点そのものを集める走査の打ち切りを捨てていたので、上限を超えた名前のファイルが黙って「両側にない」になり、終了コード 1 を返していた。打ち切った一覧から「ない」とは決められない。
  - decided_by: Claude（REQ-cli-021・REQ-cli-084・REQ-scan-010・REQ-cli-057 から導いた。自走の指示により決めた）

- F84 --ref の diff で左右がテキストで参照先だけがバイナリのときも、ほかのバイナリの組と同じく参照先の SHA-256 を "ref_hash" に出して参照先との違いを示し、"ref_hunks" を出さない（[F78](#F78) で返した sd-5）。[REQ-cli-100](../../ir/cli/reference.md#REQ-cli-100) に書き足し、[EX-cli-114](../../ir/cli/reference.md#EX-cli-114) を足した。左右と参照先のどれかがテキストとして示せなければ、"ref_hash" にする一つの規則にした。
  - why: 今までは "ref" だけを出し、参照先との違いを何も示さなかった。
  - decided_by: 利用者（2026-10-14）

- F85 --follow-external-links なしの diff で、参照先の側だけが root_dir の外へ解決されるか、外を指す symlink の下にあるパスも、左右の側と同じく内容未検証の理由（OUTSIDE_NOT_COMPARED と同じ文）を errors に出して終了コード 2 にする（[F78](#F78) で返した sd-7）。"ref" を付けないのは今までどおり（[REQ-cli-063](../../ir/cli/reference.md#REQ-cli-063)）。参照先の一覧が上限か循環で途中までのときは、左右の側と同じく打ち切りの誤りに任せる。[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026) に書き足し、[EX-cli-115](../../ir/cli/symlink-diff.md#EX-cli-115) を足した。status は --follow-external-links を使わないので当たらない。
  - why: 今までは参照先との組を比べないことを示さず、終了コード 1 を返していた。
  - decided_by: 利用者（2026-10-14）

- F86 sync で、中身は書けたが権限の変更に失敗したファイルと、symlink を置き換えるために古いリンクを消した後に新しいリンクを作れなかったファイルは、書き込み先を変えたものとして [TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007) の「書き込めたファイル」に数え、それしかない書き込み先の status を "partial" にする。ファイルそのものは failed に出す（[F78](#F78) で返した mwr-9）。変更の手順の結果に「書き込み先を変えた後で失敗した」（`Applied::FailedAfterChange`・`Outcome::FailedAfterChange`）を足し、権限の変更の失敗（作ったディレクトリの権限・--with-permissions の複製・新しいファイルの権限、`接続断` の後に残りの変更を行わなかったものを含む）と、古いリンクを消した後の作成の失敗の全部をそれにした。古いリンクを消した後に作れなかった error は、`接続断` のときだけでなく、作成が失敗したときも "the old symlink was removed, but the new symlink not created: " で始めて、消したことが分かるようにした。[REQ-cli-041](../../ir/cli/sync.md#REQ-cli-041) に書き足した。書き込みの途中で `接続断` になり「書きかけ」と確かめたファイルは、利用者の判断が挙げていないので含めない（利用者に返す）。
  - why: 書き込み先を変えたのに "failed" にすると、書き込み先に何も起きていないように見える。
  - decided_by: 利用者（2026-10-14）

- F87 rollback は確認の問い（"Restore N file(s) from session S to T? [y/N]"）を出す前に戻す先につなぎ、エージェントを使えなければ問いを出さずに理由を出して終了コード 2 で止める（[F78](#F78) で返した rt-9）。問いに y 以外で答えて止めるときは、つないだ戻す先を閉じてから "Aborted." を出す。[REQ-backup-035](../../ir/backup/rollback-cli.md#REQ-backup-035) に書き足した。確認の問いを出すほかのコマンド（sync の "Proceed? [y/N]"）は、もとから書き込み先につないで計画を立ててから問うので同じ順である。
  - why: 今までは問いに y と答えさせてから止まり、答えが無駄になっていた。
  - decided_by: 利用者（2026-10-14）

- F88 uname が Linux の aarch64 と答えたサーバは、配布にないサーバとして配置できない状態にする（[F77](#F77) の残り）。uname の答えの読み方（agent の `parse_remote_target`）が、配布していない aarch64-unknown-linux-musl を配置するターゲットとして返していたので、F77 の「配布にない」の分岐を通らず、手元にないバイナリの案内か、手元のターゲットのバイナリを置いた後の版の不一致で止まっていた。配布のターゲットは release のワークフローの x86_64-unknown-linux-musl・x86_64-apple-darwin・aarch64-apple-darwin で、使い手のなくなった定数 `TARGET_LINUX_AARCH64_MUSL` を消した。Linux の aarch64 を配置するターゲットとして読むことを固めていた単体テスト（parse_linux_aarch64 と、余分な空白の読み方を aarch64 で確かめていた handles_extra_whitespace）は [REQ-ssh-005](../../ir/ssh/fallback.md#REQ-ssh-005) と違う動きを固めていたので、配布にないことと x86_64 で確かめる形に直し、uname の本当の答えから止まることを確かめるテストを足した（段 5 の 2 回目のレビューの rt-r2-1）。
  - why: REQ-ssh-005 は、配布にないサーバ（ARM の Linux を含む）を配置できないサーバとして止めるとする。
  - decided_by: Claude（REQ-ssh-005・F77 から導いた。自走の指示により決めた）

- F89 設定の平文の password を使うときの警告（[FLAG-config-011](../../ir/config/FLAGS.md#FLAG-config-011)）も、[F51](#F51)・[F65](#F65) と同じく tracing の target `remote_merge::warning` に出し、--log-level・RUST_LOG によらず標準エラーに出す（段 5 の 2 回目のレビューの rt-r2-2）。tracing の警告をすべて洗い、IR が表示すると決めている利用者に向けた警告で、この target でないものはこれだけだった（ほかは診断の記録で、IR が文言を挙げていない）。
  - why: F51 と同じ。FLAG が実装の振る舞いとして挙げている警告で、記録の細かさで消してよいとはしていない。
  - decided_by: Claude（F51・F65・FLAG-config-011 から導いた。自走の指示により決めた）

## Rejected

- F21 段 5 で行の差分と行の競合を求める関数も IR から書き直す。
  - why: 判定ではなく行の数え方で、[段 4 E27](./2026-10-12-foundation-4.md#E27) が今の示し方を要件にした。書き直すと、合格の基準にしたその示し方が変わったかを測れなくなる（F7）。
- F22 コマンドごとに古い実装を消さず、全部のコマンドを載せ替えてからまとめて消す。
  - why: 載せ替えた後も古い実装が残る間は、新旧の二重の実装になり、どちらが動いているかをテストで見分けにくい（F6）。
