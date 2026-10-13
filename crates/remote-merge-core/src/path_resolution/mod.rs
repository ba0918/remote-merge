//! root_dir の下のパスを "/" から要素ごとに辿る手順（docs/ir/paths/resolution.md）。
//!
//! 手順は [`resolve_path`] 一つで、lstat の種類・readlink・検索権限の基本操作を [`PathOps`] として
//! 外から受け取る。ローカルとエージェントは [`os::resolve_on_this_machine`] で同じ基本操作を使う。

mod error;
pub mod facts;
pub mod os;
mod walk;

pub use error::ResolveError;
pub use walk::{resolve_path, walk_missing_part};

/// 実パスの長さの上限（バイト）。つないだ直後の実パスがこれ以上なら止める
pub const PATH_LENGTH_LIMIT: usize = 4096;
/// 辿る名前の要素の長さの上限（バイト）。これを超えたら止める
pub const NAME_LENGTH_LIMIT: usize = 255;
/// 一回の確認でリンク文字列を差し込める回数
pub const SYMLINK_LIMIT: usize = 40;

/// パスの確認の結果。実パスは "/" から始まり、symlink・"."・空の要素・末尾の "/" を含まない
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPath {
    /// 存在しない。`real_parent` は末尾の要素を含めない親の実パス
    Missing {
        real_parent: String,
        parent_exists: bool,
    },
    /// symlink でないもの（ディレクトリを含む）が存在する。`kind` はその種類
    Exists {
        real_path: String,
        kind: ExistingKind,
    },
    /// 末尾の要素が symlink。`real_path` はリンク先を同じ規則で辿った実パスで、`target_kind` は
    /// 辿った先の種類（辿った先が存在しなければ None）
    Symlink {
        link_target: String,
        real_path: String,
        target_kind: Option<ExistingKind>,
    },
    /// 確認するパスの途中の要素がディレクトリでない。`real_path` はその要素を辿った実パス
    AncestorNotDirectory {
        real_path: String,
        regular_file: bool,
    },
}

/// 存在するもの（symlink は辿った先）の種類（foundation A2 の「存在する（ファイル・ディレクトリ・
/// その他）」）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExistingKind {
    File,
    Directory,
    Other,
}

/// 一つのパスを lstat した種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Missing,
    Directory,
    RegularFile,
    Symlink,
    Other,
}

/// 辿る手順が使う基本操作。パスはどれも "/" から始まる実パス
pub trait PathOps {
    /// `path` を辿らずに調べた種類
    fn entry_kind(&self, path: &str) -> Result<EntryKind, ResolveError>;
    /// 実行する利用者が実効 uid でディレクトリ `directory` を検索できるか
    fn can_search(&self, directory: &str) -> Result<bool, ResolveError>;
    /// symlink `path` のリンク文字列のバイト列
    fn read_link(&self, path: &str) -> Result<Vec<u8>, ResolveError>;
}

#[cfg(all(test, unix))]
mod kind_tests;
