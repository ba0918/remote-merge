//! マージ実行ロジック。
//! LeftToRight（左 → 右）と RightToLeft（右 → 左）を担当する。

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

    #[test]
    fn test_merge_direction_description() {
        let dir = MergeDirection::LeftToRight;
        assert_eq!(dir.description("local", "develop"), "local → develop");

        let dir = MergeDirection::RightToLeft;
        assert_eq!(dir.description("local", "develop"), "develop → local");
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
}
