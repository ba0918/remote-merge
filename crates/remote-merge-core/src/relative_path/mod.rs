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

    /// root_dir そのもの（空のパス）。入力を受け取らないので検査は要らない
    pub fn root() -> Self {
        Self {
            typed: String::new(),
        }
    }

    /// root_dir 全体を指すとき（"/"・"."・"./"・空の値）は空、それ以外は末尾の "/" を除いた値
    pub fn as_str(&self) -> &str {
        if self.is_root() {
            ""
        } else {
            self.typed.trim_end_matches('/')
        }
    }

    /// パスの確認に渡す値。末尾の "/" はディレクトリを求める意味を持つので残し（REQ-paths-005）、
    /// root_dir 全体は空のパスにする
    pub fn path_to_resolve(&self) -> &str {
        if self.is_root() {
            ""
        } else {
            &self.typed
        }
    }

    /// 親（root_dir 直下なら root_dir 全体）と最後の名前。末尾の "/" は除いて分ける
    pub fn parent_and_name(&self) -> (RelPath, &str) {
        match self.as_str().rsplit_once('/') {
            Some((parent, name)) => (
                RelPath {
                    typed: parent.to_string(),
                },
                name,
            ),
            None => (RelPath::root(), self.as_str()),
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

/// RelPath の並びを、入力されたままの値の並びにする（通信で送る値や結果のキー）
pub fn as_typed_all(paths: &[RelPath]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.as_typed().to_string())
        .collect()
}

#[cfg(test)]
mod tests;
