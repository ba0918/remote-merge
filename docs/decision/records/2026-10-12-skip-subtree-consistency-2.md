# --dry-run で書き込み先の祖先がディレクトリでないパスを失敗として出すことを IR に書く

## Context

[2026-10-12-skip-subtree-consistency.md](./2026-10-12-skip-subtree-consistency.md) の A1 の後、書き込み先の祖先が通常ファイルのパス（例 "x/a.txt"）について、merge と sync の --dry-run が "would merge" と終了コード 0 を返し、実行すると "cannot resolve path: not a directory" で失敗して終了コード 2 になる食い違いが直された。
直した後の --dry-run は、実行時が書く前に行うのと同じ書き込み先のパスの確認を読み取りだけで行い、失敗するパスを failed に出す。
[REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) は「パスの確認の結果」がディレクトリでないときの扱いを決めているが、--dry-run がその確認をするかどうかは IR のどこにも書かれていなかった。
ここでは、この振る舞いを IR が支えるかと、IR にどう書くかを決める。

Position: A1 で出揃った。

## Agreements

- A1 merge と sync の --dry-run は、書き込む予定のパスについて実行時と同じパスの確認をし、途中の祖先がディレクトリでないパスを書き込む予定に出さず、"cannot resolve path: not a directory" の失敗として failed に出す（終了コードは [REQ-cli-091](../../ir/cli/safety.md#REQ-cli-091) により 2）。[REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) にこの一文を足し、例 [EX-paths-038](../../ir/paths/routes.md#EX-paths-038) を足す。コードは変えない。確認するパスを、比べたときの書き込み先のツリーになく祖先がツリーで分かるディレクトリだけではないものに絞ることは、利用者から見える違いを生まない実装の選択なので IR に書かない。
  - why: [REQ-paths-011](../../ir/paths/routes.md#REQ-paths-011) は merge と sync について --dry-run を除いておらず、[REQ-cli-004](../../ir/cli/safety.md#REQ-cli-004) は dry-run に変更予定の報告を求め、[quality-cli A7](./2026-10-05-quality-cli.md#A7) は dry-run の結果の正直さを実行時と同じにすると決めている。実行すると失敗するパスを "would merge" と報告するのはこれらに反する向きで、確認をして failed に出すのはこれらと矛盾しない具体化である。[FLAG-cli-024](../../ir/cli/FLAGS.md#FLAG-cli-024) は「dry-run が実行時の確認をしない」ことを IR の欠けとして扱っており、同じ形の欠けがここにもあった。測った事実: 直す前（8442d6a）の契約テスト `merge_dry_run_paths` は --dry-run で merge が `"failed":[],"merged":[{"path":"x/a.txt","status":"would merge"}]`、sync が同じ形で落ちた（実装の報告の RED）。直した後（40fa7a3）は `systemd-run --user --scope -q -p MemoryMax=8G -- cargo nextest run --all-features -E 'test(merge_dry_run_paths)'` で 2 件とも通り、--dry-run の有無によらず failed が "x/a.txt" 一件で理由が "cannot resolve path: not a directory"、merged は空、終了コード 2、書き込み先の "x" は変わらない。確認は実行時と同じ `CoreRuntime::inspect_path`（crates/remote-merge/src/runtime/side_io.rs の inspect_path、merge_flow.rs の target_path_exists が使うもの）を通るので、経路の振り分けも実行時と同じである。
  - decided_by: 一貫性の段（consistency phase）
