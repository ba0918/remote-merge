# Agent配置処理の内部crateへの抽出

## Context

Agentの実行処理は既に内部crateに所属している。
今回は配置処理を同じownerへ移すが、製品版を生成する責任とその検査は製品に残す。
機械的移動として扱い、正式な計画や新しいIRは作らない。

Position: 利用者の自律的な移行の委任を受け、呼び出し側が以下の設計と閉じた範囲を確定した。実装者の照合後、独立した担当がレビューと記録を行う。

## Agreements

- A1 振る舞い・公開パス・既存依存版とfeature・製品版0.2.13と通信版v4を保つ移行の設計、継続的レビューとローカル統合・後片付けは確認なしに進める委任を受けた。今回の実装者は確定した配置処理の抽出だけを行い、新IR・要件・policy testを作らない。独立検査と統合は呼び出し側が行う。
  - why: 所属変更を版の一致判定・コマンド・解決順序・入力やエラーの意味の変更と混ぜず、実装と独立レビューの責任を分ける。
  - decided_by: user

## Delegated

- D1 六つの既存配置モジュールをAgentのnested agent/deployへ移し、private resolve/sudo/transfer/verifyとpublic remote_targetを保つ。DeployConfigとそのDefault、VersionCheck・DeployResult・DeployCommands・ResolutionSource・ResolvedBinaryと既存定数は同じ下位型を製品から明示的に再公開する。ResolveContextとresolve_agent_binary_withはprivateモジュールに留め、rootへ公開しない。
  - why: 既存の閉じた実装をコードベース探索の段階で採用し、Rust再公開で同じ型と公開パスを維持できる。新しいwrapper型やprivate helperの公開は不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計と再利用判断）
- D2 下位のexpected_version_line・parse_version_output・parse_uname_and_version・build_post_write_scriptは最後の必須引数cli_versionを受け取る。製品の旧signatureを保つ四wrapperは製品CLI_VERSIONを渡すだけとし、uname parserのroot/remote_target両旧パスも保つ。transferの40・verifyの27・remote_targetの21テストモジュールは本文を一切変えず製品に残す。test-only compute_file_sha256の元のdoc/cfg/bodyも製品にだけ残す。
  - why: 既存テストのCARGO_PKG_VERSIONを生成する製品packageを0.2.13のまま保ち、内部版0.1.0を製品版と誤認しない。隠れた既定版や新しい版macro・manifest読み込みは不要である。
  - decided_by: caller（版のproducerを保つ設計）
- D3 current_targetのenv!(TARGET)本文を保ち、Agent build.rsは既存製品scriptを全bytes同一で採用する。製品scriptも変更しない。既存SSH path依存をdevから通常へ移してprivate別名を通常有効にし、既存dirs 6を採用する。resolveの五つのログtargetはremote_merge::agent::deploy::resolveを明記して保つ。
  - why: Cargo TARGETをそのまま渡す同じ生成方法と既存shell escapeを再利用し、新しいplatform対応や依存版更新を混ぜない。file/line/module_pathはowner変更で変わり得るがtarget変更として扱わない。
  - decided_by: caller（build metadataと依存の確定設計）
- D4 resolveの22・sudoの10・rootの2テストだけを同じ名前で移し、他3075件の所在・名前・無視状態を保つ。既存122配置テストの各本文が正しいownerに一度だけ残ることを照合する。比較元は9e6fded5efffab0d5fcb3cc5145cfcabe44adf25に固定し、旧両記録を除いて現在の比較を実装者と独立担当が記録する。既存順序付き検査・API/本文比較と配布前後比較を行い、必須の変異検査は設けない。
  - why: 前回の移動を今回の根拠とせず、34件の所属変更と88件の製品版検査の維持を区別して確かめる。未取得の動的ログや未実行環境を検証済みと扱わない。
  - decided_by: caller（PROJECT.mdの照合と検証方法）
- D5 shared engineなど次の抽出は今回行わず、root/app manifest・製品protocol/build/main/runtime・Agent実行処理・SSH/config/settings/core・他のテストとIR/README/scripts/hooks/CIは変更しない。
  - why: 六つの配置モジュールを移す範囲を超える責務や安全性の判断を混ぜない。
  - decided_by: caller（閉じた実装範囲）
