//! root_dir の下の一つの項目を指すパスの型。
//!
//! root_dir 全体を指す値（"/"・"."・"./"・空の値など）は範囲を表すので、走査・子の一覧・パスの
//! 確認には渡せるが、一つの項目への操作（読み書き・削除・権限の変更・symlink の作成）には
//! 渡せない。項目の操作は [`EntryPath`] だけを受け取り、範囲の値が届く誤りを型で起きなくする。

use super::{RelPath, RelPathError};

/// root_dir の下の一つの項目。空と "." でない要素を一つ以上持つ
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryPath {
    rel: RelPath,
}

impl EntryPath {
    /// 入力の形を検査し、root_dir 全体を指す値も拒む
    pub fn parse(input: &str) -> Result<Self, RelPathError> {
        Self::try_from(&RelPath::parse(input)?)
    }

    /// 末尾の "/" を除いた値（空にならない）
    pub fn as_str(&self) -> &str {
        self.rel.as_str()
    }

    pub fn as_typed(&self) -> &str {
        self.rel.as_typed()
    }

    /// 範囲を受け取る操作（パスの確認など）に渡す形
    pub fn as_rel(&self) -> &RelPath {
        &self.rel
    }

    /// 親（root_dir 直下なら root_dir 全体）と最後の名前
    pub fn parent_and_name(&self) -> (RelPath, &str) {
        self.rel.parent_and_name()
    }
}

impl TryFrom<&RelPath> for EntryPath {
    type Error = RelPathError;

    fn try_from(rel: &RelPath) -> Result<Self, Self::Error> {
        let names_an_entry = rel
            .as_typed()
            .split('/')
            .any(|element| !element.is_empty() && element != ".");
        if names_an_entry {
            Ok(Self { rel: rel.clone() })
        } else {
            Err(RelPathError::RootDirItself(rel.as_typed().to_string()))
        }
    }
}

/// EntryPath の並びを、入力されたままの値の並びにする（通信で送る値や結果のキー）
pub fn entries_as_typed(entries: &[EntryPath]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.as_typed().to_string())
        .collect()
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
