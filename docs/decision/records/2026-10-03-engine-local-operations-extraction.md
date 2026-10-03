# ローカル操作の抽出

## Context

engineのlocal_ioは既存helperとパス検査を持つ。
今回の確定設計は、製品LocalTargetIoに残る十五操作だけを具体的な自由関数へ移す。
製品のtrait・構築・接続no-op・remote側とテスト所有先は変えない。
実装と独立レビューは別担当の記録として照合する。

## Agreements

- A1 利用者は振る舞い・公開API・テスト期待値・依存feature/版・製品0.2.13と通信v4を維持する移行の設計、継続的実装・独立レビュー・ローカル統合と後片付けを確認なしで進めるよう委任した。今回の実装者は閉じた範囲を実装し、独立レビューと統合は呼び出し側へ引き渡す。
  - why: 所属変更に機能・bug修正・新IR・恒久テストを混ぜず、実装と独立検査を分ける。
  - decided_by: user

## Delegated

- D1 local_ioに十五操作をpublic自由関数として追加し、先頭にrootを渡す。I/O層は既存LocalTargetIo本文、検査層は既存validated_path、走査とfilter層は既存local/core、dispatch層は既存製品traitとadapterを採用する。新しいbackend trait・struct・registryは作らない。
  - why: 全層に既存実装があり、具体的な委譲だけで移行できる。依存・metadata・manifest・lock・root module・mutant mappingの変更は不要である。
  - decided_by: caller（A1の設計委任に基づく確定設計）
- D2 製品十五methodのsignatureとruntimeを無視する扱いは維持し、本文は同じ入力/fieldを渡す下位呼び出しだけにする。private validated_pathは同じ本文で下位に移す。read/write context、strict batch、byte batchのforce=false、statの全path先行検査、canonical操作、recursiveのincludeとsubpathのexclude-only、childrenのfilter、hashesの非検査をそのまま保つ。親担当が実際のClippy診断を確認して許可した追加範囲は、製品executorの未使用private validator再公開と隣接空行の削除、SideIOの五helper再公開をtest-onlyにするimport変更だけとする。八公開exportと三通常helper再公開、元の二test-only再公開、全テスト本文は維持する。
  - why: runtimeから操作実装を分離するだけで、エラー・順序・filter・既定値・入力処理を変えない。絶対入力について一律の拒否や強化された安全性を主張せず、元の結果/errorとの同一性を照合する。
  - decided_by: caller（確定したsignatureと意味の維持）
- D3 下位本文は元の四space indentを除き、root/exclude/includeのfield bindingとvalidated_path呼び出しだけを置き換える。root.joinの式全体への借用は維持する。fmtは新しいsignature・fieldを外した式・delegate/importに必要な個別hunkだけとし保存して逆変換する。全3109 identity、116SideIO/51CoreRuntimeの本文・consumer・mark・fixture・旧local_io宣言とRemoteTargetIoをbyte単位で維持する。
  - why: 機械変換を全製品ファイルと十五本文に戻して照合し、既存テストと一時的な元producerを使うことで新しいpolicy oracleや恒久テストを導入しない。
  - decided_by: caller（確定したsource比較と検証方法）
- D4 比較元をe759c6d610f0ecd4fb473241e61f38f3579ec5e6に固定し、既存の順序付き検査・配布比較・producer parityを実行する。実装者はimplementation.yamlのみを書き、別担当がreview.yamlと独立検査を行う。変異実行は行わない。
  - why: 元producerの再現を全旧CLI実行や独立受け入れと混同せず、同じ変更bytesをbranch全体の記録に結び付ける。親担当は同一draftで全3109件・retryなしの実測passを、三回のquiet admissionと一時的なCPUWeight=100/Nice=0/MemoryMax=40%/Swap=0の条件付き最終検査として採用した。これは原因判定・timing不変・flakiness修正ではなく、以前の失敗は残す。commit hookは変更しない元runnerのidle/Nice=19で実行する。
  - decided_by: caller（確定した検査・記録・役割境界）
  - superseded_by: [D6の新しい照合基準](#D6)
- D6 親担当が別途承認し独立検査済みのTUI test-method修正をmainから通常mergeで取り込み、今回の再照合元をb492bcf52f94c4d375a3eb46437d83a31e920dfdとする。元の抽出比較元と失敗履歴は保持し、四抽出sourceは元の承認済みHEADと同じbytes、五harness sourceはmainと同じbytesに保つ。旧記録だけを退役して全3111 identityで新しい実装記録と独立レビューを結び直す。
  - why: 別途承認されたテスト観測方法の修正を抽出の変更範囲に混ぜず、旧3109 identityと追加済み二utilityテストの合成を実測する。以前の失敗の原因・timing不変や無制限出力の保証は新たに主張しない。
  - decided_by: caller（独立検査済みmainとの合流と固定baseの再照合を許可）
- D5 hunk・handler・rollback request ruleは今回対象外とする。D2の追加import範囲以外の他runtime、side_io、他engine/crate、manifest/lock、IR/contracts、README/scripts/hooks/CIは変更しない。
  - why: 十五操作の所属変更に別のpolicyや構成の判断を広げない。
  - decided_by: caller（閉じた変更範囲）
