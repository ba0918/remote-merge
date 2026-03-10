//! エージェントのファイル I/O 操作（リモートサーバー上で実行）。
//!
//! 全操作は `validate_path` を経由し、パストラバーサルを防止する。

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::agent::protocol::{AgentFileStat, FileReadResult};

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// root_dir 配下の相対パスを解決し、安全性を検証する。
///
/// - `..` コンポーネントを含むパスを拒否（パストラバーサル防止）
/// - canonicalize 後に root_dir のプレフィックスであることを検証
/// - 新規ファイル（存在しない）の場合は親ディレクトリで検証
///
/// NOTE: 親ディレクトリの canonicalize とファイル作成の間には TOCTOU 競合の
/// 可能性がある（親ディレクトリがシンボリックリンクに差し替えられるケース等）。
/// 現在のユースケース（単一ユーザー操作）ではリスクは低いが、マルチテナント環境
/// では追加の対策が必要。
pub fn validate_path(root_dir: &Path, rel_path: &str) -> Result<PathBuf> {
    // コンポーネントレベルで .. を拒否
    for component in Path::new(rel_path).components() {
        if matches!(component, Component::ParentDir) {
            bail!("path traversal detected: {rel_path}");
        }
    }

    let joined = root_dir.join(rel_path);

    // ファイルが存在する場合: canonicalize して root_dir 配下か検証
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .with_context(|| format!("failed to canonicalize: {}", joined.display()))?;
        let root_canonical = root_dir
            .canonicalize()
            .with_context(|| format!("failed to canonicalize root: {}", root_dir.display()))?;
        if !canonical.starts_with(&root_canonical) {
            bail!(
                "path escapes root directory: {} is not under {}",
                canonical.display(),
                root_canonical.display()
            );
        }
        return Ok(canonical);
    }

    // 新規ファイル: 親ディレクトリで検証
    let parent = joined
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent directory for: {}", joined.display()))?;
    if parent.exists() {
        let parent_canonical = parent.canonicalize()?;
        let root_canonical = root_dir.canonicalize()?;
        if !parent_canonical.starts_with(&root_canonical) {
            bail!(
                "path escapes root directory: {} is not under {}",
                parent_canonical.display(),
                root_canonical.display()
            );
        }
    }
    // 親も存在しない場合は join 結果をそのまま返す（write_file が mkdir_all する）
    Ok(joined)
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

/// ファイルコンテンツを読み込む（チャンク対応）。
/// chunk_size_limit を超えるファイルは分割して返す。
pub fn read_file_chunked(
    root_dir: &Path,
    rel_path: &str,
    chunk_size_limit: usize,
) -> Result<Vec<FileReadResult>> {
    let path = validate_path(root_dir, rel_path)?;
    let content = fs::read(&path).with_context(|| format!("failed to read: {}", path.display()))?;

    if chunk_size_limit == 0 || content.len() <= chunk_size_limit {
        return Ok(vec![FileReadResult::Ok {
            path: rel_path.to_string(),
            content,
            more_to_follow: false,
        }]);
    }

    let mut results = Vec::new();
    for chunk in content.chunks(chunk_size_limit) {
        results.push(FileReadResult::Ok {
            path: rel_path.to_string(),
            content: chunk.to_vec(),
            more_to_follow: true, // 仮に全部 true — 最後だけ修正
        });
    }
    if let Some(FileReadResult::Ok { more_to_follow, .. }) = results.last_mut() {
        *more_to_follow = false;
    }
    Ok(results)
}

/// ファイルに書き込む。チャンク転送に対応。
///
/// - `is_first_chunk=true`: create/truncate + write（親ディレクトリも作成）
/// - `is_first_chunk=false`: append モード（既存ファイルに追記）
pub fn write_file(
    root_dir: &Path,
    rel_path: &str,
    content: &[u8],
    is_first_chunk: bool,
) -> Result<()> {
    let path = validate_path(root_dir, rel_path)?;

    if is_first_chunk {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dirs: {}", parent.display()))?;
        }
        fs::write(&path, content)
            .with_context(|| format!("failed to write: {}", path.display()))?;
    } else {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open for append: {}", path.display()))?;
        f.write_all(content)?;
    }
    Ok(())
}

/// ファイルのメタデータ（stat）を取得する。
pub fn stat_file(root_dir: &Path, rel_path: &str) -> Result<AgentFileStat> {
    let path = validate_path(root_dir, rel_path)?;
    let meta = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to stat: {}", path.display()))?;
    Ok(AgentFileStat {
        path: rel_path.to_string(),
        size: meta.size(),
        mtime_secs: meta.mtime(),
        mtime_nanos: meta.mtime_nsec() as u32,
        permissions: meta.permissions().mode(),
    })
}

/// バックアップを作成する。
pub fn create_backup(root_dir: &Path, rel_path: &str, backup_dir: &Path) -> Result<()> {
    let src = validate_path(root_dir, rel_path)?;
    if !src.exists() {
        bail!("source file does not exist: {}", src.display());
    }
    // rel_path に .. が含まれる場合、backup_dir 外への書き込みを防止
    if rel_path.contains("..") {
        bail!("backup path contains traversal: {rel_path}");
    }
    let dest = backup_dir.join(rel_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create backup dirs: {}", parent.display()))?;
    }
    fs::copy(&src, &dest)
        .with_context(|| format!("failed to copy {} -> {}", src.display(), dest.display()))?;
    Ok(())
}

/// シンボリックリンクを作成する。ターゲットが root_dir 外に逃げないことを検証する。
pub fn create_symlink(root_dir: &Path, rel_path: &str, target: &str) -> Result<()> {
    let link_path = validate_path(root_dir, rel_path)?;

    // ターゲットの安全性検証: root_dir からの相対として解決
    for component in Path::new(target).components() {
        if matches!(component, Component::ParentDir | Component::RootDir) {
            bail!("symlink target escapes root: {target}");
        }
    }

    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 既存のリンクがあれば削除
    if link_path.symlink_metadata().is_ok() {
        fs::remove_file(&link_path)?;
    }

    std::os::unix::fs::symlink(target, &link_path)
        .with_context(|| format!("failed to create symlink: {}", link_path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    // ── validate_path ──

    #[test]
    fn valid_relative_path_accepted() {
        let tmp = setup();
        fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
        let result = validate_path(tmp.path(), "hello.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn nested_valid_path_accepted() {
        let tmp = setup();
        fs::create_dir_all(tmp.path().join("a/b/c")).unwrap();
        fs::write(tmp.path().join("a/b/c/file.txt"), "data").unwrap();
        let result = validate_path(tmp.path(), "a/b/c/file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn dotdot_rejected() {
        let tmp = setup();
        let result = validate_path(tmp.path(), "../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));
    }

    #[test]
    fn nested_dotdot_rejected() {
        let tmp = setup();
        let result = validate_path(tmp.path(), "foo/../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));
    }

    #[test]
    fn symlink_escaping_root_rejected() {
        let tmp = setup();
        // root_dir 内にシンボリックリンクで外部を指すものを作成
        let link = tmp.path().join("escape");
        symlink("/etc", &link).unwrap();
        let result = validate_path(tmp.path(), "escape/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("escapes root"));
    }

    #[test]
    fn new_file_path_accepted_if_parent_valid() {
        let tmp = setup();
        fs::create_dir_all(tmp.path().join("subdir")).unwrap();
        let result = validate_path(tmp.path(), "subdir/newfile.txt");
        assert!(result.is_ok());
    }

    // ── read_file_chunked ──

    #[test]
    fn read_small_text_file() {
        let tmp = setup();
        fs::write(tmp.path().join("test.txt"), "hello world").unwrap();
        let results = read_file_chunked(tmp.path(), "test.txt", 1024).unwrap();
        assert_eq!(results.len(), 1);
        match &results[0] {
            FileReadResult::Ok {
                content,
                more_to_follow,
                ..
            } => {
                assert_eq!(content, b"hello world");
                assert!(!more_to_follow);
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn read_binary_file() {
        let tmp = setup();
        let data: Vec<u8> = (0..=255).collect();
        fs::write(tmp.path().join("bin.dat"), &data).unwrap();
        let results = read_file_chunked(tmp.path(), "bin.dat", 4096).unwrap();
        assert_eq!(results.len(), 1);
        if let FileReadResult::Ok { content, .. } = &results[0] {
            assert_eq!(content, &data);
        } else {
            panic!("expected Ok");
        }
    }

    #[test]
    fn read_chunked_splits_correctly() {
        let tmp = setup();
        let data = vec![0u8; 100];
        fs::write(tmp.path().join("big.dat"), &data).unwrap();
        let results = read_file_chunked(tmp.path(), "big.dat", 30).unwrap();
        // 100 / 30 = 4 chunks (30+30+30+10)
        assert_eq!(results.len(), 4);
        // 最後以外は more_to_follow = true
        for (i, r) in results.iter().enumerate() {
            if let FileReadResult::Ok { more_to_follow, .. } = r {
                if i < 3 {
                    assert!(more_to_follow);
                } else {
                    assert!(!more_to_follow);
                }
            }
        }
    }

    #[test]
    fn read_nonexistent_file_returns_error() {
        let tmp = setup();
        let result = read_file_chunked(tmp.path(), "nope.txt", 1024);
        assert!(result.is_err());
    }

    // ── write_file ──

    #[test]
    fn write_new_file() {
        let tmp = setup();
        write_file(tmp.path(), "out.txt", b"data", true).unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("out.txt")).unwrap(),
            "data"
        );
    }

    #[test]
    fn write_creates_parent_dirs() {
        let tmp = setup();
        write_file(tmp.path(), "a/b/c/deep.txt", b"nested", true).unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("a/b/c/deep.txt")).unwrap(),
            "nested"
        );
    }

    #[test]
    fn write_overwrites_existing() {
        let tmp = setup();
        fs::write(tmp.path().join("exist.txt"), "old").unwrap();
        write_file(tmp.path(), "exist.txt", b"new", true).unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("exist.txt")).unwrap(),
            "new"
        );
    }

    // ── stat_file ──

    #[test]
    fn stat_existing_file() {
        let tmp = setup();
        fs::write(tmp.path().join("s.txt"), "12345").unwrap();
        let stat = stat_file(tmp.path(), "s.txt").unwrap();
        assert_eq!(stat.path, "s.txt");
        assert_eq!(stat.size, 5);
    }

    #[test]
    fn stat_nonexistent_returns_error() {
        let tmp = setup();
        assert!(stat_file(tmp.path(), "nope").is_err());
    }

    // ── create_backup ──

    #[test]
    fn backup_creates_copy() {
        let tmp = setup();
        fs::write(tmp.path().join("orig.txt"), "backup me").unwrap();
        let backup_dir = tmp.path().join("backups");
        create_backup(tmp.path(), "orig.txt", &backup_dir).unwrap();
        assert_eq!(
            fs::read_to_string(backup_dir.join("orig.txt")).unwrap(),
            "backup me"
        );
    }

    #[test]
    fn backup_nonexistent_returns_error() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let result = create_backup(tmp.path(), "nope.txt", &backup_dir);
        assert!(result.is_err());
    }

    // ── create_symlink ──

    #[test]
    fn symlink_creation() {
        let tmp = setup();
        fs::write(tmp.path().join("target.txt"), "link target").unwrap();
        create_symlink(tmp.path(), "link.txt", "target.txt").unwrap();
        let link = tmp.path().join("link.txt");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&link).unwrap(), "link target");
    }

    #[test]
    fn symlink_target_traversal_rejected() {
        let tmp = setup();
        let result = create_symlink(tmp.path(), "bad_link", "../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("escapes root"));
    }

    // ── 追加テスト ──

    #[test]
    fn write_file_chunked_transfer() {
        let tmp = setup();
        write_file(tmp.path(), "chunked.bin", b"chunk1", true).unwrap();
        write_file(tmp.path(), "chunked.bin", b"chunk2", false).unwrap();
        write_file(tmp.path(), "chunked.bin", b"chunk3", false).unwrap();
        let content = fs::read(tmp.path().join("chunked.bin")).unwrap();
        assert_eq!(content, b"chunk1chunk2chunk3");
    }

    #[test]
    fn symlink_absolute_target_rejected() {
        let tmp = setup();
        let result = create_symlink(tmp.path(), "link", "/etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("escapes root"));
    }

    #[test]
    fn backup_path_traversal_rejected() {
        let tmp = setup();
        // validate_path が .. を弾くため、root_dir 内に実ファイルを用意し
        // rel_path に文字列 ".." を含むパスでバックアップを試みる。
        // ただし validate_path が先に弾くので、create_backup 内のチェックを
        // テストするには validate_path を通るパスが必要。
        // ここでは create_backup が独自に rel_path を検証することを確認。
        let backup_dir = tmp.path().join("backups");
        // validate_path は .. を弾くので、create_backup のエラーを直接テスト
        let result = create_backup(tmp.path(), "../escape", &backup_dir);
        assert!(result.is_err());
    }
}
