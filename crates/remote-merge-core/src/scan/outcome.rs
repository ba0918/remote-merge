//! 走査の求めと結果（一覧・不完全の報告・エラー）。エージェントの通信でもこの形のまま運ぶ。

use serde::{Deserialize, Serialize};

/// 走査の起点
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanScope {
    /// root_dir 全体
    Whole,
    /// 指定した起点だけ（一つもなければ root_dir を確かめるだけ）
    Starts(Vec<Start>),
    /// root_dir からのこのパスの直下の子（子の一覧。docs/ir/paths/operations.md の REQ-paths-017）。
    /// ディレクトリならそれ自身を器のディレクトリとして載せ、子のリンクは辿らずにリンク先が
    /// ディレクトリかだけを決める
    Children(String),
}

/// 走査の起点一つ。`path` は root_dir からの、"." の要素と空の要素と末尾の "/" を除いたもの
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Start {
    pub path: String,
    /// 起点そのものも載せる（include の値。docs/ir/config/include.md）か、下だけを載せる
    /// （指定したディレクトリ。TBL-scan-001）か
    pub with_self: bool,
    /// 下も集めるか。false なら起点がディレクトリかどうかだけを確かめ、ディレクトリなら器として
    /// 載せる（include の外の指定したディレクトリ。REQ-cli-077）
    pub descend: bool,
    /// 件数の上限を当てる組。同じ組の起点は件数を合わせて数える（TBL-scan-001）
    pub group: usize,
}

impl Start {
    /// include の値の起点
    pub fn value(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            with_self: true,
            descend: true,
            group: 0,
        }
    }

    /// 指定したディレクトリの下だけの起点
    pub fn below(path: impl Into<String>, group: usize) -> Self {
        Self {
            path: path.into(),
            with_self: false,
            descend: true,
            group,
        }
    }

    /// 指定したディレクトリがディレクトリかどうかだけを確かめる起点
    pub fn check(path: impl Into<String>, group: usize) -> Self {
        Self {
            path: path.into(),
            with_self: false,
            descend: false,
            group,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRequest {
    pub scope: ScanScope,
    pub max_entries: usize,
}

/// 一覧に載せた項目の種類
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListedKind {
    File,
    Directory,
    Symlink {
        target: String,
        is_dir: bool,
    },
    /// 通常ファイル・ディレクトリ・symlink のどれでもないもの（FIFO・ソケット・デバイス。
    /// docs/ir/scan/listing.md の REQ-scan-021）。中身は読まない
    Other,
}

/// 一覧に載せた項目。`path` は root_dir からのパス
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedEntry {
    pub path: String,
    pub kind: ListedKind,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    /// 更新時刻を読めた桁（秒の小数の桁数）
    pub mtime_digits: u8,
    pub permissions: u32,
}

/// 走査の一覧
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    pub entries: Vec<ListedEntry>,
    /// UTF-8 として読めない名前の項目の、読めない部分を `\xNN` と書いた root_dir からのパス
    pub unreadable_names: Vec<String>,
    /// 件数の上限を超えて途中で打ち切った
    pub truncated: bool,
    /// ディレクトリへの symlink の循環に出会って、そこで止めたパス（REQ-scan-002）
    pub cycle: Option<String>,
}

/// ディレクトリを読めなかった理由。経路によらず同じ文言にするため、OS や道具の文言は運ばない
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReadCause {
    PermissionDenied,
    Other,
}

impl std::fmt::Display for ReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::PermissionDenied => "permission denied",
            Self::Other => "read error",
        })
    }
}

/// 走査を止める誤り。パスは root_dir からのもので、読めない部分は `\xNN` と書く
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ScanFailure {
    #[error("root_dir not found")]
    RootMissing,
    #[error("Cannot read root_dir")]
    RootUnreadable,
    /// root_dir そのものが走査の途中でなくなったかディレクトリでなくなった
    #[error("root_dir changed during the scan")]
    RootChanged,
    #[error("Cannot scan {path}: a parent is not a directory")]
    StartBlocked { path: String },
    #[error("Cannot scan {path}: it cannot be read")]
    StartUnreadable { path: String },
    #[error("Cannot read directory: {path}: {cause}")]
    UnreadableDirectory { path: String, cause: ReadCause },
    #[error("Cannot follow symbolic link (loop or non-directory in its target): {path}")]
    UnresolvableLink { path: String },
    #[error("Cannot read the link target of {path}")]
    UnreadableLinkTarget { path: String },
    #[error("Cannot read the modification time of {path}")]
    UnreadableTime { path: String },
    #[error("Entry changed during the scan: {path}")]
    ChangedDuringScan { path: String },
    /// 集め方の答えが問いと合わない
    #[error("The scan answers did not match the questions: {detail}")]
    InconsistentAnswer { detail: String },
    /// 集め方そのものが失敗した（エージェントの中で起きたもの）
    #[error("Cannot collect the entries to scan: {message}")]
    CollectFailed { message: String },
}

/// 走査の誤り。集め方そのものの失敗は、元の誤り（接続断・時間切れ）のまま運ぶ
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error(transparent)]
    Failure(#[from] ScanFailure),
    #[error(transparent)]
    Collect(anyhow::Error),
}

impl From<super::raw::CollectError> for ScanError {
    fn from(error: super::raw::CollectError) -> Self {
        match error {
            super::raw::CollectError::Failed(error) => Self::Collect(error),
            // 集めている間に root_dir がなくなったら、ほかの答えによらず root_dir が変わったもの
            super::raw::CollectError::RootGone => Self::Failure(ScanFailure::RootChanged),
        }
    }
}
