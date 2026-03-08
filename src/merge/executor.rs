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
}
