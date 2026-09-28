# Glossary

| Term | Meaning | Source |
|---|---|---|
| バックアップセッション | 1 回の書き込み操作（merge、sync、rollback、TUI のハンクマージの確定、TUI の w）で取ったバックアップの一式。セッション ID で識別し、TUI の起動期間は指さない。 | docs/decision/records/2026-09-27-adopt-backup.md#A32 |
| 書き込み先 | バックアップを区別する単位。リモートはホスト名・ポート・root_dir の組、ローカルは root_dir の絶対パスで決まる。設定上のサーバ名とは異なる。 | docs/decision/records/2026-09-27-adopt-backup.md#A33 |
| 末尾の symlink | 操作対象のパスそのものが symlink である状態。 | docs/decision/records/legacy-terms.md#末尾の symlink |
| 途中の symlink | 操作対象の親ディレクトリのうち root_dir より下に symlink がある状態。root_dir 自体の symlink は含めない。 | docs/decision/records/legacy-terms.md#途中の symlink |
| 機密ファイル | 設定の sensitive パターン（グローバル設定とプロジェクト設定の和）にファイル名かパスが一致するファイル。 | docs/decision/records/2026-09-27-adopt-status.md#A12 |
| 参照先 | --ref で指定し、左右との比較にだけ使うサーバまたは local。 | docs/decision/records/2026-09-27-adopt-status.md#A13 |
| 既定サーバ | 設定のサーバ名を名前順に並べて最初のサーバ。 | docs/decision/records/2026-09-27-adopt-status.md#A14 |
| 競合 | 参照先に対して左右が同じ箇所を異なる内容に変えたこと。同じ箇所とは参照先からの変更の行の範囲が重なることで、バイナリや UTF-8 として読めないファイルはファイル全体を一つの箇所とする。左右の両方が参照先から変わっていても、変わった箇所が重ならなければ競合と呼ばない。 | docs/decision/records/2026-09-28-merge-ref-hunks-fix.md#A4 |
