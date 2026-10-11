# Flags

## Flags

### FLAG-scan-002: merge と sync で途中のディレクトリ symlink を辿る
- kind: contradiction
- related: REQ-scan-002
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A4

旧総合仕様のシンボリックリンクの扱いの節はリンク先を辿らないとし、旧個別仕様 symlink-merge の 3.7 は途中のディレクトリ symlink の配下をツリーの取得で列挙しないとするが、merge と sync のツリーの取得は status と同じ走査を使うため、ローカル・SSH・エージェントのどの経路でも途中のディレクトリ symlink を辿って配下を列挙する（実装を読んで分かったことで未実行。エージェントの経路は REQ-scan-011 のとおり辿る）。

### FLAG-scan-007: パスを指定した merge と sync の絞り込み
- kind: contradiction
- related: REQ-config-003, REQ-config-004, REQ-config-022
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A9

REQ-config-003 と REQ-config-004 は exclude と include が merge と sync の走査にも効くとし、REQ-config-022 は "/" を含むパターンを root_dir からの相対パスに当てるとするが、ディレクトリのパスを指定した merge と sync は include を当てず、"/" を含む exclude のパターンを指定したディレクトリからの相対パスに当てるため、include の外のファイルやパスのパターンで除外したファイルも書き込みの対象になる。同じ指定の diff はそれらを除く。
ファイルのパスを指定した merge と sync も、TBL-scan-001 のとおり親ディレクトリの下を走査し、"/" を含む exclude のパターンを親ディレクトリからの相対パスに当てるため、パスのパターンで除外したディレクトリの下のファイルも書き込みの対象になる（exclude "a/vendor" で merge a/vendor/x.txt が "a/vendor/x.txt" を書き込むことを SSH の経路で確かめた。ローカルとエージェントの経路は実装を読んで分かったことで未実行）。

### FLAG-scan-009: 名前に改行を含むファイルの SSH の経路の走査
- kind: gap
- related: REQ-scan-006
- source: docs/decision/records/2026-09-29-adopt-scan-listing.md#A12

SSH の経路の走査は find の出力を行ごとに読むため、名前に改行を含むファイルがあると、改行の前までを名前とするファイルを一覧に載せると実装からは読める（未実行）。旧資料に記述がなく、確かめるテストもない。
