# Glossary

| Term | Meaning | Source |
|---|---|---|
| 通常テスト | 開発者が Docker や個人の SSH 設定を準備せずに実行する Rust の全機能テスト。 | docs/decision/records/2026-09-26-reproducible-test-environments.md#A13 |
| 実 OpenSSH 検証 | OS の sshd・権限・所有者を用い、隔離したコンテナで実行する必須 CI ジョブ。 | docs/decision/records/2026-09-26-reproducible-test-environments.md#A7 |
| 旧 SSH 負荷環境 | 旧サーバーと大量のファイルを用いる手動の負荷試験用環境。通常テストと必須 CI の一部ではない。 | docs/decision/records/2026-09-26-reproducible-test-environments.md#A8 |
