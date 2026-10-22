//! コマンドの引数の型を作る契約テストの道具。
//!
//! 製品の引数の型は Default を持たないので、テストの既定をここに置き、テストは既定と違う欄だけを
//! 構造体の更新の構文（`MergeArgs { force: true, ..merge_args(&["a.txt"]) }`）で書く。既定は、
//! local を左、develop を右にした JSON の出力で、ほかの指定はない。sync と rollback は、書き込む
//! 予定があると確認のプロンプトで標準入力を読むため --force を付ける。

use remote_merge::cli::diff::DiffArgs;
use remote_merge::cli::merge::MergeArgs;
use remote_merge::cli::rollback::RollbackArgs;
use remote_merge::cli::sync::SyncArgs;

fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// local から develop へ `paths` を書き込む merge の引数
pub fn merge_args(paths: &[&str]) -> MergeArgs {
    MergeArgs {
        paths: owned(paths),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        dry_run: false,
        force: false,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
        hunks: None,
        encodings: Default::default(),
    }
}

/// local から develop へ `paths` を --force で同期する sync の引数
pub fn sync_args(paths: &[&str]) -> SyncArgs {
    SyncArgs {
        paths: owned(paths),
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

/// local と develop の `paths` を比べる diff の引数（--max-files 100）
pub fn diff_args(paths: &[&str]) -> DiffArgs {
    DiffArgs {
        paths: owned(paths),
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        max_lines: None,
        max_files: 100,
        force: false,
        follow_external_links: false,
        encodings: Default::default(),
        max_entries: None,
    }
}

/// `target` の最新のセッションを --force で書き戻す rollback の引数
pub fn rollback_args(target: &str) -> RollbackArgs {
    RollbackArgs {
        target: Some(target.into()),
        list: false,
        session: None,
        dry_run: false,
        force: true,
        format: "json".into(),
    }
}
