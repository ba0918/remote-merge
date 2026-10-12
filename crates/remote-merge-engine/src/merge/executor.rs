//! マージ実行ロジック。
//! LeftToRight（左 → 右）と RightToLeft（右 → 左）を担当する。

use std::path::Path;

use remote_merge_core::relative_path::RelPath;

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

/// 中身を読むファイルの大きさの上限（docs/ir/cli/size-limit.md の REQ-cli-079）。判定は製品の
/// 読み込みの入口が、読む前に大きさで行う
pub const MAX_BINARY_FILE_SIZE: usize = 104_857_600;

/// ローカルファイルのバイト列を読み込む（バイナリファイル対応）
///
/// UTF-8 変換を行わず、生のバイト列をそのまま返す。
pub fn read_local_file_bytes(root_dir: &Path, rel_path: &RelPath) -> crate::error::Result<Vec<u8>> {
    let normalized = root_dir.join(rel_path.as_str());

    let bytes = std::fs::read(&normalized).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => crate::error::AppError::PathNotFound {
            path: normalized.clone(),
        },
        _ => crate::error::AppError::Io(e),
    })?;

    Ok(bytes)
}

/// ローカルファイルにバイト列を書き込む（バイナリファイル対応）
///
/// 途中の symlink は root_dir の外を指していても辿る。
pub fn write_local_file_bytes(
    root_dir: &Path,
    rel_path: &RelPath,
    content: &[u8],
) -> crate::error::Result<()> {
    let normalized = root_dir.join(rel_path.as_str());

    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent)?;
    }

    write_without_following_link(&normalized, content)?;
    tracing::info!(target: "remote_merge::merge::executor", "Local file written (bytes): {}", normalized.display());
    Ok(())
}

/// ローカルファイルに書き込む（RemoteToLocal で使用）
///
/// 途中の symlink は root_dir の外を指していても辿る
pub fn write_local_file(
    root_dir: &Path,
    rel_path: &RelPath,
    content: &str,
) -> crate::error::Result<()> {
    let normalized = root_dir.join(rel_path.as_str());

    // 親ディレクトリを作成（存在しなければ）
    if let Some(parent) = normalized.parent() {
        std::fs::create_dir_all(parent)?;
    }

    write_without_following_link(&normalized, content.as_bytes())?;
    tracing::info!(target: "remote_merge::merge::executor", "Local file written: {}", normalized.display());
    Ok(())
}

/// `std::fs::write` と同じく作成・切り詰めて書くが、最後の要素が symlink なら辿らずに失敗する。
///
/// `std::fs::write` はリンク先を書き換えるため、確かめてから書く間の差し替えも含めて
/// O_NOFOLLOW で開く。途中の要素の symlink は O_NOFOLLOW の対象外なので今までどおり辿る。
#[cfg(unix)]
fn write_without_following_link(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .write_all(content)
}

/// O_NOFOLLOW のない環境では今までどおり書く
#[cfg(not(unix))]
fn write_without_following_link(path: &Path, content: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, content)
}

/// root_dir からの相対パスを remote_root の下のパスにする。root_dir 全体は remote_root に "/" を
/// 付けたものになる
pub fn remote_full_path(remote_root: &str, rel_path: &RelPath) -> String {
    format!(
        "{}/{}",
        remote_root.trim_end_matches('/'),
        rel_path.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(path: &str) -> RelPath {
        RelPath::parse(path).unwrap()
    }
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
        write_local_file(dir.path(), &rel("test.txt"), "hello world").unwrap();

        let content = std::fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_write_local_file_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        write_local_file(dir.path(), &rel("a/b/c/test.txt"), "nested").unwrap();

        let content = std::fs::read_to_string(dir.path().join("a/b/c/test.txt")).unwrap();
        assert_eq!(content, "nested");
    }

    #[test]
    fn test_write_local_file_overwrite() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("existing.txt"), "old content").unwrap();

        write_local_file(dir.path(), &rel("existing.txt"), "new content").unwrap();

        let content = std::fs::read_to_string(dir.path().join("existing.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_remote_full_path() {
        assert_eq!(
            remote_full_path("/var/www/app", &rel("src/index.html")),
            "/var/www/app/src/index.html"
        );
        assert_eq!(remote_full_path("/var/www/app", &rel("")), "/var/www/app/");
    }

    #[test]
    fn test_remote_full_path_trailing_slash() {
        let result = remote_full_path("/var/www/app/", &rel("src/index.html"));
        assert_eq!(result, "/var/www/app/src/index.html");
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

        let result = read_local_file_bytes(dir.path(), &rel("binary.dat")).unwrap();
        assert_eq!(result, binary_data, "Binary data should be returned as-is");
    }

    #[test]
    fn test_write_local_file_bytes_binary() {
        let dir = TempDir::new().unwrap();
        let binary_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD, 0x00, 0xAB];

        write_local_file_bytes(dir.path(), &rel("output.bin"), &binary_data).unwrap();

        let written = std::fs::read(dir.path().join("output.bin")).unwrap();
        assert_eq!(written, binary_data, "Written bytes should match exactly");
    }

    #[test]
    fn test_read_file_bytes_text() {
        let dir = TempDir::new().unwrap();
        let text = "Hello, world!\nThis is a text file.\n";
        std::fs::write(dir.path().join("text.txt"), text).unwrap();

        let result = read_local_file_bytes(dir.path(), &rel("text.txt")).unwrap();
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

        write_local_file_bytes(dir.path(), &rel("text_out.txt"), text.as_bytes()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("text_out.txt")).unwrap();
        assert_eq!(content, text);
    }

    #[test]
    fn test_write_local_file_bytes_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let data = vec![0x01, 0x02, 0x03];

        write_local_file_bytes(dir.path(), &rel("a/b/c/nested.bin"), &data).unwrap();

        let written = std::fs::read(dir.path().join("a/b/c/nested.bin")).unwrap();
        assert_eq!(written, data);
    }

    #[test]
    fn test_read_local_file_bytes_not_found() {
        let dir = TempDir::new().unwrap();
        let result = read_local_file_bytes(dir.path(), &rel("nonexistent.bin"));
        let err = result.unwrap_err();
        let app_err = err.downcast_ref::<crate::error::AppError>();
        assert!(
            matches!(app_err, Some(crate::error::AppError::PathNotFound { .. })),
            "Expected PathNotFound, got: {}",
            err
        );
    }

    /// `real.txt` を指す symlink `link.txt` を置いた一時ディレクトリ
    #[cfg(unix)]
    fn root_with_link_to_real_file() -> TempDir {
        let root = TempDir::new().unwrap();
        std::fs::write(root.path().join("real.txt"), "original").unwrap();
        std::os::unix::fs::symlink("real.txt", root.path().join("link.txt")).unwrap();
        root
    }

    #[cfg(unix)]
    fn assert_link_and_target_unchanged(root: &TempDir) {
        assert_eq!(
            std::fs::read_link(root.path().join("link.txt")).unwrap(),
            Path::new("real.txt")
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("real.txt")).unwrap(),
            "original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn binary_write_refuses_a_symlink_destination() {
        let root = root_with_link_to_real_file();

        let result = write_local_file_bytes(root.path(), &rel("link.txt"), b"new");

        assert!(result.is_err());
        assert_link_and_target_unchanged(&root);
    }

    #[cfg(unix)]
    #[test]
    fn text_write_refuses_a_symlink_destination() {
        let root = root_with_link_to_real_file();

        let result = write_local_file(root.path(), &rel("link.txt"), "new");

        assert!(result.is_err());
        assert_link_and_target_unchanged(&root);
    }
}
