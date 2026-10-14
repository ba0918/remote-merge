# 走査と子の一覧の exclude と symlink の扱いを、三つの経路でそろえる

## Context

品質の台帳の iv-1・iv-2・ao-3 を直す単位である。どれも、同じ設定と同じファイルの状態で、ローカル・SSH・エージェントの経路の一覧が違う件である。

- iv-1: SSH の走査は、"/" を含む exclude に当たったディレクトリの子孫を除かない。承認済みの [REQ-config-022](../../ir/config/filters.md#REQ-config-022) はどの経路も子孫を除くと決めているので、仕様は変えずにコードを直す。
- iv-2: エージェントの走査は、ディレクトリを指す symlink の配下を辿らない。承認済みの [REQ-scan-011](../../ir/scan/directory-links.md#REQ-scan-011) は辿ると決めているので、仕様は変えずにコードを直す。
- ao-3: SSH の子の一覧は "vendor/**" のような exclude を深い階層の "a/vendor" にも当て、ローカルとエージェントは root_dir からのパス全体に当てる（[FLAG-paths-002](../../ir/paths/FLAGS.md)）。利用者は [agent-operations A17](./2026-10-11-agent-operations.md#A17) で、iv-1 と同じ単位で exclude の規則として全経路をそろえると決めている。

利用者は 2026-10-11 に、品質の台帳の単位を順に自走して進めてよいと指示した。ここに書く決定は、どれも A17 と承認済みの要件から導いたもので、新しい方針は足していない。

Position: 決定は A1 で出揃った。

## Agreements

- A1 子の一覧の exclude は、どの経路でも、直下の子の root_dir からの相対パスに、走査と同じ規則（"/" を含まないパターンは [REQ-config-021](../../ir/config/filters.md#REQ-config-021)、含むパターンは [REQ-config-022](../../ir/config/filters.md#REQ-config-022)）で当てる。SSH の経路も、ローカルとエージェントと同じ当て方にする。[REQ-paths-013](../../ir/paths/operations.md#REQ-paths-013) と [REQ-paths-017](../../ir/paths/operations.md#REQ-paths-017) をこの形に書き直し、FLAG-paths-002 を閉じて消す。
  - why: A17 が「exclude の規則として全経路をそろえる」と決め、エージェントの一覧はローカルと同じ当て方にした。走査の規則は REQ-config-021・022 で決まっていて、一覧だけ別の規則にする理由がない。
  - decided_by: 利用者（[agent-operations A17](./2026-10-11-agent-operations.md#A17) から導いた。2026-10-11 の自走の指示により、この単位で書き下ろした）

## Revisions

- 2026-10-11: [FLAG-paths-002](../../ir/paths/FLAGS.md) を A1 で閉じ、IR から消した。
