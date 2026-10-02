# プロトコル抽出の検証手順

## Context

[次の細分化の決定](./2026-10-03-next-crate-extraction.md#Agreements)はプロトコルと既存テストだけの抽出を定める。
版定数の構成だけは、製品版を新crateの版に置き換えないために構造を変更する。
通信処理・公開パス・検査対象を維持し、この限定した変更を前後比較で検証する。

Position: 実装計画と一緒に承認を受ける具体的な検証手順の草稿。実装は未着手。

## Delegated

- D1 比較元は抽出方針をコミットしたmainの7d2331aの完全なIDとする。旧両変更記録は実装開始時に外し、今回の比較だけで実装者と独立レビュー担当が作成し直す。
  - why: 前回のcore抽出の記録を今回の根拠として扱わず、計画コミット以降の変更をブランチ全体で照合する。
  - decided_by: planner（PROJECT.mdのChange conformanceに従う）
- D2 既存14単体テストの本文・名前・印・無視状態は保ち、所属package・library target・test binaryとprotocolモジュールの所在の変化だけを対応付ける。他の3095テストの所属と名前は変えない。
  - why: 新crateのルートへ置く場合にテスト名からagent::protocolの接頭辞がなくなるため、許可した所在変更と検査漏れを区別する必要がある。
  - decided_by: planner（A2・A3の具体化）
- D3 変異検査は既存のformat_handshakeの戻り値を空にする変異だけを抽出前後に実行する。新しい同等変異・テストは追加せず、既存テストで検知されることと、既存スクリプトによるworkspace選択を確認する。
  - why: 移動対象の公開関数を既存handshakeテストが実行するため、変異検査と新crateのテスト収集を小さい範囲で確認できる。
  - decided_by: planner（A2・A4の検証範囲の具体化）
- D4 新crateは既存のagent/protocol.rsをルートsrc/lib.rsへ移して作る。変更は版定義マクロのCLI_VERSION生成を公開product_cli_versionマクロ生成に替える箇所だけとし、製品側agent/protocol.rsは再公開と製品版定数を提供する薄いファイルにする。
  - why: バージョン構成以外の処理と既存14テストの内容を変えず、既存の公開パスを維持する最小構成にする。
  - decided_by: planner（抽出方針D1・D2の具体化）
