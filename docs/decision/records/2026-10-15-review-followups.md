# レビューの持ち帰り 2 件の裁定（--max-lines の境界・エージェントの読みの時間切れ）

## Context

2026-10-15 のレビュー round 2（CLI と設定・ssh と agent）の指摘のうち、実装の判断を要した 2 件を
利用者に返し、裁定を受けた。

## Agreements

- A1 `--max-lines` で追加と削除の行が値に達したとき、その後に出す行が残っていれば truncated を
  true にし、残っていなければ truncated にしない（何も省いていないため）。IR の REQ-cli-054 の
  字面（「その数が値に達したら」）をこの意味に合わせて書き直す。実装は変えない。
  - why: truncated が立てばテキストは "... (output truncated)" を出す。省いていないのにその印を
    出すのは誤り。走査の上限も「上限ちょうどは打ち切りにしない」で揃っている（sd-r3-1 の修正で
    固定した）。
  - decided_by: 利用者（2026-10-15）
- A2 エージェントと手元をつなぐ読みに時間切れを付ける。値は ssh の接続の時間切れと同じ値に、
  inactivity_timeout と同じ下限（120 秒）を当てた max(timeout_sec, 120) とし、時間切れは
  `接続断`（応答なし）として運ぶ。
  - why: IR の `接続断` の定義は「時間切れになったこと」を含むが、そこへ到達するコードがなく、
    sshd が生きていてエージェントだけが答えない場合にコマンドが返らなかった。大きな走査の間は
    エージェントが黙って計算するので、短い値にすると健全な処理を切ってしまう。
  - decided_by: 利用者（2026-10-15）

## Position

A1・A2 は反映済み。

- A1: `docs/ir/cli/diff-output.md`（REQ-cli-054）の字面を直し、`diff_hunks_tests.rs` に
  「ちょうど上限で残りがなければ truncated にしない」「上限の後に行が残れば truncated にする」を
  足した。
- A2: `SshAgentTransport::start` が読みの時間切れを受け取り `client_read` に設定する。
  `agent_start.rs` が `remote_merge_ssh::silence_timeout(timeout_sec)` を渡す。`AgentClient::receive`
  は時間切れを "the agent did not answer within the timeout" の文言で `接続断`（応答なし）にする。
  `client/tests/failures.rs` に、答えない相手でハングせず接続断になるテストを足した。
  ssh と agent のレビュアーに確かめてもらった（round 3。直っている）。その再確認の指摘 2 件も
  反映した: 挨拶の読みの時間切れの文言（sa-r2-1）、黙っていてよい時間の下限を当てる規則を
  ssh クレートの `silence_timeout` にまとめ、inactivity_timeout と読みの時間切れで同じ値を使う
  こと（sa-r2-2。REQ-ssh-025 に明記し、契約テストを足した）。
