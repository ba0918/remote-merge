//! エージェントのファイル I/O 操作（リモートサーバー上で実行）。
//!
//! 全操作は `validate_path` を経由し、パストラバーサルを防止する。

use std::ffi::CString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::agent::protocol::{
    AgentBackupFile, AgentBackupSession, AgentFileStat, AgentRestoreFileResult, FileReadResult,
};
use crate::agent::server::MetadataConfig;

// ---------------------------------------------------------------------------
// Saved metadata (for write_file restore)
// ---------------------------------------------------------------------------

/// 既存ファイルの書き込み前メタデータ。write_file の最終チャンクで復元に使用。
#[derive(Debug, Clone)]
pub struct SavedMetadata {
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

impl SavedMetadata {
    /// ファイルが存在する場合にメタデータを取得して保存する。
    pub fn from_path(path: &Path) -> Option<Self> {
        fs::symlink_metadata(path).ok().map(|meta| Self {
            uid: meta.uid(),
            gid: meta.gid(),
            mode: meta.mode(),
        })
    }
}

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
    // 絶対パスを拒否 — root_dir 外のファイルアクセスを防止
    if Path::new(rel_path).is_absolute() {
        bail!("absolute path not allowed: {rel_path}");
    }

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
// Metadata operations
// ---------------------------------------------------------------------------

/// 現在のプロセスの effective UID が 0 (root) かどうかを返す。
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// パスを CString に変換する。NUL バイトを含む場合は None を返す。
fn path_to_cstring(path: &Path) -> Option<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).ok()
}

/// ファイルの owner/group/permissions を設定する。
///
/// - `canonical_path`: `validate_path` 経由で検証済みのパス
/// - euid == 0 の場合のみ chown を実行
/// - chmod/chown 失敗はセキュリティ上重要なので warn レベルでログ出力し、エラーにはしない
pub fn apply_metadata(
    canonical_path: &Path,
    uid: Option<u32>,
    gid: Option<u32>,
    mode: Option<u32>,
) -> std::io::Result<()> {
    // chmod
    if let Some(m) = mode {
        // mode の下位12ビットのみ使用（permission bits）
        let perms = fs::Permissions::from_mode(m & 0o7777);
        if let Err(e) = fs::set_permissions(canonical_path, perms) {
            tracing::warn!("chmod failed for {}: {e}", canonical_path.display());
        }
    }

    // chown（root のみ）
    if is_root() && (uid.is_some() || gid.is_some()) {
        apply_chown(canonical_path, uid, gid);
    }

    Ok(())
}

/// chown を実行する。失敗時は warn レベルでログ出力のみ。
fn apply_chown(path: &Path, uid: Option<u32>, gid: Option<u32>) {
    let Some(c_path) = path_to_cstring(path) else {
        tracing::warn!("chown skipped: path contains NUL byte: {}", path.display());
        return;
    };
    let uid_val = uid.map(|u| u as libc::uid_t).unwrap_or(u32::MAX);
    let gid_val = gid.map(|g| g as libc::gid_t).unwrap_or(u32::MAX);
    let ret = unsafe { libc::chown(c_path.as_ptr(), uid_val, gid_val) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        tracing::warn!("chown failed for {}: {err}", path.display());
    }
}

/// lchown を実行する（シンボリックリンク自体の所有権を変更）。失敗時は warn レベルでログ出力のみ。
fn apply_lchown(path: &Path, uid: Option<u32>, gid: Option<u32>) {
    let Some(c_path) = path_to_cstring(path) else {
        tracing::warn!("lchown skipped: path contains NUL byte: {}", path.display());
        return;
    };
    let uid_val = uid.map(|u| u as libc::uid_t).unwrap_or(u32::MAX);
    let gid_val = gid.map(|g| g as libc::gid_t).unwrap_or(u32::MAX);
    let ret = unsafe { libc::lchown(c_path.as_ptr(), uid_val, gid_val) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        tracing::warn!("lchown failed for {}: {err}", path.display());
    }
}

/// ディレクトリにパーミッションを適用する。
///
/// `create_dir_all` で作成された新規ディレクトリに対して、`dir_permissions` を設定する。
/// 既存ディレクトリは変更しない。
pub fn apply_dir_permissions(path: &Path, mode: Option<u32>) -> std::io::Result<()> {
    if let Some(m) = mode {
        let perms = fs::Permissions::from_mode(m & 0o7777);
        if let Err(e) = fs::set_permissions(path, perms) {
            tracing::debug!("chmod failed for directory {}: {e}", path.display());
        }
    }
    Ok(())
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

/// メタデータ復元付きファイル書き込み。チャンク転送に対応。
///
/// `write_file` と同等の書き込みを行い、追加で以下を実行:
/// - `is_first_chunk=true`: 既存ファイルのメタデータを `saved_metadata` に保存して返す
/// - `is_first_chunk=true`: 新規ディレクトリに `config.dir_permissions` を適用
/// - `is_last_chunk=true`（最終チャンク）: メタデータを復元/適用
///   - 既存ファイルだった場合: `saved_metadata` の uid/gid/mode で復元
///   - 新規ファイルだった場合: `config` のデフォルト値で設定
pub fn write_file_with_metadata(
    root_dir: &Path,
    rel_path: &str,
    content: &[u8],
    is_first_chunk: bool,
    is_last_chunk: bool,
    saved_metadata: Option<&SavedMetadata>,
    config: &MetadataConfig,
) -> Result<Option<SavedMetadata>> {
    let path = validate_path(root_dir, rel_path)?;

    let mut captured_metadata = None;

    if is_first_chunk {
        // 既存ファイルのメタデータを保存
        captured_metadata = SavedMetadata::from_path(&path);

        // 親ディレクトリの作成（新規ディレクトリにはパーミッション適用）
        if let Some(parent) = path.parent() {
            // create_dir_all 前に新規作成されるディレクトリを特定
            let new_dirs = if config.dir_permissions.is_some() {
                find_dirs_to_create(root_dir, parent)
            } else {
                vec![]
            };
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dirs: {}", parent.display()))?;
            // 新規作成されたディレクトリに dir_permissions を適用
            if !new_dirs.is_empty() {
                apply_dir_permissions_recursive(&new_dirs, config.dir_permissions)?;
            }
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

    // 最終チャンクでメタデータを適用
    if is_last_chunk {
        let effective_saved = saved_metadata.or(captured_metadata.as_ref());
        match effective_saved {
            Some(meta) => {
                // 既存ファイルだった: 保存したメタデータで復元
                let _ = apply_metadata(&path, Some(meta.uid), Some(meta.gid), Some(meta.mode));
            }
            None => {
                // 新規ファイルだった: config のデフォルト値で設定
                let _ = apply_metadata(
                    &path,
                    config.default_uid,
                    config.default_gid,
                    config.file_permissions,
                );
            }
        }
    }

    Ok(captured_metadata)
}

/// root_dir から dir までの各ディレクトリに対してパーミッションを適用する。
///
/// `create_dir_all` で複数階層が新規作成された場合、`newly_created` で
/// 指定された新規ディレクトリにのみ `dir_permissions` を適用する。
/// root_dir 自体は除外する。
fn apply_dir_permissions_recursive(newly_created: &[PathBuf], mode: Option<u32>) -> Result<()> {
    for dir in newly_created {
        let _ = apply_dir_permissions(dir, mode);
    }
    Ok(())
}

/// root_dir から parent までのパスを列挙し、事前に存在しなかったディレクトリを特定する。
///
/// `create_dir_all` の前に呼び出し、戻り値を `apply_dir_permissions_recursive` に渡す。
fn find_dirs_to_create(root_dir: &Path, parent: &Path) -> Vec<PathBuf> {
    let Ok(rel) = parent.strip_prefix(root_dir) else {
        return vec![];
    };
    let mut dirs_to_create = Vec::new();
    let mut current = root_dir.to_path_buf();
    for component in rel.components() {
        current = current.join(component);
        if !current.exists() {
            dirs_to_create.push(current.clone());
        }
    }
    dirs_to_create
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
    create_symlink_with_metadata(root_dir, rel_path, target, None, None, None)
}

/// メタデータ付きシンボリックリンクを作成する。
///
/// symlink 作成後、euid==0 の場合のみ lchown で uid/gid を設定する。
/// `dir_permissions` が指定されている場合、新規作成されたディレクトリにパーミッションを適用する。
pub fn create_symlink_with_metadata(
    root_dir: &Path,
    rel_path: &str,
    target: &str,
    uid: Option<u32>,
    gid: Option<u32>,
    dir_permissions: Option<u32>,
) -> Result<()> {
    let link_path = validate_path(root_dir, rel_path)?;

    // ターゲットの安全性検証: root_dir からの相対として解決
    for component in Path::new(target).components() {
        if matches!(component, Component::ParentDir | Component::RootDir) {
            bail!("symlink target escapes root: {target}");
        }
    }

    if let Some(parent) = link_path.parent() {
        // create_dir_all 前に新規作成されるディレクトリを特定
        let new_dirs = if dir_permissions.is_some() {
            find_dirs_to_create(root_dir, parent)
        } else {
            vec![]
        };
        fs::create_dir_all(parent)?;
        // 新規作成されたディレクトリに dir_permissions を適用
        if !new_dirs.is_empty() {
            apply_dir_permissions_recursive(&new_dirs, dir_permissions)?;
        }
    }

    // 既存のリンクがあれば削除
    if link_path.symlink_metadata().is_ok() {
        fs::remove_file(&link_path)?;
    }

    std::os::unix::fs::symlink(target, &link_path)
        .with_context(|| format!("failed to create symlink: {}", link_path.display()))?;

    // lchown（root のみ）
    if is_root() && (uid.is_some() || gid.is_some()) {
        apply_lchown(&link_path, uid, gid);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Backup session operations
// ---------------------------------------------------------------------------

/// バックアップセッション一覧を取得する。
///
/// `backup_dir` 内のタイムスタンプ形式ディレクトリを走査し、
/// 各セッション内のファイル一覧を返す。新しい順（降順）にソート。
pub fn list_backup_sessions(backup_dir: &Path) -> Result<Vec<AgentBackupSession>> {
    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();

    let entries = fs::read_dir(backup_dir)
        .with_context(|| format!("failed to read backup dir: {}", backup_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // タイムスタンプ形式のディレクトリ名のみ対象
        if crate::backup::extract_timestamp(&name_str).is_none() {
            continue;
        }

        let session_dir = entry.path();
        let files = collect_session_files(&session_dir, &session_dir)?;

        sessions.push(AgentBackupSession {
            session_id: name_str.into_owned(),
            files,
        });
    }

    // 新しい順（降順）でソート
    sessions.sort_by(|a, b| b.session_id.cmp(&a.session_id));

    Ok(sessions)
}

/// セッションディレクトリ内のファイルを再帰的に収集する。
fn collect_session_files(dir: &Path, session_root: &Path) -> Result<Vec<AgentBackupFile>> {
    let mut files = Vec::new();

    let entries =
        fs::read_dir(dir).with_context(|| format!("failed to read dir: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;

        // symlink は無視（バックアップdir内の symlink はセキュリティリスク）
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();

        if file_type.is_dir() {
            files.extend(collect_session_files(&path, session_root)?);
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(session_root)
                .map_err(|_| anyhow::anyhow!("failed to strip prefix"))?;
            let meta = fs::metadata(&path)
                .with_context(|| format!("failed to stat: {}", path.display()))?;
            files.push(AgentBackupFile {
                path: rel.to_string_lossy().into_owned(),
                size: meta.len(),
            });
        }
    }

    Ok(files)
}

/// バックアップからファイルを復元する。
///
/// `session_dir` 内のファイルを `root_dir` 配下にコピーする。
/// パストラバーサル検証を行い、個別ファイルの失敗は記録して続行する。
/// `metadata_config` が指定されている場合、復元先ファイルのメタデータを保持/適用する。
pub fn restore_backup(
    backup_dir: &Path,
    session_id: &str,
    files: &[String],
    root_dir: &Path,
    metadata_config: &MetadataConfig,
) -> Vec<AgentRestoreFileResult> {
    // session_id のパストラバーサル検証
    if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
        return vec![AgentRestoreFileResult {
            path: String::new(),
            success: false,
            error: Some(format!(
                "invalid session_id (path traversal detected): {}",
                session_id
            )),
        }];
    }
    // タイムスタンプ形式の検証
    if crate::backup::extract_timestamp(session_id).is_none() {
        return vec![AgentRestoreFileResult {
            path: String::new(),
            success: false,
            error: Some(format!("invalid session_id format: {}", session_id)),
        }];
    }

    let session_dir = backup_dir.join(session_id);

    files
        .iter()
        .map(|rel_path| restore_single_file(&session_dir, rel_path, root_dir, metadata_config))
        .collect()
}

/// 単一ファイルの復元を実行する。
///
/// 復元先に既存ファイルがある場合、そのメタデータ（owner/permissions）を保存し、
/// コピー後に復元する。
fn restore_single_file(
    session_dir: &Path,
    rel_path: &str,
    root_dir: &Path,
    metadata_config: &MetadataConfig,
) -> AgentRestoreFileResult {
    let make_error = |msg: String| AgentRestoreFileResult {
        path: rel_path.to_string(),
        success: false,
        error: Some(msg),
    };

    // パストラバーサル検証（復元先）
    let dest = match validate_path(root_dir, rel_path) {
        Ok(p) => p,
        Err(e) => return make_error(e.to_string()),
    };

    // パストラバーサル検証（バックアップ元）
    let src = match validate_path(session_dir, rel_path) {
        Ok(p) => p,
        Err(e) => return make_error(e.to_string()),
    };

    // バックアップ元の存在確認
    if !src.exists() {
        return make_error(format!("backup file not found: {}", src.display()));
    }

    // 復元先に既存ファイルがあればメタデータを保存
    let existing_metadata = SavedMetadata::from_path(&dest);

    // 復元先ディレクトリの自動作成
    if let Some(parent) = dest.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return make_error(format!(
                "failed to create directory {}: {e}",
                parent.display()
            ));
        }
    }

    // コピー実行
    match fs::copy(&src, &dest) {
        Ok(_) => {
            // メタデータを復元/適用
            match existing_metadata {
                Some(meta) => {
                    // 既存ファイルだった: 保存したメタデータで復元
                    let _ = apply_metadata(&dest, Some(meta.uid), Some(meta.gid), Some(meta.mode));
                }
                None => {
                    // 新規ファイルだった: config のデフォルト値で設定
                    let _ = apply_metadata(
                        &dest,
                        metadata_config.default_uid,
                        metadata_config.default_gid,
                        metadata_config.file_permissions,
                    );
                }
            }
            AgentRestoreFileResult {
                path: rel_path.to_string(),
                success: true,
                error: None,
            }
        }
        Err(e) => make_error(format!(
            "failed to copy {} -> {}: {e}",
            src.display(),
            dest.display()
        )),
    }
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

    // ── apply_metadata ──

    #[test]
    fn apply_metadata_sets_permissions() {
        let tmp = setup();
        let path = tmp.path().join("perm.txt");
        fs::write(&path, "data").unwrap();

        apply_metadata(&path, None, None, Some(0o644)).unwrap();

        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o644);
    }

    #[test]
    fn apply_metadata_sets_restrictive_permissions() {
        let tmp = setup();
        let path = tmp.path().join("secret.txt");
        fs::write(&path, "secret").unwrap();

        apply_metadata(&path, None, None, Some(0o600)).unwrap();

        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o600);
    }

    #[test]
    fn apply_metadata_skips_chown_when_uid_gid_none() {
        let tmp = setup();
        let path = tmp.path().join("no_chown.txt");
        fs::write(&path, "data").unwrap();

        let meta_before = fs::metadata(&path).unwrap();
        let uid_before = meta_before.uid();
        let gid_before = meta_before.gid();

        // uid/gid = None で呼び出し — chown は実行されない
        apply_metadata(&path, None, None, Some(0o644)).unwrap();

        let meta_after = fs::metadata(&path).unwrap();
        assert_eq!(meta_after.uid(), uid_before);
        assert_eq!(meta_after.gid(), gid_before);
    }

    #[test]
    fn apply_metadata_mode_none_does_not_change_permissions() {
        let tmp = setup();
        let path = tmp.path().join("keep_mode.txt");
        fs::write(&path, "data").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

        apply_metadata(&path, None, None, None).unwrap();

        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    }

    #[test]
    fn apply_metadata_chown_graceful_on_non_root() {
        // 非 root 環境では chown が失敗してもエラーにならないことを確認
        let tmp = setup();
        let path = tmp.path().join("chown_test.txt");
        fs::write(&path, "data").unwrap();

        // root でない場合、uid=0 への chown は失敗するはずだが、エラーにはならない
        let result = apply_metadata(&path, Some(0), Some(0), Some(0o644));
        assert!(result.is_ok());
    }

    #[test]
    fn apply_metadata_euid_nonroot_skips_chown() {
        // 非 root（通常のテスト環境）では chown がスキップされ、
        // uid/gid が変わらないことを確認
        if is_root() {
            // root で実行されている場合はスキップ
            return;
        }

        let tmp = setup();
        let path = tmp.path().join("skip_chown.txt");
        fs::write(&path, "data").unwrap();

        let meta_before = fs::metadata(&path).unwrap();

        // uid/gid を指定しても、非 root なので chown は実行されない
        apply_metadata(&path, Some(0), Some(0), None).unwrap();

        let meta_after = fs::metadata(&path).unwrap();
        assert_eq!(meta_after.uid(), meta_before.uid());
        assert_eq!(meta_after.gid(), meta_before.gid());
    }

    // ── write_file_with_metadata ──

    #[test]
    fn write_file_with_metadata_new_file_applies_default_permissions() {
        let tmp = setup();
        let config = MetadataConfig {
            file_permissions: Some(0o644),
            ..Default::default()
        };

        let saved = write_file_with_metadata(
            tmp.path(),
            "new.txt",
            b"hello",
            true, // is_first_chunk
            true, // is_last_chunk
            None,
            &config,
        )
        .unwrap();

        // 新規ファイルなので saved_metadata は None
        assert!(saved.is_none());

        let meta = fs::metadata(tmp.path().join("new.txt")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o644);
    }

    #[test]
    fn write_file_with_metadata_existing_file_restores_permissions() {
        let tmp = setup();
        let file_path = tmp.path().join("existing.txt");
        fs::write(&file_path, "old content").unwrap();
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755)).unwrap();

        let config = MetadataConfig {
            file_permissions: Some(0o644), // これは使われないはず
            ..Default::default()
        };

        let saved = write_file_with_metadata(
            tmp.path(),
            "existing.txt",
            b"new content",
            true, // is_first_chunk
            true, // is_last_chunk
            None,
            &config,
        )
        .unwrap();

        // 既存ファイルの metadata が保存されているはず
        assert!(saved.is_some());
        let saved = saved.unwrap();
        assert_eq!(saved.mode & 0o7777, 0o755);

        // パーミッションが復元されているはず（元の 0o755）
        let meta = fs::metadata(tmp.path().join("existing.txt")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    }

    #[test]
    fn write_file_with_metadata_chunked_transfer() {
        let tmp = setup();
        let config = MetadataConfig {
            file_permissions: Some(0o600),
            ..Default::default()
        };

        // 最初のチャンク（is_last_chunk=false なのでメタデータ適用されない）
        let saved = write_file_with_metadata(
            tmp.path(),
            "chunked.bin",
            b"chunk1",
            true,  // is_first_chunk
            false, // is_last_chunk
            None,
            &config,
        )
        .unwrap();
        assert!(saved.is_none()); // 新規ファイル

        // 2番目のチャンク（最終、メタデータ適用）
        write_file_with_metadata(
            tmp.path(),
            "chunked.bin",
            b"chunk2",
            false, // is_first_chunk
            true,  // is_last_chunk
            None,  // saved_metadata (新規なので None)
            &config,
        )
        .unwrap();

        let content = fs::read(tmp.path().join("chunked.bin")).unwrap();
        assert_eq!(content, b"chunk1chunk2");

        let meta = fs::metadata(tmp.path().join("chunked.bin")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o600);
    }

    #[test]
    fn write_file_with_metadata_new_dir_permissions() {
        let tmp = setup();
        let config = MetadataConfig {
            file_permissions: Some(0o644),
            dir_permissions: Some(0o755),
            ..Default::default()
        };

        write_file_with_metadata(
            tmp.path(),
            "newdir/subdir/file.txt",
            b"data",
            true,
            true,
            None,
            &config,
        )
        .unwrap();

        // 新規作成された中間ディレクトリ (newdir) にも dir_permissions が適用されている
        let parent_dir_meta = fs::metadata(tmp.path().join("newdir")).unwrap();
        assert_eq!(
            parent_dir_meta.permissions().mode() & 0o7777,
            0o755,
            "intermediate directory 'newdir' should have dir_permissions applied"
        );

        // 末端ディレクトリ (newdir/subdir) にも dir_permissions が適用されている
        let dir_meta = fs::metadata(tmp.path().join("newdir/subdir")).unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o7777, 0o755);

        // ファイルにも file_permissions が適用されている
        let file_meta = fs::metadata(tmp.path().join("newdir/subdir/file.txt")).unwrap();
        assert_eq!(file_meta.permissions().mode() & 0o7777, 0o644);
    }

    #[test]
    fn write_file_with_metadata_existing_intermediate_dir_not_changed() {
        let tmp = setup();
        // 既存の中間ディレクトリを先に作成（パーミッション 0o700）
        fs::create_dir(tmp.path().join("existing_parent")).unwrap();
        fs::set_permissions(
            tmp.path().join("existing_parent"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();

        let config = MetadataConfig {
            file_permissions: Some(0o644),
            dir_permissions: Some(0o755),
            ..Default::default()
        };

        write_file_with_metadata(
            tmp.path(),
            "existing_parent/newchild/file.txt",
            b"data",
            true,
            true,
            None,
            &config,
        )
        .unwrap();

        // 既存ディレクトリは変更されないこと
        let parent_meta = fs::metadata(tmp.path().join("existing_parent")).unwrap();
        assert_eq!(
            parent_meta.permissions().mode() & 0o7777,
            0o700,
            "existing directory should NOT have permissions changed"
        );

        // 新規作成された子ディレクトリには dir_permissions が適用されること
        let child_meta = fs::metadata(tmp.path().join("existing_parent/newchild")).unwrap();
        assert_eq!(
            child_meta.permissions().mode() & 0o7777,
            0o755,
            "newly created directory should have dir_permissions applied"
        );
    }

    // ── apply_dir_permissions ──

    #[test]
    fn apply_dir_permissions_sets_mode() {
        let tmp = setup();
        let dir = tmp.path().join("testdir");
        fs::create_dir(&dir).unwrap();

        apply_dir_permissions(&dir, Some(0o755)).unwrap();

        let meta = fs::metadata(&dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    }

    #[test]
    fn apply_dir_permissions_none_does_nothing() {
        let tmp = setup();
        let dir = tmp.path().join("testdir2");
        fs::create_dir(&dir).unwrap();

        let meta_before = fs::metadata(&dir).unwrap();
        let mode_before = meta_before.permissions().mode() & 0o7777;

        apply_dir_permissions(&dir, None).unwrap();

        let meta_after = fs::metadata(&dir).unwrap();
        assert_eq!(meta_after.permissions().mode() & 0o7777, mode_before);
    }

    // ── create_symlink_with_metadata ──

    #[test]
    fn symlink_with_metadata_creation() {
        let tmp = setup();
        fs::write(tmp.path().join("target.txt"), "link target").unwrap();

        create_symlink_with_metadata(tmp.path(), "meta_link.txt", "target.txt", None, None, None)
            .unwrap();

        let link = tmp.path().join("meta_link.txt");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&link).unwrap(), "link target");
    }

    // ── SavedMetadata ──

    #[test]
    fn saved_metadata_from_existing_file() {
        let tmp = setup();
        let path = tmp.path().join("meta_test.txt");
        fs::write(&path, "data").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        let saved = SavedMetadata::from_path(&path);
        assert!(saved.is_some());
        let saved = saved.unwrap();
        assert_eq!(saved.mode & 0o7777, 0o640);
    }

    #[test]
    fn saved_metadata_from_nonexistent_file() {
        let tmp = setup();
        let path = tmp.path().join("no_such_file.txt");
        let saved = SavedMetadata::from_path(&path);
        assert!(saved.is_none());
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

    // ── list_backup_sessions ──

    #[test]
    fn list_backup_sessions_returns_sessions() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");

        // セッションディレクトリを作成
        let s1 = backup_dir.join("20260311-100000");
        let s2 = backup_dir.join("20260311-120000");
        fs::create_dir_all(s1.join("sub")).unwrap();
        fs::write(s1.join("a.txt"), "aaa").unwrap();
        fs::write(s1.join("sub/b.txt"), "bbbbb").unwrap();
        fs::create_dir_all(&s2).unwrap();
        fs::write(s2.join("c.txt"), "cc").unwrap();

        // 非タイムスタンプディレクトリは無視されるべき
        fs::create_dir_all(backup_dir.join("not-a-session")).unwrap();

        let sessions = list_backup_sessions(&backup_dir).unwrap();
        assert_eq!(sessions.len(), 2);
        // 新しい順
        assert_eq!(sessions[0].session_id, "20260311-120000");
        assert_eq!(sessions[1].session_id, "20260311-100000");
        // ファイル一覧
        assert_eq!(sessions[0].files.len(), 1);
        assert_eq!(sessions[0].files[0].path, "c.txt");
        assert_eq!(sessions[0].files[0].size, 2);
        assert_eq!(sessions[1].files.len(), 2);
    }

    #[test]
    fn list_backup_sessions_empty_dir() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let sessions = list_backup_sessions(&backup_dir).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn list_backup_sessions_nonexistent_dir() {
        let tmp = setup();
        let backup_dir = tmp.path().join("no-such-dir");
        let sessions = list_backup_sessions(&backup_dir).unwrap();
        assert!(sessions.is_empty());
    }

    // ── restore_backup ──

    #[test]
    fn restore_backup_copies_files() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(session_dir.join("sub")).unwrap();
        fs::write(session_dir.join("a.txt"), "restored-a").unwrap();
        fs::write(session_dir.join("sub/b.txt"), "restored-b").unwrap();

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["a.txt".into(), "sub/b.txt".into()],
            &root_dir,
            &MetadataConfig::default(),
        );

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
        assert_eq!(
            fs::read_to_string(root_dir.join("a.txt")).unwrap(),
            "restored-a"
        );
        assert_eq!(
            fs::read_to_string(root_dir.join("sub/b.txt")).unwrap(),
            "restored-b"
        );
    }

    #[test]
    fn restore_backup_path_traversal_rejected() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(&session_dir).unwrap();

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["../etc/passwd".into()],
            &root_dir,
            &MetadataConfig::default(),
        );

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("path traversal"));
    }

    #[test]
    fn restore_backup_creates_parent_dirs() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(session_dir.join("deep/nested")).unwrap();
        fs::write(session_dir.join("deep/nested/file.txt"), "deep").unwrap();

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["deep/nested/file.txt".into()],
            &root_dir,
            &MetadataConfig::default(),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(
            fs::read_to_string(root_dir.join("deep/nested/file.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn restore_backup_partial_failure() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("exists.txt"), "ok").unwrap();
        // missing.txt は作成しない

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["exists.txt".into(), "missing.txt".into()],
            &root_dir,
            &MetadataConfig::default(),
        );

        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(!results[1].success);
        assert!(results[1]
            .error
            .as_ref()
            .unwrap()
            .contains("backup file not found"));
    }

    #[test]
    fn restore_backup_preserves_existing_file_metadata() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("a.txt"), "restored content").unwrap();

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        // 既存ファイルをパーミッション 0o755 で作成
        fs::write(root_dir.join("a.txt"), "old content").unwrap();
        fs::set_permissions(root_dir.join("a.txt"), fs::Permissions::from_mode(0o755)).unwrap();

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["a.txt".into()],
            &root_dir,
            &MetadataConfig::default(),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        // コンテンツが復元されていること
        assert_eq!(
            fs::read_to_string(root_dir.join("a.txt")).unwrap(),
            "restored content"
        );
        // 既存ファイルのパーミッションが保持されていること
        let meta = fs::metadata(root_dir.join("a.txt")).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o7777,
            0o755,
            "existing file permissions should be preserved after restore"
        );
    }

    #[test]
    fn restore_backup_new_file_applies_config_permissions() {
        let tmp = setup();
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("new.txt"), "new content").unwrap();

        let root_dir = tmp.path().join("root");
        fs::create_dir_all(&root_dir).unwrap();

        let config = MetadataConfig {
            file_permissions: Some(0o600),
            ..Default::default()
        };

        let results = restore_backup(
            &backup_dir,
            "20260311-120000",
            &["new.txt".into()],
            &root_dir,
            &config,
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        // 新規ファイルには config のデフォルトパーミッションが適用されること
        let meta = fs::metadata(root_dir.join("new.txt")).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o7777,
            0o600,
            "new file should get config default permissions"
        );
    }

    // ── create_symlink_with_metadata dir_permissions ──

    #[test]
    fn symlink_with_metadata_applies_dir_permissions() {
        let tmp = setup();
        fs::write(tmp.path().join("target.txt"), "link target").unwrap();

        create_symlink_with_metadata(
            tmp.path(),
            "newparent/nested/link.txt",
            "target.txt",
            None,
            None,
            Some(0o755),
        )
        .unwrap();

        // 新規作成されたディレクトリに dir_permissions が適用されている
        let parent_meta = fs::metadata(tmp.path().join("newparent")).unwrap();
        assert_eq!(
            parent_meta.permissions().mode() & 0o7777,
            0o755,
            "intermediate directory should have dir_permissions applied"
        );
        let nested_meta = fs::metadata(tmp.path().join("newparent/nested")).unwrap();
        assert_eq!(
            nested_meta.permissions().mode() & 0o7777,
            0o755,
            "nested directory should have dir_permissions applied"
        );

        // symlink が正しく作成されていること
        let link = tmp.path().join("newparent/nested/link.txt");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }
}
