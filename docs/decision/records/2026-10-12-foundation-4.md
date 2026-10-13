# 土台の段 4: 比べる流れを一つにする

## Context

[土台の作り直し](./2026-10-12-foundation.md) の段 4 である。diff・status・merge・sync・rollback は、二つの側の状態から「同じ」「違う」「読めない」を決める規則を、コマンドごとに別の要件と別の実装で持っている。IR でも、更新時刻での判定が status（[TBL-cli-001](../../ir/cli/status.md#TBL-cli-001)）と merge（[REQ-merge-020](../../ir/merge/comparison.md#REQ-merge-020)）に二度書かれ、読めないときの扱いが status・diff・merge・参照先で四通りに書かれている。[foundation A7](./2026-10-12-foundation.md#A7) のとおり、段 4 では比べる流れ（両側の状態 → 判定 → 必要な分だけ読む → もう一度判定）を一つの純粋な流れにし、その IR を書く。段 2・段 3 で持ち越した二つ（FIFO などの特殊なファイルを通常ファイルとして一覧に載せる件、子の一覧の 10,000 件での黙った打ち切り）もここで決める。各コマンドを新しい流れに載せ替えて古いコードを消すのは段 5 である。

Position: 段 4 の仕様（E1〜E29）を書いた。比べる流れの IR は `docs/ir/compare/` に置き、コマンドごとの要件はその規則を引く形に書き直した。製品の方針に関わるもの（参照先の印・種類の違いの終了コード・競合の意味・参照先との差の出し方・その他の種類の終了コード）は推奨を利用者に確かめて E24〜E29 で決めた。次は [段 4 の計画](../../plans/foundation-4.md) を実装する。

## Agreements

- E1 diff・status・merge・sync・rollback は、一つの比べる流れを通る。流れは四つの段からなる。一つ目は、比べる側（左右と、--ref の参照先）のパスごとの状態（ない・通常ファイル・ディレクトリ・symlink・その他の種類と、大きさ・更新時刻・リンク文字列）を、走査かパスの確認で集める段。二つ目は、集めた状態の組から一つの判定の表で判定する段。三つ目は、判定が求めたものと、コマンドがその判定の扱いに使うものだけを、まとめて読む段。四つ目は、読んだ結果でもう一度判定する段である。二つ目と四つ目は純粋な関数にし、経路は一つ目と三つ目の事実を集めるだけにする。コマンドは判定をそのコマンドの扱い（表示・書く・飛ばす・失敗に出す）に訳すだけにし、判定の規則を持たない。
  - why: [foundation A7](./2026-10-12-foundation.md#A7) の決めた段 4 の形である。規則がコマンドごとにあると、一つを直しても別のコマンドに差が残る（[foundation A3](./2026-10-12-foundation.md#A3)）。
  - decided_by: 利用者（[foundation A7](./2026-10-12-foundation.md#A7)・[foundation A3](./2026-10-12-foundation.md#A3) から導いた。自走の指示により書き下ろした）
- E2 判定と扱いを分ける。判定（同じ・違う・片側だけ・種類の違い・両側にない・判定できない）は、経路・コマンド・パスの指定の仕方（名前で指定したか、ディレクトリの展開か、root_dir 全体か、指定したパスの数）・走査の範囲によらず、同じ状態の組に同じ値を返す。コマンドごとに違ってよいのは、判定をどう扱うか（status の "modified"、merge の書き込みやスキップなど）と、比べ方（E4）だけである。ディレクトリとディレクトリへの symlink の組は、どのコマンドでも種類の違いと判定し、status は "modified" に、diff は種類とリンク文字列の差と子の差に、merge・sync は組そのものを置き換えずに中を比べること（[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001)）に訳す。
  - why: 今の IR は、同じ組を status では変更、merge では種類の違いでないとしており、判定が二通りあるように読めた。違うのは判定ではなく、表示するコマンドと書き込むコマンドの目的である。判定を一つにして扱いの表を分ければ、今の利用者に見える振る舞いを変えずに規則を一か所にできる。
  - decided_by: Claude（E1 と [foundation A3](./2026-10-12-foundation.md#A3) から導いた。自走の指示により決めた）
- E3 パスが「ない」ことは、一つ目の段で集めた状態だけから決める。三つ目の段の読み込みが「ない」の失敗（[REQ-paths-020](../../ir/paths/operations.md#REQ-paths-020)）を返したときは、状態を集めた後にそのパスが変わったものとして判定できない（E7）にし、片側だけとも両側にないとも扱わない。参照先も同じにし、参照先の同じパスがないかは参照先の状態から決める（[REQ-cli-090](../../ir/cli/unreadable.md#REQ-cli-090)）。rollback が戻す先にファイルがないか（[REQ-backup-027](../../ir/backup/rollback-path.md#REQ-backup-027)）も、戻す先の状態から決める。
  - why: [REQ-paths-020](../../ir/paths/operations.md#REQ-paths-020) は失敗した操作の結果をないこととして扱わないと決めている。今の REQ-cli-090 は参照先の読み込みの失敗のうち「ない」を参照先にないこととしており、状態を見ずに読み込みの失敗の種類からないことを推し量っていた。
  - decided_by: Claude（REQ-paths-020 から導いた。自走の指示により決めた）
- E4 二つの通常ファイルの比べ方は、メタデータで比べる・中身で比べるの二つにし、どのコマンドがどちらを使うかを一つの表にする。status は --checksum がなければメタデータ、あれば中身。diff は、名前で指定したファイルもディレクトリの展開も root_dir 全体も中身。merge・sync は、名前で指定したファイルと --checksum のときは中身、ディレクトリの展開と root_dir 全体の下のファイルは --checksum がなければメタデータ。メタデータで比べるときの規則（大きさが違えば違う、大きさが同じで両側がともに秒より細かい精度の更新時刻を持ち共通する最も細かい精度で同じなら読まずに同じ、そのほかは中身で決める）は、一つの表に一度だけ書く。中身で比べるときも、大きさが違えば読まずに違うとする。
  - why: 今の IR は同じ更新時刻の規則を status と merge に二度書いていた。diff は名前で指定したファイルだけ中身で比べると書かれ、ディレクトリの展開と root_dir 全体の比べ方は決まっていなかった。diff は中身の差を示すコマンドなので、更新時刻が同じだけで差なしとすると違う中身を差なしと答える。大きさが違えば中身も違うので、読まずに決めても答えは変わらない。
  - decided_by: Claude（[REQ-cli-080](../../ir/cli/diff-strategy.md#REQ-cli-080)・[REQ-cli-027](../../ir/cli/status.md#REQ-cli-027)・[REQ-merge-020](../../ir/merge/comparison.md#REQ-merge-020) から導いた。自走の指示により決めた）
- E5 二つの通常ファイルの中身が同じかは、バイト列が同じかで決め、流れは両側の SHA-256 のハッシュを比べてそれを決める。中身をメモリに持つ読み込み（diff が差分を示すため、merge・sync・rollback が書くため、バックアップのため）は、コマンドがその中身を使うときだけ行う。[REQ-cli-079](../../ir/cli/size-limit.md#REQ-cli-079) の大きさの上限は、この中身をメモリに持つ読み込みにだけ当て、ハッシュには当てない。ハッシュを取る道具のないリモートで中身を流して手元でハッシュを取るとき（[TBL-ssh-002](../../ir/ssh/compatibility.md#TBL-ssh-002)）も、ハッシュとして扱い上限を当てない。status が上限を当てないこと（[REQ-cli-094](../../ir/cli/status.md#REQ-cli-094)）は、status の例外ではなくこの規則の帰結になる。
  - why: [foundation A7](./2026-10-12-foundation.md#A7) の「必要な分だけ読む」である。SSH で比べるだけのために中身を送ると、比べるファイルの数と大きさの分だけ遅くなる。上限は中身をメモリに持つことを防ぐためのもので（[REQ-cli-095](../../ir/cli/status.md#REQ-cli-095) と同じ理由）、ハッシュは中身を少しずつ読むので当てる理由がない。利用者に見えて変わるのは、上限を超える同じ中身のファイルが diff・merge・sync で読めないファイルにならなくなることだけで、中身が違えば差分か書き込みのために読むので、今までどおり上限を超える失敗になる。
  - decided_by: Claude（[foundation A7](./2026-10-12-foundation.md#A7)・REQ-cli-094・REQ-cli-095 から導いた。自走の指示により決めた）
- E6 symlink は、どのコマンドでもリンク文字列で比べ、リンク文字列が同じなら同じ、違えば違うとする（status の [REQ-cli-028](../../ir/cli/status.md#REQ-cli-028)、merge の [REQ-merge-003](../../ir/merge/symlink.md#REQ-merge-003)、参照先を使う merge の [REQ-cli-051](../../ir/cli/merge.md#REQ-cli-051)）。diff が参照先の中身も示すこと（[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020)）は別の規則ではなく、辿った先の組をもう一つの組として同じ判定の表で判定し、二つの組のどちらかが同じでなければ差分ありとすることとする。参照先を使う merge で三つとも symlink のときは、リンク文字列で REQ-cli-051 の確認をする。[FLAG-cli-027](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: リンクを書くコマンド（merge・sync）が書くのはリンク文字列で、status もリンク文字列で比べている。diff だけが辿った先も見るのは、表示するものが多いからで、判定の規則が違うからではない。三つとも symlink のときだけ比べ方が決まっていなかったのは、規則がコマンドごとにあったためである。
  - decided_by: Claude（REQ-cli-028・REQ-merge-003・REQ-cli-020 から導いた。自走の指示により決めた）
- E7 判定できないパスの扱いを、どのコマンドでも一つの規則にする。判定できないのは、中身かハッシュの読み込みが `接続断` 以外の失敗（ない・ディレクトリでない・権限がない・上限を超える・道具がない・I/O）になったとき、名前が UTF-8 として読めないとき（[REQ-scan-012](../../ir/scan/listing.md#REQ-scan-012)）、どちらかの側がその他の種類（E8）のときである。判定できないパスは、同じとも違うとも片側だけとも扱わない。status・diff は、そのパスを結果の一覧と差分の数に入れず、パスと理由を JSON ではトップレベルの errors に、テキストでは標準エラーに出し、ほかのパスを続けて終了コード 2 を返す。merge・sync は、そのパスを書かず消さずに failed に出し、ほかのパスを続けて終了コード 2 を返す。rollback は、そのパスを書き戻さずに failed に出す。読み込みの失敗の理由は、どのコマンドでも [REQ-merge-019](../../ir/merge/read-failure.md#REQ-merge-019) の形（"read failed: " に、読めなかった側ごとの "left: 原因"・"right: 原因"・"ref: 原因" を "; " でつないだもの）にする。`接続断` は判定できないにせず、[REQ-ssh-003](../../ir/ssh/reconnect.md#REQ-ssh-003)・[REQ-ssh-022](../../ir/ssh/reconnect.md#REQ-ssh-022)・[REQ-ssh-023](../../ir/ssh/reconnect.md#REQ-ssh-023) による。三つの側があるときは、判定できないのはそのパスの組（左右、左と参照先、右と参照先）ごととし、参照先との組だけが判定できないときは左右の判定を残す（[REQ-cli-088](../../ir/cli/unreadable.md#REQ-cli-088)）。
  - why: 今の IR は、読めないときの扱いを status（REQ-cli-088）・diff（[REQ-cli-089](../../ir/cli/unreadable.md#REQ-cli-089)）・merge（[REQ-merge-017](../../ir/merge/read-failure.md#REQ-merge-017)）・UTF-8 でない名前（REQ-scan-012）に別々に書き、理由の形は merge だけが決めていた。理由の形を揃えても、D22 で直したテストが確かめる「理由に失敗の種類の文言が入ること」は保たれる。
  - decided_by: Claude（REQ-cli-088・REQ-cli-089・REQ-merge-017・REQ-merge-019・REQ-scan-012 と [段 3 D1](./2026-10-12-foundation-3.md#D1) から導いた。自走の指示により決めた）
- E8 FIFO・ソケット・デバイスなど、通常ファイル・ディレクトリ・symlink のどれでもないもの（その他の種類）は、どの経路の走査でも通常ファイルとせず、その他の種類として一覧に載せる（件数の上限にはファイルと同じく数える）。比べる流れはその他の種類の中身もハッシュも読まず、どちらかの側がその他の種類のパスは判定できない（理由は "not a regular file"）とし、E7 のとおり扱う。merge・sync は、その他の種類を作らず、書き換えず、--delete でも消さない。中身とハッシュの読み込みは、symlink を辿った先が通常ファイルでなければ、開いて待たずに I/O の失敗（理由は "not a regular file"）にし、状態を集めた後に通常ファイルが FIFO に替わったときも止まらない。
  - why: [段 2 の記録](./2026-10-12-foundation-2.md) の Position で後回しにした件である。三つの経路の集め方はその他の種類を見分けて返しているが、走査の規則（`scan/walk.rs`）がそれを通常ファイルにしていた。そのため FIFO を読みに行くと、書き手が現れるまで止まる。その他の種類は中身で比べられず、書き込み先に作ることもできないので、同じとも違うとも答えられない。黙って一覧から外すと握りつぶしになり、通常ファイルとして読みに行くと止まる。判定できないにすれば、どのコマンドもパスと理由を示し、利用者は exclude で外せる。
  - rejected: その他の種類を、警告を出して飛ばし、終了コードを変えない（E29）。
  - decided_by: Claude（[REQ-paths-020](../../ir/paths/operations.md#REQ-paths-020) と利用者の品質の基準「エラーの握りつぶしは必ず直す」から導いた。自走の指示により決めた）
- E9 merge・sync は、書き込み・symlink の作成と置き換え・削除・--hunks の書き込みのどれでも、その変更の直前に書き込み先のそのパスの状態をもう一度集め、通常ファイルならハッシュも取り、比べたときと種類・リンク文字列・ハッシュのどれかが違うか、比べたときになかったパスが現れていれば、変えずにそのパスを failed に出してほかを続ける（[REQ-merge-011](../../ir/merge/concurrency.md#REQ-merge-011)・[REQ-merge-033](../../ir/merge/concurrency.md#REQ-merge-033)）。そのために、比べる流れは書き換えるか消す予定の書き込み先の通常ファイルのハッシュを、比べるときに取る。--hunks の merge も同じにし、[FLAG-merge-017](../../ir/merge/FLAGS.md) はこれで閉じる。もう一度集めた状態が「ない」の失敗以外で読めないときは、E7 のとおり判定できないとして failed に出す。
  - why: REQ-merge-011 はサイズと更新時刻が同じでも中身が変わっていれば止めると決めているが、PROJECT.md の Constraints のとおり今の確認は更新時刻だけで、--hunks は確認をしていなかった。--delete で消すファイルが比べた後に書き換えられていたときに消すのも、他者の更新を失う点で同じである。もう一度判定することは [foundation A7](./2026-10-12-foundation.md#A7) の流れの四つ目の段そのものなので、変更の種類ごとに書き分けない。
  - decided_by: Claude（REQ-merge-011・REQ-merge-033 と foundation A7 から導いた。自走の指示により決めた）
- E10 merge・sync の --dry-run は、書き込まないことのほかは実行時と同じ流れと同じ判定を通る。--ref を使う merge の --dry-run も [REQ-cli-051](../../ir/cli/merge.md#REQ-cli-051) と [TBL-cli-009](../../ir/cli/merge.md#TBL-cli-009) の確認をし、実行時に書かないファイルは書く予定に出さずに failed に出す。[FLAG-cli-024](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: [REQ-cli-091](../../ir/cli/safety.md#REQ-cli-091) と [REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) は、--dry-run が実行時と同じ確認をし同じ失敗を出すと決めている。参照先の確認だけを飛ばすと、プレビューが実行の結果と食い違う。
  - decided_by: Claude（REQ-cli-091・REQ-paths-011 から導いた。自走の指示により決めた）
- E11 子の一覧は、それだけの件数の上限（10,000）を持たない。子の一覧は比べる流れの走査の一部として、その比較の走査と同じ件数の上限（max_scan_entries。ファイルと symlink とその他の種類を数える）に一度だけ数え（[REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021)）、上限を超えたら不完全な一覧として [REQ-scan-004](../../ir/scan/limits.md#REQ-scan-004)・[REQ-scan-008](../../ir/scan/limits.md#REQ-scan-008) のとおり知らせる。diff は読めた差分を残して errors に出し終了コード 2 を返し（[REQ-cli-084](../../ir/cli/diff-strategy.md#REQ-cli-084)）、merge・sync は [REQ-scan-005](../../ir/scan/limits.md#REQ-scan-005) のとおり書き込みを始めない。[FLAG-cli-049](../../ir/cli/FLAGS.md) はこれで閉じる。[段 3 D29](./2026-10-12-foundation-3.md#D29) の「今までどおり読めた子で続ける」はこれで置き換える。
  - why: 10,000 は設定の `DEFAULT_MAX_DIR_ENTRIES`（crates/remote-merge-config）で、IR にも利用者の設定にも出ない内部の値だった。子の一覧（ローカルの local_io、リモートの remote_change、エージェントの tree_listing がこの値を渡す）は打ち切っても印を返さず、黙って読めた子だけで続けていた。黙った打ち切りは握りつぶしであり、上限を二つ持つと REQ-cli-021 の「一回だけ適用」と食い違う。
  - decided_by: Claude（REQ-cli-021・REQ-scan-004 と利用者の品質の基準から導いた。自走の指示により決めた）
- E12 diff のテキストの出力は、errors に載せるどの失敗（ディレクトリの展開の循環と件数の超過を含む）も、パスと理由を標準エラーに出す。[FLAG-cli-050](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: [REQ-cli-022](../../ir/cli/symlink-diff.md#REQ-cli-022) は比べられなかったパスと理由を示すと決めており、JSON にだけ出してテキストに出さないのは出力の形で握りつぶしになっていた。E7 の判定できないパスと同じ出し方にする。
  - decided_by: Claude（REQ-cli-022・E7 から導いた。自走の指示により決めた）
- E13 diff の種類の違いの note は、両側に項目があり種類が違うときだけ出し、相手側の種類を判定に使った種類で示す。片側だけの symlink は種類の違いではなく片側だけとして示す。[FLAG-cli-048](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: note が判定と別の規則で種類を決めていたため、片側だけの symlink にも種類の違いと出ていた。判定が一つ（E2）なら、note はその判定を写すだけになる。
  - decided_by: Claude（[REQ-cli-020](../../ir/cli/symlink-diff.md#REQ-cli-020) と E2 から導いた。自走の指示により決めた）
- E14 diff の summary の files_with_changes とテキストの "file(s) with changes" の数は、判定が違う・片側だけ・種類の違いになったファイルの数（バイナリと symlink を含む）とする。利用者向けの手引き（skills/remote-merge/SKILL.md）の hunk を持つファイルの数という説明は、この意味に直す。[FLAG-cli-034](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: [REQ-cli-056](../../ir/cli/diff-output.md#REQ-cli-056) は差分があれば終了コード 1 を返し、ハッシュの違うバイナリとリンク文字列の違う symlink も差分ありとしている。数だけを hunk で数えると、終了コード 1 なのに差分のあるファイルが 0 件と出ることがある。
  - decided_by: Claude（REQ-cli-056・E2 から導いた。自走の指示により決めた）
- E15 --ref の参照先の側も、左右と同じ一つ目の段で状態を集め、root_dir の外への解決（[REQ-cli-026](../../ir/cli/symlink-diff.md#REQ-cli-026)）も左右と同じに判定する。--follow-external-links がなければ、参照先の同じパスの途中が root_dir の外を指すときも、その外の中身を "ref_hunks" に出さない。[FLAG-cli-070](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: 側によって状態の集め方と判定が違うと、同じ状態で側ごとに結果が違う。今は参照先だけが root_dir の外の判定を通らず、指定なしでも外の中身を示していた。
  - decided_by: Claude（REQ-cli-026 と E2 から導いた。自走の指示により決めた）
- E16 diff の競合の判定は、バイナリと UTF-8 として読めないファイルも、用語の `競合` のとおりファイル全体を一つの箇所とし、左右がともに参照先から変わり左右の中身が違えば一つの競合として示す（[REQ-cli-016](../../ir/cli/conflicts.md#REQ-cli-016)・[REQ-cli-065](../../ir/cli/conflicts.md#REQ-cli-065)）。[FLAG-cli-059](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: 用語の定義はすでにバイナリを一つの箇所と決めており、diff だけがバイナリの競合を示していなかった。merge は同じ組を REQ-cli-051 で "three-way conflict" として書かないので、diff が示さないとコマンドで競合の判定が分かれる。
  - decided_by: Claude（用語の `競合` と REQ-cli-051 から導いた。自走の指示により決めた）
- E17 比べる流れが扱う IR を、[foundation A10](./2026-10-12-foundation.md#A10) のとおり経路によらない文に書き直す。[REQ-cli-028](../../ir/cli/status.md#REQ-cli-028) の「経路によらず」、[REQ-cli-077](../../ir/cli/directory-paths.md#REQ-cli-077)・[REQ-cli-082](../../ir/cli/diff-strategy.md#REQ-cli-082) の「比べる経路の組」、[REQ-cli-096](../../ir/cli/directory-paths.md#REQ-cli-096) の「どの経路でも」、[REQ-merge-004](../../ir/merge/symlink.md#REQ-merge-004) の「ローカル・SSH・エージェントの全経路で」、入口の検査（[REQ-cli-078](../../ir/cli/diff-output.md#REQ-cli-078)・[REQ-merge-007](../../ir/merge/symlink.md#REQ-merge-007)）の「どの経路でも」を外し、経路によらないことは [REQ-compare-002](../../ir/compare/flow.md#REQ-compare-002) と段 1〜3 の要件に任せる。diff-strategy.md の「20 個以下か 21 個以上か」と、diff-strategy.md・directory-paths.md・binary.md の「走査方式によらず」は今の実装の走査の仕方の都合なので、パスの指定の仕方によらないことに書き直す。重複していた規則の本体（更新時刻での判定、読めないときの扱い）は compare の文書にだけ置き、コマンドの要件はそれを引く。要件の ID は、テストの印を保つために消さない。例（EX）に経路の名前を挙げるものは、経路で同じになることを確かめる例なので残す。新しいディレクトリの権限の [REQ-merge-012](../../ir/merge/permissions.md#REQ-merge-012) は比べる流れではないので、段 5 で扱う。
  - why: [段 3 D12](./2026-10-12-foundation-3.md#D12) が段 4・5 に残したものと、比べる流れの要件に残っていた今の実装の都合である。ID を消すと、合格の基準にするテストの印が宙に浮く（[foundation A6](./2026-10-12-foundation.md#A6)）。
  - decided_by: 利用者（[foundation A10](./2026-10-12-foundation.md#A10)・[段 3 D12](./2026-10-12-foundation-3.md#D12) から導いた。自走の指示により書き下ろした）
- E18 段 4 は、比べる流れ（判定の表・比べ方の表・読み込みの失敗の扱い・変更の直前の判定）を純粋な関数で作り、段 2 の走査と段 3 の経路の上で状態とハッシュと中身を集める組み立てを作る。`remote_merge::cli::*` の `execute_*` の形と出力の型は変えず、各コマンドをこの流れに載せ替えて古い比べる実装を消すのは段 5 にする。段 4 の要件は、流れを直接呼ぶ契約テスト（crates/remote-merge/tests/contract/）で確かめ、CLI の出力での確かめは段 5 で載せ替えたときに足す。
  - why: [foundation A7](./2026-10-12-foundation.md#A7) の段の分け方である。kotowari が印を数えるのは contract/ と cli_diff.rs のテストだけなので、純粋な関数の単体テストだけでは要件のテストが埋まらない。段 3 の `operation_failures_agree.rs` が経路を直接呼んで確かめた前例に合わせる。
  - decided_by: 利用者（[foundation A7](./2026-10-12-foundation.md#A7) から導いた。自走の指示により書き下ろした）
- E19 段 4・段 5 の実装で、今のテストが IR と違う動きを固めていると分かったら、[段 1 B5](./2026-10-12-foundation-1.md#B5) と同じ扱いで IR に合わせて直し、この記録に E 番号で残す。E5（上限を超える同じ中身）・E8（その他の種類）・E11（子の一覧の打ち切り）・E13（note）・E16（バイナリの競合）は利用者に見える出力を変えるので、これらを固めていたテストを直すときは、どの要件と違っていたかを書く。
  - why: [foundation A6](./2026-10-12-foundation.md#A6) のとおり、物差しを書き換えずに作り直しが正しいかを測る。
  - decided_by: 利用者（[foundation A6](./2026-10-12-foundation.md#A6) から導いた。自走の指示により書き下ろした）
- E20 比べる流れの規則は `docs/ir/compare/` の三つの文書（流れ・判定・読み込みと変更の直前の判定）に置き、ID は `REQ-compare-NNN` とする。
  - why: 規則が五つのコマンドに共通するので、どのコマンドの文書にも属さない。コマンドの文書から引ける一か所が要る。
  - decided_by: Claude（E1 から導いた。自走の指示により決めた）

- E24 status の --ref の ref_badge は、左右と参照先の三つとも同じファイルに "all_equal"（テキストは印なし）を、片方だけにあり参照先にもないファイルに "missing_in_ref"（テキストは " [ref-]"）を付ける（[TBL-cli-005](../../ir/cli/status-targets.md#TBL-cli-005)）。status は diff と同じく参照先にだけあるファイルを一覧に載せない（[REQ-cli-068](../../ir/cli/reference.md#REQ-cli-068)）。[FLAG-cli-002](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: 参照先にないことを示す印がすでにあり、片方にあるものが参照先にないことは両方にあるときと同じ関係である。今の "all_equal" は、片方だけにあるファイルを参照先と同じと示していた。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E25 種類の違い（[REQ-merge-001](../../ir/merge/symlink.md#REQ-merge-001)）でスキップしたパスが一件でもある merge は終了コード 2 を返し、sync のその書き込み先の status は "partial"（書けたファイルがなければ "failed"）にする（[REQ-cli-047](../../ir/cli/merge.md#REQ-cli-047)・[TBL-cli-007](../../ir/cli/sync.md#TBL-cli-007)）。--delete なしで書き込み先だけのファイルをスキップすること（[REQ-merge-024](../../ir/merge/deletion.md#REQ-merge-024)）は、今のまま終了コードと status を変えない。[FLAG-cli-010](../../ir/cli/FLAGS.md)・[FLAG-cli-016](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: 利用者が書くよう求めたパスを書けていないので、[REQ-cli-019](../../ir/cli/results.md#REQ-cli-019) の部分的な成功に当たり、旧仕様もそう決めていた。書き込み先だけのファイルを残すのは指定どおりの動きなので、部分的な成功ではない。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E26 diff は、参照先になく左右にあって中身が違うファイルと、左右の片方が参照先から消し他方が参照先から変えたファイルを、どちらもファイル全体で一つの競合として示す（[REQ-cli-016](../../ir/cli/conflicts.md#REQ-cli-016)）。片方が消し他方が参照先と同じままのときは競合にしない。[FLAG-cli-060](../../ir/cli/FLAGS.md)・[FLAG-cli-067](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: merge は前者を [TBL-cli-009](../../ir/cli/merge.md#TBL-cli-009) で書かずに止めており、後者の「消す」と「変える」は行の範囲では比べられない。ない側を空の中身として行で比べる今の形は、片側にないことを全行の削除という別の意味にしていた。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E27 行を足すだけの変更の競合と、重なる変更の競合の数え方は、今の diff の示し方を要件にする（[REQ-cli-065](../../ir/cli/conflicts.md#REQ-cli-065)）。左右が同じ位置に行を足したとき、一方が足した位置が他方の変えた範囲の中にあるとき、他方の変えた範囲の先頭の行の前に足したときを競合とする。一方の一つの変更が他方の複数の変更と重なるときは重なる組を一つの競合とし、接するが重ならない二つの競合は二つと数える。テストは段 5 で入れる。[FLAG-cli-071](../../ir/cli/FLAGS.md)・[FLAG-cli-072](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: 比べる流れの判定ではなく行の差分の数え方の問題で、どちらに決めても流れの形は変わらない。今の示し方を固めれば、段 5 で載せ替えたときに出力が変わったかを測れる。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E28 --ref の diff の参照先との差は次のとおり示す。symlink の項目は、"link_targets" に参照先のリンク文字列 "ref" を加え（参照先が symlink でなければ null）、参照先とのリンク文字列の差を示す。バイナリのファイルは、参照先のハッシュを "ref_hash" に示し、"ref_hunks" を出さない。--max-lines で "ref_hunks" を打ち切ったときも、そのファイルの "truncated" を true にする。バイナリの競合（E16）は、"conflict_regions" に一つの要素を出し、その "ref_range" をファイル全体を表す {"whole_file": true} にして、行の範囲と行の内容の欄を出さない。root_dir の外へ解決される項目を --follow-external-links なしで比べるときと、--max-files の枠を超えて出さないファイルには、今のとおり "ref" を付けない。[FLAG-cli-061](../../ir/cli/FLAGS.md)・[FLAG-cli-063](../../ir/cli/FLAGS.md)・[FLAG-cli-064](../../ir/cli/FLAGS.md) はこれで閉じる。
  - why: E6 で symlink の比べ方をリンク文字列に一つにしたので、参照先との差もリンク文字列で示せる。空の "ref_hunks" が「同じ」と「比べていない」の両方を表すのは、比べていないことの握りつぶしになる。中身を読まない項目と出さないファイルは、参照先と比べる中身がない。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E29 その他の種類（FIFO・ソケット・デバイス）は E8 のとおり判定できないとし、どのコマンドも終了コード 2 にする。exclude で外せる。
  - why: 黙って外すのは握りつぶしで、警告だけで終了コード 0 にすると、自動の処理がそのパスを比べていないことに気づけない。
  - decided_by: Claude（利用者の品質の基準『経路差とエラーの握りつぶしは必ず直す』から導いた。自走の指示により決めた）
- E30 SSH の経路の読み込みのスクリプトの単体テスト（`crates/remote-merge-ssh/src/contents_tests.rs` の `each_path_gets_its_content_or_the_kind_of_its_failure`）が、ディレクトリの中身の読み込みを理由 "Is a directory" の I/O の失敗として固めていたものを、"not a regular file" の I/O の失敗を確かめるように直す。
  - why: [REQ-paths-021](../../ir/paths/operations.md#REQ-paths-021) は、辿った先がディレクトリを含む通常ファイルでないものの中身とハッシュを、理由 "not a regular file" の I/O の失敗で終えると決めている。失敗の種類（I/O）は変わらず、理由の文言だけが違っていた。
  - decided_by: Claude（E19 と REQ-paths-021 から導いた。自走の指示により決めた）
- E31 子の一覧の結果の単体テスト（`crates/remote-merge-engine/src/child_listing_tests.rs` の `a_listed_directory_gives_its_children_and_a_truncated_listing_keeps_what_was_read`）が、件数の上限で打ち切った子の一覧を、印のない読めた子の一覧として返すことを固めていたものを、読めた子を持つ不完全な一覧の誤り（`ChildrenTruncated`）として返すことを確かめるように直す。
  - why: E11 と [REQ-cli-021](../../ir/cli/symlink-diff.md#REQ-cli-021)・[REQ-scan-004](../../ir/scan/limits.md#REQ-scan-004) は、子の一覧が上限を超えたら不完全な一覧として知らせると決めている。今のテストは、FLAG-cli-049 で持ち越していた黙った打ち切りを固めていた。
  - decided_by: Claude（E19 と E11 から導いた。自走の指示により決めた）
- E32 変更の直前にもう一度集めた状態では通常ファイルだったパスのハッシュが「ない」の失敗になったときは、比べた後にそのパスが変わったものとして、変えずに failed に出す（理由は変わったことを示す文）。[REQ-compare-015](../../ir/compare/reading.md#REQ-compare-015) の文をこれに合わせる。
  - why: E9 は、もう一度集めた状態が「ない」の失敗以外で読めないときだけを判定できないとしており、「ない」の失敗は比べた後に消えたこと、つまり変わったことを表す。REQ-compare-015 は E9 のこの区別を書き落とし、どの失敗も判定できないとしていた。変更の直前の判定だけの扱いなので、比べるときの読み込みの失敗からないことを決めない [REQ-compare-003](../../ir/compare/flow.md#REQ-compare-003) とは矛盾しない。
  - decided_by: Claude（E9 と利用者の品質の基準から導いた。自走の指示により決めた）
- E33 状態を集める段でも、名前で指定したパスとディレクトリ（走査の起点）の途中の要素がディレクトリでない側では、その起点のパスはその側にないとし、ほかの起点の状態を集め続ける。[REQ-compare-003](../../ir/compare/flow.md#REQ-compare-003) に一行足す。
  - why: 状態を集める段は、名前で指定した起点の一つの途中がディレクトリでないと流れ全体を止めていたが、変更の直前の判定は同じ事実をそのパスがないとしていた。途中がディレクトリでないことは、そのパスがないことを集めた状態から決められる事実である（E3）。一つの起点のために残りを比べないのは、[REQ-cli-096](../../ir/cli/directory-paths.md#REQ-cli-096) の「ほかを続ける」と食い違う。
  - decided_by: Claude（E3・REQ-cli-096 と利用者の品質の基準から導いた。自走の指示により決めた）
- E34 二つの symlink を辿った先の組（[REQ-compare-008](../../ir/compare/judgement.md#REQ-compare-008)）でも、辿った先のどちらかがその他の種類なら、読む前に判定して、理由を "not a regular file" とした判定できないとする。そのために走査の一覧の symlink は、辿った先が通常ファイルかその他の種類かも持つ。[REQ-compare-009](../../ir/compare/judgement.md#REQ-compare-009) に辿った先を含むことを書く。
  - why: 走査は辿った先を「ディレクトリか・ディレクトリでないか・ないか」しか持たなかったので、辿った先がその他の種類の組はハッシュを取りに行き、"read failed: …" の理由になっていた。E8 は、その他の種類の中身もハッシュも読まないと決めている。
  - decided_by: Claude（E8 と利用者の品質の基準から導いた。自走の指示により決めた）

## Rejected

- E21 比べ方の違いをなくし、どのコマンドもいつも中身で比べる。
  - why: status と merge・sync のディレクトリの比較は、更新時刻で読まずに決めることで大きな木を速く比べる。--checksum という利用者の指定があり、比べ方の選択は製品の機能である。判定の規則を一つにすることと、比べ方の指定をなくすことは別である。
- E22 判定できないパスを、コマンドごとに今の書き方のまま残す。
  - why: 同じ失敗の扱いが四か所に書かれ、理由の形も一つのコマンドだけが決めていた（E7）。
- E23 子の一覧の上限を利用者が設定できる値にする。
  - why: 上限が二つあると、REQ-cli-021 の「比較全体で一回だけ」と食い違う（E11）。
