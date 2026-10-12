//! root_dir からの相対パスの型と、入力の形の検査。
//!
//! root_dir からの相対パスとして受け取る値は、コマンドの入口とエージェントの受け取り口で
//! [`RelPath::parse`] を通し、経路の操作には [`RelPath`] で渡す。ここで見るのは入力の形
//! （絶対パスと ".." の要素）だけで、パスを辿って root_dir の外かどうかは判定しない。

/// 入力の形の検査で拒んだ理由。文言には入力されたままの値を出す
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum RelPathError {
    #[error("absolute subpath not allowed: {0}")]
    Absolute(String),
    #[error("path traversal not allowed: {0}")]
    Traversal(String),
}

/// 入力の形の検査を通った、root_dir からの相対パス
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath {
    /// 入力されたままの値
    typed: String,
}

impl RelPath {
    /// 絶対パスと ".." の要素を拒む。"/" だけからなる値は root_dir 全体として受け付ける
    pub fn parse(input: &str) -> Result<Self, RelPathError> {
        if input.starts_with('/') && !input.trim_end_matches('/').is_empty() {
            return Err(RelPathError::Absolute(input.to_string()));
        }
        if input.split('/').any(|element| element == "..") {
            return Err(RelPathError::Traversal(input.to_string()));
        }
        Ok(Self {
            typed: input.to_string(),
        })
    }

    /// root_dir 全体を指すとき（"/"・"."・"./"・空の値）は空、それ以外は末尾の "/" を除いた値
    pub fn as_str(&self) -> &str {
        if self.is_root() {
            ""
        } else {
            self.typed.trim_end_matches('/')
        }
    }

    pub fn as_typed(&self) -> &str {
        &self.typed
    }

    pub fn is_root(&self) -> bool {
        let stripped = self.typed.trim_end_matches('/');
        stripped.is_empty() || stripped == "."
    }

    /// 末尾の "/" を除いた値で入力されたものとみなした RelPath。"/" だけからなる値と空の値は
    /// そのまま残す
    pub fn without_trailing_slash(&self) -> RelPath {
        let stripped = self.typed.trim_end_matches('/');
        if stripped.is_empty() {
            self.clone()
        } else {
            RelPath {
                typed: stripped.to_string(),
            }
        }
    }

    /// root_dir 全体を指す値を除き、末尾に "/" を付けて入力されたか
    pub fn has_trailing_slash(&self) -> bool {
        !self.is_root() && self.typed.ends_with('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> RelPath {
        RelPath::parse(input).unwrap()
    }

    fn refusal(input: &str) -> String {
        RelPath::parse(input).unwrap_err().to_string()
    }

    #[test]
    fn a_plain_path_is_kept_as_it_was_typed() {
        let path = parsed("src/app.rs");
        assert_eq!(path.as_str(), "src/app.rs");
        assert_eq!(path.as_typed(), "src/app.rs");
        assert!(!path.is_root());
        assert!(!path.has_trailing_slash());
    }

    #[test]
    fn an_absolute_path_is_refused_with_the_path_as_typed() {
        assert_eq!(
            refusal("/etc/passwd"),
            "absolute subpath not allowed: /etc/passwd"
        );
        assert_eq!(refusal("/a/"), "absolute subpath not allowed: /a/");
        assert_eq!(refusal("/."), "absolute subpath not allowed: /.");
    }

    #[test]
    fn a_parent_element_is_refused_with_the_path_as_typed() {
        for input in [
            "../b.txt",
            "a/../b",
            "a/..",
            "src/../",
            "..",
            "./../x",
            "../etc/passwd",
            "src/..",
            "src/../etc/passwd",
        ] {
            assert_eq!(
                refusal(input),
                format!("path traversal not allowed: {input}"),
                "{input}"
            );
        }
    }

    #[test]
    fn an_absolute_path_with_a_parent_element_is_refused_as_absolute() {
        assert_eq!(refusal("/a/../b"), "absolute subpath not allowed: /a/../b");
    }

    #[test]
    fn names_that_only_contain_dots_are_not_parent_elements() {
        for input in ["a..b", "..a", "a/...", ".hidden", "a./b"] {
            assert_eq!(parsed(input).as_str(), input, "{input}");
        }
    }

    #[test]
    fn slash_dot_dot_slash_and_empty_name_the_whole_root_dir() {
        for input in ["/", "//", ".", "./", ""] {
            let path = parsed(input);
            assert!(path.is_root(), "{input}");
            assert_eq!(path.as_str(), "", "{input}");
            assert_eq!(path.as_typed(), input, "{input}");
            assert!(!path.has_trailing_slash(), "{input}");
        }
    }

    #[test]
    fn a_trailing_slash_names_the_same_path_and_is_remembered() {
        let plain = parsed("src");
        let slash = parsed("src/");
        assert_eq!(slash.as_str(), plain.as_str());
        assert!(slash.has_trailing_slash());
        assert!(!plain.has_trailing_slash());
        assert_eq!(slash.as_typed(), "src/");
        assert_eq!(parsed("a/b//").as_str(), "a/b");
    }

    #[test]
    fn dropping_the_trailing_slash_keeps_the_whole_root_dir_as_typed() {
        assert_eq!(parsed("src/").without_trailing_slash().as_typed(), "src");
        assert_eq!(parsed("src").without_trailing_slash().as_typed(), "src");
        assert_eq!(parsed("./").without_trailing_slash().as_typed(), ".");
        for input in ["/", "//", ".", ""] {
            assert_eq!(parsed(input).without_trailing_slash().as_typed(), input);
        }
    }

    #[test]
    fn dot_elements_inside_a_path_are_left_for_path_resolution() {
        assert_eq!(parsed("./src/app.rs").as_str(), "./src/app.rs");
        assert_eq!(parsed("a/./b").as_str(), "a/./b");
        assert!(!parsed("./src").is_root());
    }

    #[test]
    fn a_checked_path_is_checked_again_with_the_same_result() {
        let path = parsed("src/");
        assert_eq!(RelPath::parse(path.as_str()).unwrap().as_str(), "src");
    }
}
