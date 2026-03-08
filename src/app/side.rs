//! 比較の「片側」を表す型。
//! ローカル or リモート（サーバ名付き）で比較元/先を抽象化する。

/// 比較の片側がどこから来るか
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Side {
    /// ローカルファイルシステム
    Local,
    /// リモートサーバ（サーバ名）
    Remote(String),
}

impl Side {
    /// サーバ名から Side を構築する。"local" なら Side::Local を返す。
    pub fn new(name: &str) -> Self {
        if name == "local" {
            Side::Local
        } else {
            Side::Remote(name.to_string())
        }
    }

    /// 表示用の名前を返す
    pub fn display_name(&self) -> &str {
        match self {
            Side::Local => "local",
            Side::Remote(name) => name.as_str(),
        }
    }

    /// ローカルかどうか
    pub fn is_local(&self) -> bool {
        matches!(self, Side::Local)
    }

    /// リモートかどうか
    pub fn is_remote(&self) -> bool {
        matches!(self, Side::Remote(_))
    }

    /// リモートの場合、サーバ名を返す
    pub fn server_name(&self) -> Option<&str> {
        match self {
            Side::Local => None,
            Side::Remote(name) => Some(name.as_str()),
        }
    }
}

/// 両サイドがリモート同士かどうか
pub fn is_remote_to_remote(left: &Side, right: &Side) -> bool {
    left.is_remote() && right.is_remote()
}

/// 比較モードの表示ラベルを生成する（例: "local <-> develop"）
pub fn comparison_label(left: &Side, right: &Side) -> String {
    format!("{} <-> {}", left.display_name(), right.display_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_local() {
        assert_eq!(Side::Local.display_name(), "local");
    }

    #[test]
    fn display_name_remote() {
        let side = Side::Remote("develop".to_string());
        assert_eq!(side.display_name(), "develop");
    }

    #[test]
    fn is_local_returns_true_for_local() {
        assert!(Side::Local.is_local());
        assert!(!Side::Remote("staging".to_string()).is_local());
    }

    #[test]
    fn is_remote_returns_true_for_remote() {
        assert!(Side::Remote("staging".to_string()).is_remote());
        assert!(!Side::Local.is_remote());
    }

    #[test]
    fn server_name_returns_name_for_remote() {
        assert_eq!(Side::Local.server_name(), None);
        assert_eq!(
            Side::Remote("develop".to_string()).server_name(),
            Some("develop")
        );
    }

    #[test]
    fn is_remote_to_remote_both_remote() {
        let left = Side::Remote("develop".to_string());
        let right = Side::Remote("staging".to_string());
        assert!(is_remote_to_remote(&left, &right));
    }

    #[test]
    fn is_remote_to_remote_local_and_remote() {
        let left = Side::Local;
        let right = Side::Remote("develop".to_string());
        assert!(!is_remote_to_remote(&left, &right));
    }

    #[test]
    fn is_remote_to_remote_both_local() {
        assert!(!is_remote_to_remote(&Side::Local, &Side::Local));
    }

    #[test]
    fn comparison_label_local_remote() {
        let left = Side::Local;
        let right = Side::Remote("develop".to_string());
        assert_eq!(comparison_label(&left, &right), "local <-> develop");
    }

    #[test]
    fn comparison_label_remote_remote() {
        let left = Side::Remote("develop".to_string());
        let right = Side::Remote("staging".to_string());
        assert_eq!(comparison_label(&left, &right), "develop <-> staging");
    }

    #[test]
    fn side_new_local() {
        assert_eq!(Side::new("local"), Side::Local);
    }

    #[test]
    fn side_new_remote() {
        assert_eq!(Side::new("develop"), Side::Remote("develop".to_string()));
    }

    #[test]
    fn side_new_case_sensitive() {
        // 大文字小文字は区別する — "LOCAL" は Remote として扱われる
        assert_eq!(Side::new("LOCAL"), Side::Remote("LOCAL".to_string()));
    }

    #[test]
    fn side_new_empty_string() {
        // 空文字のバリデーションは上位層の責務
        assert_eq!(Side::new(""), Side::Remote("".to_string()));
    }

    #[test]
    fn side_equality() {
        assert_eq!(Side::Local, Side::Local);
        assert_eq!(Side::Remote("a".to_string()), Side::Remote("a".to_string()));
        assert_ne!(Side::Local, Side::Remote("a".to_string()));
        assert_ne!(Side::Remote("a".to_string()), Side::Remote("b".to_string()));
    }
}
