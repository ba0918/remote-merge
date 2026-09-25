# 再現可能なテスト環境

## Context

通常の全体テストでは、外部環境を必要とするテストがスキップされている。
利用者の localhost SSH と秘密鍵に依存するテストと、専用のコンテナを要するテストが混在している。
既存の testenv がどの振る舞いを検証し、どう起動されるかを確認したうえで、常時実行できるテストと外部 OS が必要なテストの境界を決める。
この記録はテスト環境の選択と、通常の検証コマンドが保証する範囲を定めるためのもの。
調査の結果、スキップ中の 100 件は利用者の localhost:22 と SSH 鍵が必要で、別の 1 件はその設定を生成するが接続しない。残り 7 件は二系統のコンテナ環境を前提にしている。
testenv/setup.sh は旧サーバーの負荷試験データを生成する手動手順であり、これら 108 件を起動するテストランナーではない。

## Agreements

- A1 通常の SSH・CLI・TUI テストは Docker を使わずに実行できる構成を基本とし、OS 固有の検証は別の隔離環境で実行する。
  - why: 普段のテスト結果を Docker の有無に左右されないものにしつつ、ホスト OS では再現できない条件を検証するため。
  - decided_by: user (took the recommendation)
- A2 現在スキップされている 108 件を検証対象として見直し、重複や実際の振る舞いを見ない検証を整理して、必要なケースを再現可能な実行経路へ移す。
  - why: 環境だけ整えても、誤検証や重複が残ればテスト結果の意味は改善しないため。
  - decided_by: user (took the recommendation)
- A3 通常の検証は必要なテストを毎回実行し、OS 固有の検証は専用コマンドと CI ジョブで必須実行する。両者の結果を区別して報告する。
  - why: 開発環境で Docker が使えない場合も通常系を検証でき、CI では sudo や OS 固有の未検証を成功として扱わないため。
  - decided_by: user (took the recommendation)
- A4 旧サーバー互換性の保証対象は旧 SSH アルゴリズムとサーバーごとの設定であり、CentOS 5／OpenSSH 4.3 固有の動作は保証対象に含めない。
  - why: 現行仕様が指定する接続方式を直接検証でき、入手困難な旧 OS イメージやホストのカーネル設定を通常の保証に持ち込まずに済むため。
  - decided_by: user (took the recommendation)
- A5 通常の SSH テストにはテストごとに動的ポートを持つ既存の隔離された Rust 製テストサーバーを活用し、実 OpenSSH との相互運用は別ジョブで代表的な経路を検証する。
  - why: 固定ポートや個人鍵を必要としない一方、テストサーバーが sshd 固有の挙動を再現しない限界も検出するため。
  - decided_by: user (took the recommendation)
- A6 標準テストの再現性は Linux CI を必須の基準とし、Unix 系の開発環境でも実行できるようにする。
  - why: 対話型端末の既存テストが Unix 専用で、Windows ネイティブを同じ範囲に含めるには別の実行方式が必要なため。
  - decided_by: user (took the recommendation)
- A7 実 OpenSSH、sudo、所有者や権限の振る舞いは、イメージのダイジェストを固定した現行 OpenSSH の専用コンテナを必須 CI ジョブで起動し、一時鍵・動的ポート・一時データで検証して破棄する。
  - why: 個人の SSH 設定に依存せず OS の権限を伴う接続を再現し、実行後の状態を残さないため。
  - decided_by: user (took the recommendation)
- A8 既存の testenv は手動の旧 SSH サーバー負荷環境として残し、bench/legacy-ssh/ へ移し、自動テスト用 fixture は tests 以下で別管理する。
  - why: 現状の環境は10万ファイルの負荷データと CentOS 5 を扱う手順であり、通常テストの起動条件とは一致しないため。
  - decided_by: user (took the recommendation)
- A9 通常系の必要テストは ignore を外して標準のテスト実行に含め、コンテナを要するものは別のテストターゲットと専用コマンド・必須 CI ジョブに分ける。
  - why: 利用できない環境を暗黙にスキップして成功扱いにせず、標準実行の保証範囲を明確にするため。
  - decided_by: user (took the recommendation)
- A10 既存の 108 件は振る舞いごとに実害のある回帰を検知するかで選別し、重複・実処理を見ないテストを件数維持のために残さない。
  - why: 環境を整えても検知力のないテストを移すだけでは再現性のある品質保証にならないため。
  - decided_by: user (took the recommendation)
- A11 テスト環境と CI の合格条件は製品の SSH 仕様から分け、docs/ir/testing/ の独立した運用仕様として記録する。
  - why: 製品の接続契約と、検証基盤の実行条件を混同しないため。
  - decided_by: user (took the recommendation)
- A12 手動の旧 SSH 負荷環境は改名の際に接続情報・ポート・生成物を隔離し、利用者の SSH 設定を書き換えないようにする。
  - why: 手動の負荷試験にも利用者の個人鍵や既知ホスト設定への副作用を残さないため。
  - decided_by: user (took the recommendation)
- A13 cargo test --all-features と cargo nextest run --all-features のどちらも、Docker・個人鍵・localhost:22 なしに通常系の必要ケースを実行する。CI は少なくとも一方を隔離した HOME で実行する。
  - why: 開発者の標準コマンドと現行 CI のいずれでも、環境に依存してテストが無効化されないようにするため。
  - decided_by: user (took the recommendation)
- A14 必須コンテナジョブで Docker・イメージ・sshd を準備できない場合はジョブを失敗させ、成功扱いでスキップしない。
  - why: 検証不能をテストの成功と混同しないため。
  - decided_by: user (took the recommendation)
- A15 通常系の SSH fixture はテスト固有の一時ディレクトリに対する実ファイル操作と CLI・TUI の表示を検証し、コマンド文字列や模擬応答のみで成功を判定しない。fixture が過剰なシェル再実装になるケースは実 OpenSSH テストへ回す。
  - why: 模擬応答を検査するテストでは比較・マージの実害ある失敗を検知できないため。
  - decided_by: user (took the recommendation)
- A16 手動の旧 SSH 負荷環境は一試行ごとに生成し、専用の終了手順で鍵・設定・データ・コンテナを破棄する。
  - why: 永続状態による試行間の差異と、個人の SSH 設定に残る副作用を避けるため。
  - decided_by: user (took the recommendation)
- A17 コンテナ検証と手動の旧 SSH 負荷環境は起動または実行の途中で失敗しても一時資源を片付け、失敗の終了状態を保つ。
  - why: 前回の鍵・設定・コンテナ・データが次回の検証に混入しないようにするため。
  - decided_by: user (took the recommendation)
- A18 コンテナ検証ジョブは pull request と main 向け push の双方で毎回起動し、その失敗を CI 全体の失敗にする。
  - why: 通常系だけの成功で実 OpenSSH と sudo の検証失敗を隠さないため。
  - decided_by: user (took the recommendation)
- A19 コンテナ専用の Rust テストは主パッケージの標準 Cargo 実行から独立したテスト用パッケージに置き、専用コマンドで実行する。
  - why: --all-features を付けても通常テストにコンテナ専用ケースを混ぜず、Rust の実操作アサーションを維持するため。
  - decided_by: user (took the recommendation)
- A20 実 OpenSSH 検証では非対話 sudo が使える場合の更新と、使えない場合に非ゼロ終了し通常権限への切り替えや書き込みを行わないことの両方を確認する。
  - why: OS の権限昇格が失敗する実条件で、安全な停止を確認するため。
  - decided_by: user (took the recommendation)

## Delegated

- D1 スキップされている 108 件を、一件ずつ現実の回帰検知と重複の有無で判断し、維持・統合・削除と通常／コンテナの実行先の対応表を作る。未定義の製品挙動は判断せず利用者に戻す。
  - why: 採否の判断基準は合意済みだが、各テストの結果と既存の検証との重なりは実装・実行時に確認する必要があるため。

## Reuse decisions

- SSH fixture: adopt (codebase) — tests/contract/ssh_server.rs の動的ポートと隔離サーバーを通常系へ再利用し、必要なファイル操作のみ拡張する。
- CLI・TUI 起動: adopt (codebase/dependency) — tests/common/mod.rs の実バイナリ起動と既存 expectrl の PTY 操作を個人 SSH 設定から切り離して使う。
- OS 固有の接続: adopt (platform/image) — Docker と現行 OpenSSH のバージョン固定イメージを使い、所有者・sudo の違いを Rust 製サーバーで再現しない。採用によってテスト対象の OpenSSH 版と Linux ユーザーランドが固定される。
- コンテナ実行・CI: adopt (codebase/platform) — testenv/sudo_e2e.sh の隔離・後片付けと既存 GitHub Actions を起点にし、実行失敗を CI 失敗として伝える。
- 手動の旧 SSH 負荷環境: adopt (codebase) — testenv の生成器と設定を負荷用途として再利用し、名前・接続状態・終了手順を独立させる。
- 108 件の移行台帳: build (few lines) — テスト名と採否・代替検証を一覧で記録し、件数だけでは判断できない失われる検知力を示す。
- コンテナ用の Rust テスト配置: build (minimal) — 独立した小さなパッケージに移し、標準 Cargo 実行の自動収集から外す。
