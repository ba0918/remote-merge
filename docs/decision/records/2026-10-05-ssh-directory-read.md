# SSH の側でディレクトリが空のファイルとして読める不具合を直す判断

## Context

SSH の一件ずつの読み込みは `openssl base64 -in` を使うが、openssl はディレクトリを読むと "Is a directory" を出しながら終了コード 0 で空を返す。
このため SSH の側だけディレクトリやディレクトリへの symlink が空のファイルとして読め、diff の結果がローカルやエージェントの側と食い違っていた。
読み込みを直すと、diff が中身の空なディレクトリと include の外のディレクトリをどう示すかが読み込みの成否に頼れなくなる。
そこで diff がディレクトリを子の一覧の取得で判定する形に変え、その振る舞いと、この不具合を原因とする問題記録の扱いを決める。

## Agreements

- A1 include の外の既存ディレクトリを指定した diff は、配下を include で絞った結果に比べるファイルがないものとして、差分なし（"0 file(s) with changes"）・終了コード 0 にする。ローカル・SSH・エージェントのどの経路の組でも同じにする。[REQ-cli-077](../../ir/cli/directory-paths.md#REQ-cli-077) として書く。
  - why: 利用者の決定の理由は記録されていない。AI が確かめたところ、この答えは include を比べる対象を一致するパスに限る設定とする [REQ-config-004](../../ir/config/filters.md#REQ-config-004) と、中身の空なディレクトリを差分なしとする [REQ-cli-025](../../ir/cli/directory-paths.md#REQ-cli-025) の既存のテストと矛盾しない。
  - decided_by: 利用者（2026-10-05）
