# Flags

## Flags

### FLAG-scan-001: エージェントの経路のディレクトリ symlink
- kind: contradiction
- related: REQ-scan-002
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A3

REQ-scan-002 は CLI status がディレクトリを指す symlink の配下も再帰列挙するとし、ローカルと SSH の走査はそのとおり配下を列挙するが、エージェントの走査はディレクトリを指す symlink を辿らず、リンク自体だけを一覧に載せる（走査関数の呼び出しで確認。エージェントを有効にした status は未実行）。

### FLAG-scan-002: merge と sync で途中のディレクトリ symlink を辿る
- kind: contradiction
- related: REQ-scan-002
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A4

旧総合仕様のシンボリックリンクの扱いの節はリンク先を辿らないとし、旧個別仕様 symlink-merge の 3.7 は途中のディレクトリ symlink の配下をツリーの取得で列挙しないとするが、merge と sync のツリーの取得は status と同じ走査を使うため、ローカルと SSH の経路では途中のディレクトリ symlink を辿って配下を列挙する（実装を読んで分かったことで未実行）。

### FLAG-scan-003: パスのパターンに当たったディレクトリの配下
- kind: ambiguity
- related: REQ-config-021, REQ-config-022
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A5

"/" を含む exclude のパターンがディレクトリに当たったとき、ローカルとエージェントの走査は配下も一覧から除くが、SSH の走査は配下を一覧に残す（"a/b" と "a/*" で確認）。名前のパターンは REQ-config-021 がディレクトリに当たれば配下も除くとするが、REQ-config-022 はパス全体に当たるときそのパスを除くとだけ述べ、配下を除くかが読み分けられない。

### FLAG-scan-004: include に書いたファイル
- kind: contradiction
- related: REQ-config-023
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A6

REQ-config-023 は include に書いた相対パスそのものを対象にするとするが、include にファイルのパスを書くと、どの経路の走査もそのファイルを一覧に載せない。

### FLAG-scan-005: 存在しない include
- kind: gap
- related: REQ-config-004
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A7

存在しないパスを include に書いたとき、ローカルとエージェントの走査は "Include path does not exist, skipping: 値" で始まる警告を出してそのパスを飛ばし、残りを走査するが、SSH の走査は "SSH command execution failed:" に find のコマンドが続くエラーで走査全体が失敗し、status は終了コード 2 を返す。旧資料に記述がなく、SSH の失敗を確かめるテストはない。

### FLAG-scan-006: include に書いたディレクトリ symlink
- kind: gap
- related: REQ-config-023
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A8

include にディレクトリを指す symlink を書いたとき、ローカルとエージェントの走査はリンク先の実パスを走査し、root_dir の中なら実パスの名前で配下を載せ、root_dir の外なら警告を出して飛ばす（ローカルは "Include path escapes root directory: 値 -> 実パス"）。SSH の走査はリンクの名前の下に配下を載せ、リンク先が root_dir の外でも辿る。旧資料に記述がない。

### FLAG-scan-007: ディレクトリを指定した merge と sync の絞り込み
- kind: contradiction
- related: REQ-config-003, REQ-config-004, REQ-config-022
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A9

REQ-config-003 と REQ-config-004 は exclude と include が merge と sync の走査にも効くとし、REQ-config-022 は "/" を含むパターンを root_dir からの相対パスに当てるとするが、ディレクトリのパスを指定した merge と sync は include を当てず、"/" を含む exclude のパターンを指定したディレクトリからの相対パスに当てるため、include の外のファイルやパスのパターンで除外したファイルも書き込みの対象になる。同じ指定の diff はそれらを除く。
