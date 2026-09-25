# Glossary

| Term | Meaning | Source |
|---|---|---|
| バックアップセッション | 1 回の書き込み操作で取ったバックアップの一式。セッション ID で識別し、TUI の起動期間は指さない。 | docs/decision/records/legacy-terms.md#バックアップセッション |
| 書き込み先 | バックアップを区別する単位。リモートはホスト名・ポート・root_dir の組、ローカルは root_dir の絶対パスで決まる。設定上のサーバ名とは異なる。 | docs/decision/records/legacy-terms.md#書き込み先 |
| 末尾の symlink | 操作対象のパスそのものが symlink である状態。 | docs/decision/records/legacy-terms.md#末尾の symlink |
| 途中の symlink | 操作対象の親ディレクトリのうち root_dir より下に symlink がある状態。root_dir 自体の symlink は含めない。 | docs/decision/records/legacy-terms.md#途中の symlink |
