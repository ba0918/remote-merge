//! マージ実行ロジック。
//! LeftToRight（左 → 右）と RightToLeft（右 → 左）を担当する。

use std::path::{Path, PathBuf};

/// マージの方向
///
/// - Left = 左パネル
/// - Right = 右パネル
///
/// `LeftToRight` は左側の内容を右側に適用（上書き）する方向を表し、
/// `RightToLeft` は右側の内容を左側に適用（上書き）する方向を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeDirection {
    /// 左 → 右（左側の内容で右側を上書き）
    LeftToRight,
    /// 右 → 左（右側の内容で左側を上書き）
    RightToLeft,
}

impl MergeDirection {
    /// JSON / テキスト出力用の文字列表現を返す。
    pub fn as_str(self) -> &'static str {
        match self {
            MergeDirection::LeftToRight => "left_to_right",
            MergeDirection::RightToLeft => "right_to_left",
        }
    }

    /// 対応する `HunkDirection` に変換する。
    pub fn to_hunk_direction(self) -> crate::diff::engine::HunkDirection {
        match self {
            MergeDirection::LeftToRight => crate::diff::engine::HunkDirection::LeftToRight,
            MergeDirection::RightToLeft => crate::diff::engine::HunkDirection::RightToLeft,
        }
    }
}

impl MergeDirection {
    /// 方向を表す矢印文字列
    pub fn arrow(&self) -> &'static str {
        match self {
            MergeDirection::LeftToRight => "→",
            MergeDirection::RightToLeft => "←",
        }
    }

    /// 表示用の説明文
    pub fn description(&self, left: &str, right: &str) -> String {
        match self {
            MergeDirection::LeftToRight => format!("{} → {}", left, right),
            MergeDirection::RightToLeft => format!("{} → {}", right, left),
        }
    }
}

/// マージ時のオプション
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    /// マージ時にソースファイルのパーミッションもコピーするか
    pub with_permissions: bool,
}

/// バイナリファイルの最大サイズ（100MB）
pub const MAX_BINARY_FILE_SIZE: usize = 104_857_600;

/// ローカルファイルのバイト列を読み込む（バイナリファイル対応）
///
/// UTF-8 変換を行わず、生のバイト列をそのまま返す。
/// 100MB 超のファイルは `force` が false の場合エラーを返す。
pub fn read_local_file_bytes(
    root_dir: &Path,
    rel_path: &str,
    force: bool,
) -> crate::error::Result<Vec<u8>> {
    let full_path = root_dir.join(rel_path);
    let normalized = validate_path_within_root(root_dir, &full_path)?;

    let bytes = std::fs::read(&normalized).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => crate::error::AppError::PathNotFound {
            path: normalized.clone(),
        },
        _ => crate::error::AppError::Io(e),
    })?;

    if !force && bytes.len() > MAX_BINARY_FILE_SIZE {
        anyhow::bail!(
            "File too large ({} bytes > {} bytes limit): {}. Use --force to override.",
            bytes.len(),
            MAX_BINARY_FILE_SIZE,
            normalized.display()
        );
    }

    Ok(bytes)
}

/// ローカルファイルにバイト列を書き込む（バイナリファイル対応）
///
/// root_dir 配下であることを検証してから書き込む。
pub fn write_local_file_bytes(
    root_dir: &Path,
    rel_path: &str,
    content: &[u8],
) -> crate::error::Result<()> {
    let full_path = root_dir.join(rel_path);
    let normalized = validate_path_within_root(root_dir, &full_path)?;

    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&normalized, content)?;
    tracing::info!("Local file written (bytes): {}", normalized.display());
    Ok(())
}

/// ローカルファイルに書き込む（RemoteToLocal で使用）
///
/// root_dir 配下であることを検証してから書き込む
pub fn write_local_file(
    root_dir: &Path,
    rel_path: &str,
    content: &str,
) -> crate::error::Result<()> {
    let full_path = root_dir.join(rel_path);

    // セキュリティ: root_dir 配下であることを検証
    let normalized = validate_path_within_root(root_dir, &full_path)?;

    // 親ディレクトリを作成（存在しなければ）
    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&normalized, content)?;
    tracing::info!("Local file written: {}", normalized.display());
    Ok(())
}

/// ローカルファイルの内容を読み込む
///
/// バイナリファイル（UTF-8でないファイル）の場合は lossy 変換して返す。
/// これにより diff エンジンの `is_binary()` でバイナリ判定できるようになる。
pub fn read_local_file(root_dir: &Path, rel_path: &str) -> crate::error::Result<String> {
    let full_path = root_dir.join(rel_path);
    let normalized = validate_path_within_root(root_dir, &full_path)?;

    let bytes = std::fs::read(&normalized).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => crate::error::AppError::PathNotFound {
            path: normalized.clone(),
        },
        _ => crate::error::AppError::Io(e),
    })?;

    // UTF-8 として読めればそのまま返す。
    // バイナリの場合は lossy 変換して返し、diff エンジンの is_binary() に判定を委ねる。
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}

/// パスが root_dir 配下にあることを検証する
///
/// `..` やシンボリックリンク経由のエスケープを防止する。
/// 検証に成功した場合、正規化済みのフルパスを返す。
pub(crate) fn validate_path_within_root(
    root_dir: &Path,
    full_path: &Path,
) -> crate::error::Result<PathBuf> {
    // canonicalize が使えない（ファイルが存在しない可能性）ので
    // コンポーネントベースで検証する
    let normalized = normalize_path(full_path);
    let root_normalized = normalize_path(root_dir);

    if !normalized.starts_with(&root_normalized) {
        anyhow::bail!(crate::error::AppError::ConfigValidation {
            field: "path".to_string(),
            message: format!(
                "Path escapes root_dir: {} (root: {})",
                full_path.display(),
                root_dir.display()
            ),
        });
    }
    Ok(normalized)
}

/// パスの `..` コンポーネントを解決して正規化する（ファイル存在不要）
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

/// リモートパスのサニタイズ（`..` 等の危険なコンポーネントを検出）
pub fn validate_remote_path(remote_root: &str, rel_path: &str) -> crate::error::Result<String> {
    // rel_path に `..` が含まれていないことを確認
    let path = Path::new(rel_path);
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            anyhow::bail!(crate::error::AppError::ConfigValidation {
                field: "remote_path".to_string(),
                message: format!("Remote path must not contain '..': {}", rel_path),
            });
        }
    }

    // remote_root + rel_path を結合
    let full_path = format!(
        "{}/{}",
        remote_root.trim_end_matches('/'),
        rel_path.trim_start_matches('/')
    );
    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_merge_direction_description() {
        let dir = MergeDirection::LeftToRight;
        assert_eq!(dir.description("local", "develop"), "local → develop");

        let dir = MergeDirection::RightToLeft;
        assert_eq!(dir.description("local", "develop"), "develop → local");
    }

    #[test]
    fn test_write_local_file() {
        let dir = TempDir::new().unwrap();
        write_local_file(dir.path(), "test.txt", "hello world").unwrap();

        let content = std::fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_write_local_file_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        write_local_file(dir.path(), "a/b/c/test.txt", "nested").unwrap();

        let content = std::fs::read_to_string(dir.path().join("a/b/c/test.txt")).unwrap();
        assert_eq!(content, "nested");
    }

    #[test]
    fn test_write_local_file_overwrite() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("existing.txt"), "old content").unwrap();

        write_local_file(dir.path(), "existing.txt", "new content").unwrap();

        let content = std::fs::read_to_string(dir.path().join("existing.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_read_local_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();

        let content = read_local_file(dir.path(), "test.txt").unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_read_local_file_not_found() {
        let dir = TempDir::new().unwrap();
        let result = read_local_file(dir.path(), "nonexistent.txt");
        assert!(result.is_err());
        // PathNotFound エラーであることを確認
        let err = result.unwrap_err();
        let app_err = err.downcast_ref::<crate::error::AppError>();
        assert!(
            matches!(app_err, Some(crate::error::AppError::PathNotFound { .. })),
            "Expected PathNotFound, got: {}",
            err
        );
    }

    #[test]
    fn test_read_local_file_binary() {
        let dir = TempDir::new().unwrap();
        // NULバイトを含むバイナリファイルを作成
        let binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0xFF, 0xFE, 0xFD];
        std::fs::write(dir.path().join("binary.dat"), &binary_data).unwrap();

        // バイナリファイルでもエラーにならず、lossy 変換された文字列が返る
        let result = read_local_file(dir.path(), "binary.dat");
        assert!(result.is_ok(), "Binary file should not cause error");
        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_validate_path_within_root() {
        let root = Path::new("/home/user/app");

        // 正常パス: 正規化済みパスが返る
        let result =
            validate_path_within_root(root, Path::new("/home/user/app/src/main.rs")).unwrap();
        assert_eq!(result, PathBuf::from("/home/user/app/src/main.rs"));

        // パストラバーサル: エラー
        assert!(
            validate_path_within_root(root, Path::new("/home/user/app/../../../etc/passwd"))
                .is_err()
        );
    }

    #[test]
    fn test_validate_remote_path() {
        let result = validate_remote_path("/var/www/app", "src/index.html");
        assert_eq!(result.unwrap(), "/var/www/app/src/index.html");

        let result = validate_remote_path("/var/www/app", "../../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_remote_path_trailing_slash() {
        let result = validate_remote_path("/var/www/app/", "src/index.html");
        assert_eq!(result.unwrap(), "/var/www/app/src/index.html");
    }

    #[test]
    fn test_merge_direction_arrow() {
        assert_eq!(MergeDirection::LeftToRight.arrow(), "→");
        assert_eq!(MergeDirection::RightToLeft.arrow(), "←");
    }

    // ── バイト列版テスト ──

    #[test]
    fn test_read_local_file_bytes_binary() {
        let dir = TempDir::new().unwrap();
        // NUL バイトを含むバイナリデータ
        let binary_data: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0xFF, 0xFE, 0xFD];
        std::fs::write(dir.path().join("binary.dat"), &binary_data).unwrap();

        let result = read_local_file_bytes(dir.path(), "binary.dat", false).unwrap();
        assert_eq!(result, binary_data, "Binary data should be returned as-is");
    }

    #[test]
    fn test_write_local_file_bytes_binary() {
        let dir = TempDir::new().unwrap();
        let binary_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD, 0x00, 0xAB];

        write_local_file_bytes(dir.path(), "output.bin", &binary_data).unwrap();

        let written = std::fs::read(dir.path().join("output.bin")).unwrap();
        assert_eq!(written, binary_data, "Written bytes should match exactly");
    }

    #[test]
    fn test_bytes_roundtrip_sha256() {
        use sha2::{Digest, Sha256};

        let dir = TempDir::new().unwrap();
        // PNG ヘッダ風のバイナリデータ + NUL バイト
        let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();

        // 書き込み → 読み込みのラウンドトリップ
        write_local_file_bytes(dir.path(), "roundtrip.bin", &binary_data).unwrap();
        let read_back = read_local_file_bytes(dir.path(), "roundtrip.bin", false).unwrap();

        let original_hash = Sha256::digest(&binary_data);
        let readback_hash = Sha256::digest(&read_back);
        assert_eq!(
            original_hash, readback_hash,
            "SHA-256 should match after roundtrip"
        );
    }

    #[test]
    fn test_read_file_bytes_text() {
        let dir = TempDir::new().unwrap();
        let text = "Hello, world!\nThis is a text file.\n";
        std::fs::write(dir.path().join("text.txt"), text).unwrap();

        let result = read_local_file_bytes(dir.path(), "text.txt", false).unwrap();
        assert_eq!(
            result,
            text.as_bytes(),
            "Text files should also work with bytes API"
        );
    }

    #[test]
    fn test_write_file_bytes_text() {
        let dir = TempDir::new().unwrap();
        let text = "Text content via bytes API\n";

        write_local_file_bytes(dir.path(), "text_out.txt", text.as_bytes()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("text_out.txt")).unwrap();
        assert_eq!(content, text);
    }

    #[test]
    fn test_read_local_file_bytes_size_limit() {
        let dir = TempDir::new().unwrap();
        // 100MB + 1 バイトのデータを作成
        let large_data = vec![0xABu8; MAX_BINARY_FILE_SIZE + 1];
        std::fs::write(dir.path().join("huge.bin"), &large_data).unwrap();

        // force=false: エラーになること
        let result = read_local_file_bytes(dir.path(), "huge.bin", false);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("File too large"),
            "Expected 'File too large' error, got: {}",
            err
        );
        assert!(err.contains("--force"), "Error should mention --force");

        // force=true: 成功すること
        let result = read_local_file_bytes(dir.path(), "huge.bin", true);
        assert!(result.is_ok(), "force=true should bypass size limit");
        assert_eq!(result.unwrap().len(), MAX_BINARY_FILE_SIZE + 1);
    }

    #[test]
    fn test_write_local_file_bytes_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let data = vec![0x01, 0x02, 0x03];

        write_local_file_bytes(dir.path(), "a/b/c/nested.bin", &data).unwrap();

        let written = std::fs::read(dir.path().join("a/b/c/nested.bin")).unwrap();
        assert_eq!(written, data);
    }

    #[test]
    fn test_read_local_file_bytes_not_found() {
        let dir = TempDir::new().unwrap();
        let result = read_local_file_bytes(dir.path(), "nonexistent.bin", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_local_file_bytes_path_traversal() {
        let dir = TempDir::new().unwrap();
        let result = read_local_file_bytes(dir.path(), "../../../etc/passwd", false);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Path escapes root_dir"));
    }
}
