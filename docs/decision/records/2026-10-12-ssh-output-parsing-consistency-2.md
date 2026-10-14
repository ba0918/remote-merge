# 名前とリンク先を読んだとおりに載せる単位で、辿れない symlink での走査の止め方を経路でそろえたこと

## Context

[2026-10-12-ssh-output-parsing-consistency.md](./2026-10-12-ssh-output-parsing-consistency.md) の A2 は、エージェントの走査が項目の情報を読めないときに止まると決めた。同じ `process_entry`（`crates/remote-merge-agent/src/agent/tree_scan.rs`）で、symlink がディレクトリを指すかを `Path::is_dir` で決めており、リンク先を辿れないときの失敗を「ディレクトリを指さない」として握りつぶしていた。ローカルと SSH の走査は、リンク先がない（ENOENT）ときだけ symlink を載せ、ループや途中がディレクトリでないためにリンク先を辿れないときは止まる。IR の [REQ-scan-006](../../ir/scan/listing.md#REQ-scan-006) の「リンク先がない symlink も載せる」が、辿れない symlink を含むかを決めていなかった。

Position: 決定は A1 で、コードと IR に入れた。

## Agreements

- A1 リンク先がないのではなく、ループしているか途中にディレクトリでないものを含むためにリンク先を辿れない symlink があるとき、どの経路の走査も、それをディレクトリを指さない symlink として載せずにエラーで止まる。REQ-scan-006 の「リンク先がない」はリンク先が存在しない（ENOENT）ことと読み、[REQ-scan-015](../../ir/scan/listing.md#REQ-scan-015) にこの止まり方を足す。エージェントの走査は、リンク先の stat が NotFound のときだけディレクトリを指さない symlink として載せ、ほかの失敗では不完全な走査として止める。
  - why: 「リンク先がない symlink も載せる」は [2026-09-29-adopt-scan-listing.md の A1](./2026-09-29-adopt-scan-listing.md#A1) で実装とテストから追認した文で、その時点のローカルの走査（6c48524、2026-09-26。件名「ローカルのディレクトリリンクを上限付きで走査」、本文「循環や読み取り失敗時は走査を中止する。参照先が存在しない symlink 自体は一覧に残す」）は、walkdir のエラーが NotFound のときだけ symlink を載せ、ほかは止めていた。だから追認した意味は ENOENT に限られる。IR の別の項目も、途中の祖先がディレクトリでない値を「ない」ではなくエラーとして扱う（[REQ-config-032](../../ir/config/include.md#REQ-config-032)）。エージェントの走査はディレクトリへの symlink の配下も辿る（[REQ-scan-011](../../ir/scan/directory-links.md#REQ-scan-011)）ので、辿れない symlink を「ディレクトリを指さない」とすると、ディレクトリを指していたときの配下が欠けた一覧を完全として返しうる。測ったこと（どれも使い捨てで、コミットしていない）: 互いを指す "a"・"b" と "top.txt/x" を指す "n" を置いた root_dir で、エージェントの `scan_tree` は `OK [("a", Some("b"), false), ("b", Some("a"), false), ...]` と `OK [("n", Some("top.txt/x"), false), ...]` を返し、ローカルの `scan_local_tree_recursive_with_include` は `Directory scan incomplete: ... Too many levels of symbolic links (os error 40)` と `... Not a directory (os error 20)` で止まった。`LC_ALL=C /usr/bin/find -L <root> -mindepth 1 -xtype l ...`（GNU findutils 4.9.0）はどちらも診断を出して終了コード 1 で終わった。リンク先がない "d" -> "missing" は、エージェントとローカルのどちらも載せた。契約テスト `req_scan_015_the_agent_scan_stops_on_a_symlink_whose_target_cannot_be_followed` は、修正の前は status が一覧（"a"・"b" が right_only）を返して失敗し、修正の後は成功した。ローカルと SSH の同じテストは修正の前から成功した。途中のディレクトリを検索する権限がなくリンク先を辿れないときも同じ分岐で止まるが、リンク先が読めないディレクトリに当たる場合は [REQ-scan-014](../../ir/scan/limits.md#REQ-scan-014) の単位の扱いでもある。
  - decided_by: 一貫性の確認（consistency phase）
