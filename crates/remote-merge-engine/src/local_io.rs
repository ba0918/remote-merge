use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use chrono::{DateTime, Utc};
use remote_merge_core::path_resolution::{self, ResolvedPath};
use remote_merge_core::relative_path::RelPath;
use remote_merge_protocol::FileHashResult;

use crate::local;
use crate::merge::executor;
use crate::tree::{FileNode, FileTree};

// ── truncation 判定関数 ──

/// 走査が件数の上限に当たって打ち切られたこと。文は上限の上げ方とパスの指定の仕方を案内する
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanTruncated {
    pub max_entries: usize,
}

impl std::fmt::Display for ScanTruncated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tree scan truncated at {} entries. Results may be incomplete. \
             Use --max-entries <value> to increase the limit, \
             or set max_scan_entries in config, \
             or specify file paths instead of scanning all.",
            self.max_entries
        )
    }
}

impl std::error::Error for ScanTruncated {}

/// ツリースキャンの truncation を検査する。
///
/// `fail_on_truncation` が true なら [`ScanTruncated`] のエラーを返し、false なら warn ログのみで
/// Ok を返す。
pub fn check_truncation(max_entries: usize, fail_on_truncation: bool) -> anyhow::Result<()> {
    if fail_on_truncation {
        return Err(ScanTruncated { max_entries }.into());
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

/// サブパスを起点に走査した、読めない名前のファイルのパスを root_dir からのパスにする
pub fn names_in_subpath(subpath: &str, names: Vec<String>) -> Vec<String> {
    let subpath = subpath.trim_end_matches('/');
    if subpath.is_empty() {
        return names;
    }
    names
        .into_iter()
        .map(|name| format!("{subpath}/{name}"))
        .collect()
}

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
/// 辿れないときのエラーは [`path_resolution::ResolveError`] で、表示は "cannot resolve path: " で始まる。
/// 相対の root_dir は設定を読む段階で設定ファイルの場所を基準に絶対パスにするので、ここには
/// 絶対パスだけが届く。届いた相対パスの基準は推し量らずにエラーにする
pub fn inspect_local_path(root: &Path, rel_path: &RelPath) -> anyhow::Result<ResolvedPath> {
    if !root.is_absolute() {
        anyhow::bail!("local root_dir is not an absolute path: {}", root.display());
    }
    Ok(path_resolution::os::resolve_on_this_machine(
        root,
        rel_path.path_to_resolve(),
    )?)
}

pub fn read_file_bytes(root: &Path, path: &RelPath) -> anyhow::Result<Vec<u8>> {
    executor::read_local_file_bytes(root, path)
}

/// パスごとの中身。キーは入力されたままのパス
pub fn read_files_bytes_batch(
    root: &Path,
    paths: &[RelPath],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    paths
        .iter()
        .map(|path| {
            Ok((
                path.as_typed().to_string(),
                executor::read_local_file_bytes(root, path)?,
            ))
        })
        .collect()
}

/// パスごとの、symlink を辿った先のファイルの大きさ。調べられないパス（ない、権限がない）は
/// None。キーは入力されたままのパス
pub fn file_sizes(root: &Path, paths: &[RelPath]) -> HashMap<String, Option<u64>> {
    paths
        .iter()
        .map(|path| {
            let size = std::fs::metadata(root.join(path.as_str()))
                .ok()
                .map(|metadata| metadata.len());
            (path.as_typed().to_string(), size)
        })
        .collect()
}

pub fn write_file(root: &Path, path: &RelPath, content: &str) -> anyhow::Result<()> {
    executor::write_local_file(root, path, content)
        .with_context(|| format!("write local file: {}", path.as_typed()))
}

pub fn write_file_bytes(root: &Path, path: &RelPath, content: &[u8]) -> anyhow::Result<()> {
    executor::write_local_file_bytes(root, path, content)
}

/// パスごとの更新時刻。キーは入力されたままのパス
pub fn stat_files(
    root: &Path,
    paths: &[RelPath],
) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    let mut stats = stat_local_files(root, &operation_paths(paths))?;
    for ((path, _), rel) in stats.iter_mut().zip(paths) {
        *path = rel.as_typed().to_string();
    }
    Ok(stats)
}

/// root_dir からの `path` の権限を変える（docs/ir/paths/operations.md。エージェントと同じ手順）
pub fn chmod_file(root: &Path, path: &RelPath, mode: u32) -> anyhow::Result<()> {
    remote_merge_core::operations::chmod(root, path, mode)
}

/// root_dir からの `path` を消す（docs/ir/paths/operations.md。エージェントと同じ手順）
pub fn remove_file(root: &Path, path: &RelPath) -> anyhow::Result<()> {
    remote_merge_core::operations::remove(root, path)
}

pub fn create_symlink(root: &Path, path: &RelPath, target: &str) -> anyhow::Result<()> {
    create_local_symlink(&root.join(path.as_str()), target)
}

/// 末尾の "/" を除いた、操作に渡す形のパス
fn operation_paths(paths: &[RelPath]) -> Vec<String> {
    paths.iter().map(|path| path.as_str().to_string()).collect()
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
    let scan = local::scan_local_tree_recursive_with_include(root, exclude, include, max)?;
    if scan.truncated {
        check_truncation(max, fail)?;
    }
    let mut tree = FileTree::new(root);
    tree.nodes = scan.nodes;
    tree.unreadable_names = scan.unreadable_names;
    tree.truncated = scan.truncated;
    tree.sort();
    Ok(tree)
}

pub fn fetch_tree_for_subpath(
    root: &Path,
    exclude: &[String],
    subpath: &RelPath,
    max: usize,
    fail: bool,
) -> anyhow::Result<FileTree> {
    let subpath = subpath.as_str();
    let scan_root = root.join(subpath);
    if !scan_root.exists() || !scan_root.is_dir() {
        return Ok(FileTree::new(root));
    }
    let scan = local::scan_local_tree_recursive(&scan_root, exclude, max)?;
    if scan.truncated {
        check_truncation(max, fail)?;
    }
    let mut tree = FileTree::new(root);
    tree.nodes = wrap_nodes_in_subpath(subpath, scan.nodes);
    tree.unreadable_names = names_in_subpath(subpath, scan.unreadable_names);
    tree.truncated = scan.truncated;
    tree.sort();
    Ok(tree)
}

pub fn fetch_children(
    root: &Path,
    exclude: &[String],
    include: &[String],
    path: &RelPath,
) -> anyhow::Result<Vec<FileNode>> {
    let full = root.join(path.as_str());
    let path = path.as_typed();
    let nodes = local::scan_dir(&full, exclude, path)?;
    Ok(crate::filter::filter_children_by_include(
        nodes, path, include,
    ))
}

/// root_dir からの `path` の直下の子を、三つの結果を区別して返す（docs/ir/paths/operations.md。
/// エージェントと同じ手順）
pub fn list_children(
    root: &Path,
    exclude: &[String],
    path: &RelPath,
) -> anyhow::Result<remote_merge_core::listing::ChildListing> {
    remote_merge_core::listing::list_children(
        root,
        path,
        exclude,
        crate::config::DEFAULT_MAX_DIR_ENTRIES,
    )
}

/// パスごとのハッシュ（symlink はリンク先）。キーは入力されたままのパス
pub fn hashes(root: &Path, paths: &[RelPath]) -> Option<HashMap<String, String>> {
    Some(
        paths
            .iter()
            .filter_map(|path| {
                let result = compute_local_file_hash(root, path.as_str());
                extract_hash_string(&result)
                    .map(|(_, hash)| (path.as_typed().to_string(), hash.to_string()))
            })
            .collect(),
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

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
            let real_path = match inspect_local_path(base, &RelPath::parse(rel).unwrap()).unwrap() {
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
                inspect_local_path(&root, &RelPath::parse(rel).unwrap()).unwrap(),
                ResolvedPath::Missing {
                    real_parent: expected.to_string(),
                    parent_exists,
                },
                "{rel}"
            );
        }
    }

    #[test]
    fn file_sizes_follow_symlinks_and_leave_unknown_paths_out() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("file"), "12345").unwrap();
        symlink("file", root.join("link")).unwrap();
        let paths: Vec<RelPath> = ["file", "link", "missing"]
            .map(|path| RelPath::parse(path).unwrap())
            .to_vec();

        let sizes = file_sizes(root, &paths);

        assert_eq!(sizes["file"], Some(5));
        assert_eq!(sizes["link"], Some(5));
        assert_eq!(sizes["missing"], None);
    }
}
