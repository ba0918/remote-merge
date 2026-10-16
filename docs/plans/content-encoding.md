# Plan: ファイルごとの文字コードと元バイトを保持する差分と部分マージ

## Goal

文字コードの混在するサーバでも手動取得に戻らず差分を確認でき、表示と競合と部分マージが一つの内容解釈に基づいて動くようにする。

## Specification

IR は "docs/ir/"、承認済み仕様のコミットは "3e77175c"、実装するブランチ名は "content-encoding" とする。

- `docs/ir/config/content.md#REQ-config-046`, `docs/ir/config/content.md#REQ-config-047`, `docs/ir/config/content.md#REQ-config-048`, `docs/ir/config/content.md#REQ-config-049`
- `docs/ir/content/interpretation.md#REQ-content-001`, `docs/ir/content/interpretation.md#REQ-content-002`, `docs/ir/content/interpretation.md#REQ-content-003`, `docs/ir/content/interpretation.md#REQ-content-004`, `docs/ir/content/interpretation.md#REQ-content-005`, `docs/ir/content/interpretation.md#REQ-content-006`, `docs/ir/content/interpretation.md#REQ-content-007`, `docs/ir/content/interpretation.md#REQ-content-008`
- `docs/ir/merge/hunks.md#REQ-merge-009`, `docs/ir/merge/hunks.md#REQ-merge-031`, `docs/ir/merge/hunks.md#REQ-merge-032`, `docs/ir/merge/hunks.md#REQ-merge-045`, `docs/ir/merge/hunks.md#REQ-merge-046`, `docs/ir/merge/hunks.md#REQ-merge-047`
- `docs/ir/cli/binary.md#REQ-cli-061`, `docs/ir/cli/conflicts.md#REQ-cli-016`, `docs/ir/cli/conflicts.md#REQ-cli-099`, `docs/ir/cli/diff-output.md#REQ-cli-053`, `docs/ir/cli/reference.md#REQ-cli-063`, `docs/ir/cli/reference.md#REQ-cli-100`, `docs/ir/config/loading.md#REQ-config-039`
- 判断の根拠と構造の制約は `docs/decision/records/2026-10-16-content-encoding.md#A9` と同記録の Agreements、Prohibitions、Delegated にある。

## Approach and why

共通化は呼び出し側の注意事項ではなく、解釈済みの型を受け取る API として作る。
新しい製品経路は生の内容を直接 UTF-8 化してから diff の関数に渡さない。
次の責務を下位から順に組み立てる。

| 責務 | 配置と受け渡すもの | 採用または自作の理由 |
|---|---|---|
| 文字コード名と解釈の指定 | core の新しい `content/` に、未設定、明示した文字コード、binary を区別する型を置く | 小さな専用の型を作る。設定と CLI が同じ変換を呼び、未設定を明示 UTF-8 に潰さないため |
| デコード | 同じ `content/` に、元バイトと指定から解釈済みの結果を作る一つの関数を置く | UTF-8 は標準ライブラリ、EUC-JP と CP932 は `encoding_rs` を使う。変換表と推定器は自作しない |
| パスの照合 | `core/src/filter.rs` の単一パターン照合を抜き出し、既存の exclude と内容設定から呼ぶ | インストール済みの `glob-match` と既存の照合規則を再利用し、glob の第二実装を作らない |
| 設定の合成と選択 | config に内容設定の型と純粋な解決関数を置く | 既存の AppConfig と設定読み込みを拡張する。core は AppConfig、engine の Side、CLI を参照しない |
| 行の表現 | core の解釈済みテキストに、元バイト範囲と比較用文字列の範囲を持たせる | 表示用の DiffLine だけでは改行と元バイトの位置を保持できないため、小さな専用の表現を作る |
| 二者の変更列 | core の `diff/` に、解釈済みの二者から変更列を作る一つの処理を置く | 既存の `similar`、行の対応付け、文脈による hunk の結合を再利用し、移設した処理を複製しない |
| 表示、競合、部分適用 | 同じ変更列から、それぞれの結果を投影する | 表示用 hunk は文脈 3 行、適用用の変更は文脈 0 行の区別を残し、その対応を同じ変更列から求める |
| 側とパスの解釈 | engine の `treat/content.rs` を共通の入口とする | diff と部分マージの両方が使う。パスの状態、指定、デコード結果、側の理由付けをここに集約する |
| 転送と保存 | 既存の raw comparison と byte I/O をそのまま使う | 新しい転送経路や保存形式は不要で、テキストの解釈を混ぜると更新検知の意味が変わるため |

解釈済みデータと変更列の具体的な型名は実装で決めてよいが、構築は共通の関数に限り、呼び出し側が任意の内容や範囲を詰めて作れないようにする。
解釈済みテキストは元バイトを借用し、デコードした文字列と各行の数値範囲を持つ。
自己参照の構造体、全ファイル分のデコード済みキャッシュ、新しい Arc や trait は導入しない。
一つのパスを扱う間だけ解釈済みデータを持ち、CLI の比較結果に保存された元バイト列は所有権を変えない。

元バイト範囲はデコード後の UTF-8 の長さから逆算しない。
初期対応の三つの文字コードは LF の元バイト位置で行の範囲を対応付けられるため、BOM を除く本文について元バイト側とデコード側の改行を含む行を対応させる。
UTF-8 の BOM を分ける処理を共通の構築関数に置き、`encoding_rs` の BOM による文字コードの自動切り替えや置換付きのデコードは使わない。
初期対応の部分マージは元バイトの範囲を選ぶだけなので、エンコードし直したバイトが一致するかの検査を追加の受理条件にしない。

変更列には Equal の両側の行番号を残す。
同じ文字に読める Equal 行でも、元バイトを採る側は部分マージの書き込み先であり、表示文字列を供給した側とは限らない。
競合の置換内容の比較には改行を含む行を使い、JSON 表示向けに LF を除いた文字列を比較へ戻さない。
左右の表示は left→right、参照先との差の表示は left→ref、競合は ref→left と ref→right とし、向きの違う変更列の生成を同じ処理へ集約する。
逆向きの変更列を単に反転して流用しない。
必要な向きごとに共通の生成処理を呼び、同じ行が繰り返されるときもその向きの対応付けを保つ。

engine の入口は、元の PathComparison、実際の接続先を持つ側の表、内容設定、CLI の上書き指定、必要な参照先の有無を受ける。
接続先は Side::Local または Side::Remote のサーバ名で決め、表示ラベルや実際の I/O 経路から推測しない。
内容設定の解決はこの入口から一つの純粋な関数を通す。
通常の比較や追加で指定したパスの raw 結果を集め終えた後に解釈する方式とし、`diff_compare` や `merge_files` の結果の合流で解釈の表が落ちる第二経路を作らない。
status と全体コピーのために内容を余分に読んだりデコードしたりしない。

生の PathComparison の contents、hashes、pairs、Observed は変更しない。
部分マージの最終結果だけを Vec<u8> として PlannedChange に渡し、更新確認には元の comparison を渡す。
参照先を使わない --force はその側の比較だけを省き、左右の解釈を無視する汎用フラグにはしない。

core の公開済みの `compute_diff`、`detect_conflicts`、`is_binary` の入口と公開型の形は維持し、新しい共通処理へ委譲する UTF-8 用の入口にする。
`apply_selected_hunks_single_pass` の旧引数には元バイト範囲と改行の情報がなく復元できないため、旧文字列 API の再構築だけを互換用の処理として残す。
その場合も採用する行を選ぶ走査は新旧で共有し、製品の新しい部分マージから旧文字列 API は呼ばない。
既存の `apply_hunk_to_text` も互換用の入口であり、元バイトを保持する製品経路には使わない。

## Scope of change

- `crates/remote-merge-core/src/content/`、`src/lib.rs`、`src/filter.rs`、`src/diff/`、core の Cargo.toml とルートの Cargo.lock。
- `crates/remote-merge-config/src/schema.rs`、`merge.rs`、`value.rs`、`lib.rs` と新しい内容設定のモジュール。
- `crates/remote-merge-engine/src/treat/` の共通解釈、diff、競合、部分マージとそのテスト、および `src/service/types.rs` と出力型のテスト。
- `crates/remote-merge/src/main.rs`、`src/config.rs`、`src/cli/` の diff、merge、sync の出力と引数の接続、`src/service/output.rs`。
- `crates/remote-merge/tests/contract.rs`、新しい `tests/contract/content_encoding/` と集約ファイル、関連する既存の contract テスト。
- 上記の型に新しいフィールドを足したためコンパイルできなくなる既存の初期化は、既定値の補完だけを同じ段で直す。AppConfig は config、runtime、CLI、テストの複数箇所にあり、DiffArgs、MergeArgs、DiffOutput の直接初期化も検索して残さない。
- `README.md`、`skills/remote-merge/SKILL.md`、`skills/remote-merge/references/json-schemas.md`、`PROJECT.md` の変更した構造の説明。

## Step order and prerequisites

S1 の型と内容解釈を先に作り、S2 で変更列と元バイトの適用を共有する。
S3 の内容設定は S1 の指定の型を使い、S4 は S2 と S3 の両方を受けて CLI と diff を接続する。
S5 で同じ入口を部分マージへ接続し、S6 で全体コピーの事前表示と手引きをそろえる。
S7 は全体の検証と共通化の確認であり、テストを後付けする段ではない。
実装は一つのブランチ上で行い、同じ core の変更列と型を複数の書き手が並行して別実装しない。
各段で新しい動作のテストを先に失敗させ、PROJECT.md の通常テストで GREEN と REFACTOR を確認する。
変更しない動作の回帰テストが最初から通ることは、新機能の RED の証拠には数えない。

## Verification map

仕様の時点で追加のテスト対応がない 15 要件と 24 例を、下の段で埋める。
既存のマークは要求する動作を実際に確認するテストに残し、単に件数を合わせるために付け替えない。
core と config の単体テストだけでは kotowari の対象に入らないため、利用者のシナリオを通すテストは既存の contract ツリーに置き、要件と例の両方をマークする。

| 段 | 要件 | 新しい例 | 何を実際に確かめるか |
|---|---|---|---|
| S1、S4 | REQ-content-001、REQ-cli-061 | EX-content-001、002、003 | 固定した日本語の元バイトで解釈し、CLI の行差分、未設定時のハッシュ、末尾の不正バイトを確認する |
| S2、S4、S5 | REQ-content-002、REQ-cli-016、REQ-cli-099、REQ-cli-063、REQ-cli-100、REQ-merge-031 | EX-content-004、011、012 | 同じ行と改行の変更、異なる参照先の文字コード、バイナリの参照先の競合を diff と部分マージで確認する |
| S1、S2、S4、S5 | REQ-content-003、REQ-content-005 | EX-content-005、008 | BOM と元バイトだけの差を本文の差と分け、出力と書き込みの有無を確認する |
| S3、S4 | REQ-config-046、REQ-config-039 | EX-config-073、074 | 接続設定を変えず内容設定を読み、別名と未対応の名前を扱う |
| S3 | REQ-config-047 | EX-config-075、076 | グローバルのパスルールとプロジェクトの既定値の組、および後の例外ルールを確認する |
| S3、S4 | REQ-config-048 | EX-config-077 | 一つの接続先の混在ファイルを名前、ディレクトリ、全体走査で同じに解釈する |
| S4 | REQ-config-049 | EX-config-078 | CLI の参照先の上書きが左右に漏れないことを確認する |
| S4、S5 | REQ-content-006 | EX-content-003、009、013 | 左右の失敗と参照先だけの失敗の出力範囲、件数、終了コードを確認する |
| S4 | REQ-content-007、REQ-cli-053 | EX-content-008 | 使用文字コードの側ごとの JSON と、非バイナリの元バイトのハッシュを読む |
| S2、S5 | REQ-merge-009、REQ-merge-032、REQ-merge-045、REQ-merge-046 | EX-merge-069、070、071、073 | diff の番号による選択と、未選択の文字、改行、BOM の元バイトの保持を確認する |
| S5 | REQ-merge-047 | EX-merge-072 | 存在しない側と空ファイルを区別し、新規作成先に元バイトを引き継ぐ |
| S6 | REQ-content-008 | EX-content-010 | merge と sync のヘルプと dry-run の事前表示を確認する |
| S3、S5、S6 | REQ-content-004 | EX-content-006、007 | 解釈の設定が変わっても、更新検知、全体コピー、バックアップ、復元が元バイトで動くことを確認する |

UTF-8、EUC-JP、CP932 について単体テストと代表的な CLI の契約テストを組み合わせる。
期待値を製品のデコード関数で生成せず、自分たちで作った文の固定したバイト列を使う。
同じ文字へデコードされる異なる有効な CP932 のバイト列も、未選択の Equal 行が書き込み先から残ることを確かめる材料にする。
実サーバのファイルや私的な内容はテスト資産に持ち込まない。
実際の Remote の側をローカル I/O へつなぐ既存の RuntimeTargets を使うテストでも、Remote の内容設定が選ばれることを確認する。
ハッシュの一致で省略できる内容を全件読ませるテストや、core のファイル配置を文字列で検査するテストは加えない。

## Left to the implementer

- 解釈済みデータと変更列の型名、数値範囲の具体的な保持方法、同じ責務の中でのファイル分割。
- AppConfig の内容設定と CLI の上書きを持つフィールドの内部名と、一つの小さな上書き用の型にまとめるかどうか。
- 元の PathComparison から必要な参照先を含む view を作る際の借用の配置。ただし元バイトと設定に不要な所有権変換を加えない。
- 新しい dry-run の説明用 JSON 欄の名前。既存の欄の意味と hunk の結果を変えず、テキストと JSON の手引きに記録する。
- 既存の契約を固定していない新しい英語の診断文言。ただし理由の側と文字コード、集計、終了コードは仕様どおりにする。
- 各テストの名前と固定する日本語の例文。内部の関数名、モックの存在、実装ファイルの配置をテストの契約にしない。

## Stop conditions

- 承認済みの内容の受理条件、設定の優先順位、保存するバイト、失敗の扱いを変えないと実装できない場合は、実装で補わず仕様へ戻す。
- `encoding_rs` の選んだ版のマッピングが仕様と合わない、ツールチェーンでビルドできない、または元バイト範囲の対応を承認済みの文字コードで保持できない場合は止める。
- 既存の公開 core API の引数だけでは元バイトを保持できない制約を、製品経路の lossy な再構築で回避しない。互換用の入口の範囲に収まらない公開 API の破壊が必要なら戻す。
- 設定の合流、参照先の有無、出力制限のどれかで、共通の入口を迂回して個別に判断する方が簡単に見えても第二実装を作らない。
- 一般の停止条件である未決の意味や承認内容からの逸脱、不可逆または権限の要る危険な操作、事故の拡大、方式を変えても進まない状態に当たれば止める。

## Out of scope

- 自動文字コード推定、UTF-16、ISO-2022-JP、異なる文字コード間の部分マージ、全体転送時の自動変換。
- ファイル名、symlink のリンク文字列、設定ファイルそのものの文字コード。
- settings の接続型、SSH、Agent、protocol、TargetIo、バックアップ形式と識別子、raw のハッシュや更新確認の意味の変更。
- 新しい永続形式、依存注入の仕組み、プラグイン登録機構、全体キャッシュ、CI やフックの変更。
- 版上げ、タグ、公開、実サーバでの書き込み、既存の無関係な検査結果の修正。

## Steps

### S1: 指定と元バイトから解釈済みの内容を作る

- Purpose: 後続が未設定、バイナリ、テキスト、デコード失敗を取り違えない型と一つの構築処理を作る。
- Specification: `docs/ir/content/interpretation.md#REQ-content-001`, `docs/ir/content/interpretation.md#REQ-content-003`, `docs/ir/content/interpretation.md#REQ-content-006`, `docs/ir/config/content.md#REQ-config-046`
- Prerequisites: 承認済み仕様のコミットを含むこと。既存の Rust ツールチェーンでテストを実行できること。
- May change: `crates/remote-merge-core/src/content/`、core の `src/lib.rs` と Cargo.toml、ルートの Cargo.lock、内容解釈を直接確かめる core と contract のテスト、および contract.rs のモジュール登録。
- Done when: 一つの構築関数が承認済みの判定表を表す型を返し、BOM と改行を含む元バイト範囲を保持し、不正な末尾を置換せずに検出できる。
- Shown by: test 名前の正規化、判定表の全分岐、NUL の境界、多バイト文字の境界、不正な末尾、空ファイル、BOM、LF と CRLF を固定バイトで先に失敗させ、`cargo test -p remote-merge-core --lib` と該当 contract テストを実行する。
- Left to the implementer: 型と関数の名前、私有の数値範囲の保持方法、テストの例文。
- Stop and hand back if: 置換付きデコード、文字コードの自動切り替え、元バイトの再エンコード、未設定と明示指定の同一化が必要になる場合。

### S2: 改行を含む変更列と元バイトの選択を共通化する

- Purpose: 表示、競合、部分適用に別々の行分割と差分の処理を持たせず、同じ変更列から結果を作る。
- Specification: `docs/ir/content/interpretation.md#REQ-content-002`, `docs/ir/content/interpretation.md#REQ-content-003`, `docs/ir/merge/hunks.md#REQ-merge-009`, `docs/ir/merge/hunks.md#REQ-merge-032`, `docs/ir/merge/hunks.md#REQ-merge-046`
- Prerequisites: S1 の型と元バイト範囲が使えること。
- May change: `crates/remote-merge-core/src/diff/`、S1 の content の行の表現、共通の差分と選択を確かめる core と contract のテスト。
- Done when: 表示用と操作用の hunk とその対応が一つの変更列から作られ、競合も改行付きの変更内容で比較され、選択した元バイトと書き込み先の未選択の元バイトを組み合わせられる。既存の公開 core の型の形と入口は互換用の委譲として残る。
- Shown by: test 文脈による結合、両方向の選択、繰り返す行、末尾改行、Equal 行の異なる元バイト、BOM を含む raw 適用を先に失敗させ、既存の core diff と conflict の全テストも実行する。
- Left to the implementer: 変更列の型名と shared な選択走査の関数名。旧 API の文字列の再構築は互換用の処理に閉じ込める。
- Stop and hand back if: DiffLine の表示文字列だけで新しい適用結果を作る、文字コードごとに別の diff を作る、または向きの違う変更列を無条件に反転して流用する必要がある場合。

### S3: 内容設定の合成とパスごとの選択を実装する

- Purpose: 接続設定から分離した内容設定を読み、一つの解決関数で接続先とパスに指定を対応付ける。
- Specification: `docs/ir/config/content.md#REQ-config-046`, `docs/ir/config/content.md#REQ-config-047`, `docs/ir/config/content.md#REQ-config-048`, `docs/ir/config/loading.md#REQ-config-039`
- Prerequisites: S1 の指定の型と名前の変換が使えること。
- May change: `crates/remote-merge-config/src/`、`crates/remote-merge-core/src/filter.rs` とそのテスト、`crates/remote-merge/src/config.rs`、設定の contract テスト、および AppConfig と RawConfig の既存の初期化への既定値の補完だけ。
- Done when: merged なサーバ名に対して内容設定を検証し、設定の二層とルールの順序を一つの解決関数で扱え、既存の exclude と内容ルールが同じ照合処理を使う。
- Shown by: test EX-config-073、074、075、076 を先に失敗させ、接続先名の検証と継承を含む `cargo test -p remote-merge-config --lib`、既存の filter テスト、該当 contract テストを実行する。
- Left to the implementer: 内容設定の内部の型名と resolver の名前。設定のキーと既定の解釈は変更しない。
- Stop and hand back if: 既存の接続セクションの丸ごと置換を内容設定へ流用する、glob の受理や照合の規則を追加する、または settings や SSH の型へ内容設定を置く必要がある場合。

### S4: 共通の側とパスの解釈を diff と CLI へ接続する

- Purpose: CLI は指定を渡すだけにし、engine の一つの入口で解釈と失敗をそろえて diff の表示と集計へ渡す。
- Specification: `docs/ir/config/content.md#REQ-config-048`, `docs/ir/config/content.md#REQ-config-049`, `docs/ir/content/interpretation.md#REQ-content-001`, `docs/ir/content/interpretation.md#REQ-content-002`, `docs/ir/content/interpretation.md#REQ-content-005`, `docs/ir/content/interpretation.md#REQ-content-006`, `docs/ir/content/interpretation.md#REQ-content-007`, `docs/ir/cli/diff-output.md#REQ-cli-053`, `docs/ir/cli/binary.md#REQ-cli-061`, `docs/ir/cli/conflicts.md#REQ-cli-016`, `docs/ir/cli/conflicts.md#REQ-cli-099`, `docs/ir/cli/reference.md#REQ-cli-063`, `docs/ir/cli/reference.md#REQ-cli-100`
- Prerequisites: S2 と S3 が完了していること。
- May change: engine の `src/treat/content.rs`、`diff*.rs`、`uses.rs` とテスト、`src/service/types.rs` と出力型のテスト、製品の `src/main.rs`、`src/cli/diff*.rs`、merge の引数定義、`src/service/output.rs`、関連 contract テスト、および新しい引数や出力のフィールドの既定値補完だけ。
- Done when: diff が解釈済みの型と共有した変更列だけで表示と競合を作り、左右のデコード失敗を件数の処理より前に除外し、参照先だけの失敗では左右を残し、文字として等しくても元バイトの差を落とさない。名前とディレクトリと全体走査で同じ接続先の設定が使われる。
- Shown by: test EX-config-077、078 と EX-content-001、002、003、004、005、008、009、011、012、013 の CLI または同じ engine の契約テストを接続前に失敗させ、max_files と max_lines、参照先だけの失敗、local I/O を使う Remote の側も確認して engine の全テストと該当 contract テストを実行する。
- Left to the implementer: engine の context と view の内部名、値の借用方法、仕様が固定していない英語の note。新しい JSON 欄は旧 JSON の読み込みにも既定値を持たせる。
- Stop and hand back if: 全ファイルの新しい読み込みを追加する、未取得の内容を空へ潰す、表示用ラベルで接続先を選ぶ、または CLI と各 diff のモジュールにデコードや precedence の第二実装を置く必要がある場合。

### S5: 部分マージを共通の解釈と元バイト適用へ接続する

- Purpose: diff と同じ解釈と番号で選んだ変更だけを書き、検証前の副作用と UTF-8 への暗黙の変換をなくす。
- Specification: `docs/ir/merge/hunks.md#REQ-merge-009`, `docs/ir/merge/hunks.md#REQ-merge-031`, `docs/ir/merge/hunks.md#REQ-merge-032`, `docs/ir/merge/hunks.md#REQ-merge-045`, `docs/ir/merge/hunks.md#REQ-merge-046`, `docs/ir/merge/hunks.md#REQ-merge-047`, `docs/ir/content/interpretation.md#REQ-content-003`, `docs/ir/content/interpretation.md#REQ-content-004`, `docs/ir/content/interpretation.md#REQ-content-006`
- Prerequisites: S4 の共通の入口が diff から使われていること。
- May change: engine の `src/treat/content.rs` と `hunk_merge.rs` とそのテスト、製品の `src/cli/merge_hunks.rs` と結果の処理、共通の上書き指定を渡す merge の呼び出し、関連する contract テスト。
- Done when: 部分マージが S4 と同じ入口から解釈と変更列を受け、適用する Vec<u8> を返し、元の comparison を保持したまま PlannedChange に渡す。必要な左右と参照先の検証とバイト列の構築はバックアップの掃除と予約より前に終わる。
- Shown by: test EX-merge-069、070、071、072、073 と EX-content-004、006、009、012 を先に失敗させ、部分マージと diff の番号の一致、--force と --dry-run、既存の merge_recheck_cli、engine の hunk 全テストと該当 contract テストを実行する。
- Left to the implementer: 書き込み用バイト列を持つ結果型の名前と、既存の失敗結果へ理由を渡す私有の補助関数。
- Stop and hand back if: 適用後の本文を as_bytes で UTF-8 にする、未選択箇所を再エンコードする、参照先の失敗を競合なしにする、または元の comparison のハッシュをデコードした内容で置き換える必要がある場合。

### S6: 全体コピーの事前表示と利用手引きをそろえる

- Purpose: 内容の解釈の指定と全体コピーの意味を利用者が取り違えないよう、結果とヘルプと手引きを一致させる。
- Specification: `docs/ir/content/interpretation.md#REQ-content-004`, `docs/ir/content/interpretation.md#REQ-content-008`, `docs/ir/config/content.md#REQ-config-046`, `docs/ir/config/content.md#REQ-config-047`, `docs/ir/config/content.md#REQ-config-049`, `docs/ir/content/interpretation.md#REQ-content-007`
- Prerequisites: S5 が完了し、CLI と JSON の具体的な表記が実装されていること。
- May change: 製品の `src/main.rs`、`src/cli/merge*.rs` と `sync*.rs` の結果の説明、`src/service/output.rs`、engine の出力型とテスト、関連する contract テスト、README.md、PROJECT.md、`skills/remote-merge/SKILL.md` と `references/json-schemas.md`。
- Done when: 全体コピーのヘルプと dry-run が変換しないことを示し、hunk の dry-run を全体コピーと誤表示せず、手引きが混在ルールと CLI と JSON の例を持つ。設定を変えた後の rollback を含め元バイトの転送と保存が保たれる。
- Shown by: test EX-content-010 のヘルプとテキストと JSON の dry-run を先に失敗させ、EX-content-007、既存の whole-file merge と sync と backup と rollback の契約テストも実行する。
- Left to the implementer: 全体コピーの説明用の追加欄の名前と英語の文言、既存の手引きの中での例の配置。
- Stop and hand back if: 新しい設定のひな形が明示的な UTF-8 を勝手に設定する、全体コピーのために文字コード変換を加える、または既存の JSON 欄を消したり意味を変えたりする必要がある場合。

### S7: 対象全体の検証と共通化の確認を完了する

- Purpose: 仕様のカバレッジと回帰の結果を集め、第二の解釈経路が残っていないことを型と呼び出し箇所で確かめる。
- Specification: `docs/ir/config/content.md#REQ-config-048`, `docs/ir/content/interpretation.md#REQ-content-002`, `docs/ir/content/interpretation.md#REQ-content-004`, `docs/decision/records/2026-10-16-content-encoding.md#A9`
- Prerequisites: S1 から S6 の各段がテストファーストで完了していること。
- May change: S1 から S6 の範囲の不足の修正だけ。仕様の意味、CI、フック、Agent の通信と raw I/O には広げない。
- Done when: 対象の要件と全ての新しい例が実際の動作を確かめるマーク済みテストを持ち、変更したファイルと対象 ID に仕様検査の error がなく、通常テストと fmt と Clippy が通り、各 CLI と engine に個別の解釈が残っていないことが確認できる。
- Shown by: check `cargo fmt --all --check`、PROJECT.md の `cargo nextest run`（未導入なら `cargo test`）、`cargo clippy --all-targets --all-features -- -D warnings`、対象の `kotowari query` と `kotowari check --format json` を順に実行し、`is_binary`、`from_utf8`、`TextDiff`、行分割の呼び出しを対象範囲で検索して共通の入口または説明した互換用の入口以外がないことをコードで確認する。
- Left to the implementer: 検証結果のまとめ方。構造を確かめるためだけのテストや grep の固定個数のテストは作らない。
- Stop and hand back if: カバレッジを埋めるため動作を確認しないマークを付ける必要がある、raw の境界の変更に広がる、または対象外の既存の問題を直さないと進まない場合。
