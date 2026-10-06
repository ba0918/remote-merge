use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use chrono::{DateTime, Utc};
use remote_merge_core::path_resolution::{self, ResolvedPath};
use remote_merge_protocol::FileHashResult;

use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

// ── truncation 判定関数 ──

/// ツリースキャンの truncation を検査する。
///
/// `fail_on_truncation` が true ならエラーを返し、false なら warn ログのみで Ok を返す。
pub fn check_truncation(max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<()> {
    if fail_on_truncation {
        anyhow::bail!(
            "Tree scan truncated at {} entries. Results may be incomplete. \
             Use --max-entries <value> to increase the limit, \
             or set max_scan_entries in config, \
             or specify file paths instead of scanning all.",
            max_entries
        );
    }
    tracing::warn!(
        target: "remote_merge::runtime::side_io",
        "Tree scan truncated at {} entries. Results may be incomplete.",
        max_entries
    );
    Ok(())
}

// ── ローカルハッシュ計算 ──

/// ローカルファイルの SHA-256 ハッシュを計算する（純粋関数）。
///
/// シンボリックリンクの場合はリンクターゲットパスを返す。
/// 結果は `FileHashResult` として返す。
pub fn compute_local_file_hash(root_dir: &std::path::Path, rel_path: &str) -> FileHashResult {
    use sha2::{Digest, Sha256};

    let full_path = root_dir.join(rel_path);

    // シンボリックリンク判定
    match std::fs::symlink_metadata(&full_path) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(&full_path) {
            Ok(target) => FileHashResult::Symlink {
                path: rel_path.to_string(),
                target: target.to_string_lossy().into_owned(),
            },
            Err(e) => FileHashResult::Error {
                path: rel_path.to_string(),
                reason: format!("failed to read symlink: {e}"),
            },
        },
        Ok(_) => match std::fs::read(&full_path) {
            Ok(content) => {
                let hash = Sha256::digest(&content);
                FileHashResult::Ok {
                    path: rel_path.to_string(),
                    hash: format!("{hash:x}"),
                }
            }
            Err(e) => FileHashResult::Error {
                path: rel_path.to_string(),
                reason: e.to_string(),
            },
        },
        Err(e) => FileHashResult::Error {
            path: rel_path.to_string(),
            reason: e.to_string(),
        },
    }
}

/// FileHashResult から (path, hash_or_target) ペアを抽出する。
///
/// Ok → hash 文字列、Symlink → target 文字列、Error → None。
pub fn extract_hash_string(result: &FileHashResult) -> Option<(&str, &str)> {
    match result {
        FileHashResult::Ok { path, hash } => Some((path.as_str(), hash.as_str())),
        FileHashResult::Symlink { path, target } => Some((path.as_str(), target.as_str())),
        FileHashResult::Error { .. } => None,
    }
}

/// 複数ファイルのローカルハッシュを一括計算する。
///
/// 結果は path → hash_or_target の HashMap として返す。
/// エラーになったファイルはスキップする。
pub fn compute_local_hashes_batch(
    root_dir: &std::path::Path,
    rel_paths: &[String],
) -> HashMap<String, String> {
    let mut hashes = HashMap::with_capacity(rel_paths.len());
    for rel_path in rel_paths {
        let result = compute_local_file_hash(root_dir, rel_path);
        if let Some((path, hash)) = extract_hash_string(&result) {
            hashes.insert(path.to_string(), hash.to_string());
        }
    }
    hashes
}

/// Agent hash_files 結果を path → hash_or_target の HashMap に変換する。
///
/// エラーになったファイルはスキップする。
pub fn hash_results_to_map(results: &[FileHashResult]) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(results.len());
    for result in results {
        if let Some((path, hash)) = extract_hash_string(result) {
            map.insert(path.to_string(), hash.to_string());
        }
    }
    map
}

// ── subpath ツリー構築ヘルパー ──

/// スキャン結果のノード群を subpath の階層構造でラップする。
///
/// 例: subpath="app/controllers", nodes=[file_0.php, file_1.php]
/// → [app/ → [controllers/ → [file_0.php, file_1.php]]]
///
/// これにより、返却パスが root_dir からの相対パスになる。
pub fn wrap_nodes_in_subpath(subpath: &str, nodes: Vec<FileNode>) -> Vec<FileNode> {
    if subpath.is_empty() {
        return nodes;
    }

    let parts: Vec<&str> = subpath.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return nodes;
    }

    // 最も深い部分から外側に向かってラップしていく
    let mut current = nodes;
    for part in parts.iter().rev() {
        let dir = FileNode::new_dir_with_children(*part, current);
        current = vec![dir];
    }
    current
}

// ── ローカル I/O ヘルパー（純粋関数） ──

/// ローカルファイルの mtime をバッチ取得する
pub fn stat_local_files(
    root_dir: &Path,
    rel_paths: &[String],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    use chrono::TimeZone;

    let mut results = Vec::with_capacity(rel_paths.len());
    for rel_path in rel_paths {
        let full = root_dir.join(rel_path);
        let mtime = std::fs::metadata(&full)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|dur| Utc.timestamp_opt(dur.as_secs() as i64, 0).single());
        results.push((rel_path.clone(), mtime));
    }
    Ok(results)
}

/// ローカルファイルのパーミッションを変更する
#[cfg(unix)]
pub fn chmod_local_file(full_path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let perms = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(full_path, perms)?;
    Ok(())
}

/// Windows ではパーミッション変更は no-op
#[cfg(not(unix))]
pub fn chmod_local_file(_full_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

/// ローカルファイルまたはシンボリックリンクを削除する
pub fn remove_local_file(full_path: &Path) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::remove_file(full_path)
        .with_context(|| format!("Failed to remove file: {}", full_path.display()))
}

/// ローカルにシンボリックリンクを作成する。
///
/// 作成先に何か（通常ファイル・ディレクトリ・symlink）があれば置き換えずに失敗する。比べたときに
/// あった書き込み先は呼び出し元が先に消すので、ここで見つかるのは比べた後に現れたものである。
pub fn create_local_symlink(full_path: &Path, target: &str) -> anyhow::Result<()> {
    // 親ディレクトリを作成
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(target, full_path)?;
    #[cfg(not(unix))]
    {
        // Windows では symlink 作成は未サポート
        anyhow::bail!(
            "Symlink creation is not supported on this platform: {} -> {}",
            full_path.display(),
            target
        );
    }
    Ok(())
}

/// root_dir `root` の下の `rel_path` を "/" から要素ごとに辿って確かめる。
/// 辿れないときのエラーは [`path_resolution::ResolveError`] で、表示は "cannot resolve path: " で始まる
pub fn inspect_local_path(root: &Path, rel_path: &str) -> anyhow::Result<ResolvedPath> {
    executor::validate_remote_path(&root.to_string_lossy(), rel_path)?;
    Ok(path_resolution::os::resolve_on_this_machine(
        root, rel_path,
    )?)
}

pub fn read_file(root: &Path, rel_path: &str) -> anyhow::Result<String> {
    executor::read_local_file(root, rel_path)
        .with_context(|| format!("read local file: {rel_path}"))
}

pub fn read_files_batch(root: &Path, paths: &[String]) -> anyhow::Result<HashMap<String, String>> {
    paths
        .iter()
        .map(|path| Ok((path.clone(), executor::read_local_file(root, path)?)))
        .collect()
}

pub fn read_file_bytes(root: &Path, path: &str, force: bool) -> anyhow::Result<Vec<u8>> {
    executor::read_local_file_bytes(root, path, force)
}

pub fn read_files_bytes_batch(
    root: &Path,
    paths: &[String],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    paths
        .iter()
        .map(|path| {
            Ok((
                path.clone(),
                executor::read_local_file_bytes(root, path, false)?,
            ))
        })
        .collect()
}

pub fn write_file(root: &Path, path: &str, content: &str) -> anyhow::Result<()> {
    executor::write_local_file(root, path, content)
        .with_context(|| format!("write local file: {path}"))
}

pub fn write_file_bytes(root: &Path, path: &str, content: &[u8]) -> anyhow::Result<()> {
    executor::write_local_file_bytes(root, path, content)
}

pub fn stat_files(
    root: &Path,
    paths: &[String],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    for path in paths {
        validated_path(root, path)?;
    }
    stat_local_files(root, paths)
}

pub fn chmod_file(root: &Path, path: &str, mode: u32) -> anyhow::Result<()> {
    chmod_local_file(&validated_path(root, path)?, mode)
}

pub fn remove_file(root: &Path, path: &str) -> anyhow::Result<()> {
    remove_local_file(&validated_path(root, path)?)
}

pub fn create_symlink(root: &Path, path: &str, target: &str) -> anyhow::Result<()> {
    create_local_symlink(&validated_path(root, path)?, target)
}

pub fn fetch_tree(root: &Path, exclude: &[String]) -> anyhow::Result<FileTree> {
    local::scan_local_tree(root, exclude)
}

pub fn fetch_tree_recursive(
    root: &Path,
    exclude: &[String],
    include: &[String],
    max: usize,
    fail: bool,
) -> anyhow::Result<FileTree> {
    let (nodes, truncated) =
        local::scan_local_tree_recursive_with_include(root, exclude, include, max)?;
    if truncated {
        check_truncation(max, fail)?;
    }
    let mut tree = FileTree::new(root);
    tree.nodes = nodes;
    tree.sort();
    Ok(tree)
}

pub fn fetch_tree_for_subpath(
    root: &Path,
    exclude: &[String],
    subpath: &str,
    max: usize,
    fail: bool,
) -> anyhow::Result<FileTree> {
    let subpath = subpath.trim_end_matches('/');
    if std::path::Path::new(subpath).is_absolute() {
        anyhow::bail!("absolute subpath not allowed: {}", subpath);
    }
    if subpath.split('/').any(|part| part == "..") {
        anyhow::bail!("path traversal not allowed: {}", subpath);
    }
    let scan_root = root.join(subpath);
    if !scan_root.exists() || !scan_root.is_dir() {
        return Ok(FileTree::new(root));
    }
    let (nodes, truncated) = local::scan_local_tree_recursive(&scan_root, exclude, max)?;
    if truncated {
        check_truncation(max, fail)?;
    }
    let mut tree = FileTree::new(root);
    tree.nodes = wrap_nodes_in_subpath(subpath, nodes);
    tree.sort();
    Ok(tree)
}

pub fn fetch_children(
    root: &Path,
    exclude: &[String],
    include: &[String],
    path: &str,
) -> anyhow::Result<Vec<FileNode>> {
    // 利用者の入力した絶対パスや .. で root_dir の外を一覧しない
    let nodes = local::scan_dir(&validated_path(root, path)?, exclude, path)?;
    Ok(crate::filter::filter_children_by_include(
        nodes, path, include,
    ))
}

pub fn hashes(root: &Path, paths: &[String]) -> Option<HashMap<String, String>> {
    Some(compute_local_hashes_batch(root, paths))
}

fn validated_path(root: &Path, rel_path: &str) -> anyhow::Result<PathBuf> {
    executor::validate_path_within_root(root, &root.join(rel_path))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// 相対の root_dir は、これまでの canonicalize と同じくプロセスの作業ディレクトリを基準に辿る
    #[test]
    #[serial_test::serial]
    fn a_relative_root_dir_is_resolved_from_the_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(directory.path()).unwrap();
        std::fs::create_dir(base.join("root")).unwrap();
        std::fs::write(base.join("root/file.txt"), "x").unwrap();
        let original = std::env::current_dir().unwrap();

        std::env::set_current_dir(&base).unwrap();
        let relative = inspect_local_path(Path::new("root"), "file.txt");
        std::env::set_current_dir(original).unwrap();

        assert_eq!(
            relative.unwrap(),
            inspect_local_path(&base.join("root"), "file.txt").unwrap()
        );
    }

    /// 上限から遠く "/" で終わらないリンク先だけの普通の状態では、"/" から辿った実パスが
    /// これまで使っていた canonicalize の結果とバイト列で同じになる（既存のバックアップの
    /// 記録と比べ続けられる）
    #[test]
    fn ordinary_paths_resolve_to_the_same_bytes_as_canonicalize() {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_root = std::fs::canonicalize(outside.path()).unwrap();
        std::fs::create_dir_all(root.join("dir/sub")).unwrap();
        std::fs::write(root.join("dir/sub/file.txt"), "x").unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();
        std::fs::write(outside_root.join("shared.txt"), "x").unwrap();
        symlink("dir", root.join("linkdir")).unwrap();
        symlink("dir/sub/file.txt", root.join("linkfile")).unwrap();
        symlink("linkdir/sub", root.join("chain")).unwrap();
        symlink("dir/../file.txt", root.join("dotdot")).unwrap();
        symlink("./dir//sub/./file.txt", root.join("dots")).unwrap();
        symlink(outside_root.join("shared.txt"), root.join("absolute")).unwrap();
        symlink("../sub/file.txt", root.join("dir/sub/up")).unwrap();
        let root_link = tempfile::tempdir().unwrap();
        let linked_root = root_link.path().join("root");
        symlink(&root, &linked_root).unwrap();

        let existing = [
            (root.as_path(), ""),
            (root.as_path(), "file.txt"),
            (root.as_path(), "dir"),
            (root.as_path(), "dir/sub/file.txt"),
            (root.as_path(), "linkdir"),
            (root.as_path(), "linkdir/sub/file.txt"),
            (root.as_path(), "linkfile"),
            (root.as_path(), "chain/file.txt"),
            (root.as_path(), "chain"),
            (root.as_path(), "dotdot"),
            (root.as_path(), "dots"),
            (root.as_path(), "absolute"),
            (root.as_path(), "dir/sub/up"),
            (linked_root.as_path(), ""),
            (linked_root.as_path(), "linkdir/sub"),
        ];
        for (base, rel) in existing {
            let expected = std::fs::canonicalize(base.join(rel)).unwrap();
            let real_path = match inspect_local_path(base, rel).unwrap() {
                ResolvedPath::Exists { real_path } | ResolvedPath::Symlink { real_path, .. } => {
                    real_path
                }
                other => panic!("{rel}: {other:?}"),
            };
            assert_eq!(
                real_path.as_bytes(),
                expected.as_os_str().as_encoded_bytes(),
                "{rel}"
            );
        }

        let missing = [
            ("missing.txt", "", true),
            ("linkdir/new/a.txt", "dir/new", false),
        ];
        for (rel, parent, parent_exists) in missing {
            let expected = std::fs::canonicalize(&root).unwrap().join(parent);
            let expected = expected.to_str().unwrap().trim_end_matches('/');
            assert_eq!(
                inspect_local_path(&root, rel).unwrap(),
                ResolvedPath::Missing {
                    real_parent: expected.to_string(),
                    parent_exists,
                },
                "{rel}"
            );
        }
    }
}
