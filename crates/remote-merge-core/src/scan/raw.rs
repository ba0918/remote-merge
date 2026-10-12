//! 経路が集める生の項目と、走査の規則が集め方に出す問いと答え（docs/decision/records/
//! 2026-10-12-foundation-2.md の C6・C7）。
//!
//! 集め方は名前をバイト列のまま、種類・大きさ・更新時刻・権限・リンク文字列・デバイス番号と
//! inode 番号を読んで返すだけで、何を載せるか・辿るか・止めるかは決めない。

use super::exclude::Exclude;
pub use super::outcome::ReadCause;

/// デバイス番号と inode 番号。ディレクトリへの symlink の循環を見分けるのに使う（REQ-scan-002）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirId {
    pub dev: u64,
    pub ino: u64,
}

/// 項目の更新時刻。読めた桁（秒の小数の桁数）も持つ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawTime {
    Read {
        secs: i64,
        nanos: u32,
        digits: u8,
    },
    /// 更新時刻を読めなかった（REQ-scan-015）
    Unreadable,
}

/// 項目の種類。symlink はリンク文字列を持ち、読めなかったら `None`（REQ-scan-015）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawKind {
    File,
    Directory,
    Symlink {
        target: Option<Vec<u8>>,
    },
    /// 通常ファイル・ディレクトリ・symlink のどれでもないもの（FIFO・ソケット・デバイス）
    Other,
}

/// 集めた一つの項目。`path` は問いのパスからの相対パスで、問いのパスそのものは空
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub path: Vec<u8>,
    pub kind: RawKind,
    pub size: u64,
    pub mtime: RawTime,
    pub mode: u32,
    pub id: DirId,
}

/// symlink のリンク先を辿った結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    Directory,
    NotDirectory,
    /// リンク先がない（リンク切れ）
    Missing,
    /// ループしているか途中にディレクトリでないものを含むか、読めないために辿れない（REQ-scan-018）
    Unresolvable,
}

/// 問いのパスそのものに届かなかった理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreached {
    /// パスがない
    Missing,
    /// 途中の要素がディレクトリでない
    NotDirectory,
    /// 権限がないなど、そのほかの理由で読めない
    Unreadable,
}

/// 集め方への問い。`path` は root_dir からの、辿ったリンクの名前で書いたパス（空なら root_dir）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub path: Vec<u8>,
    /// パスが（リンクを辿って）ディレクトリなら、その下の項目も集めるか
    pub descend: bool,
}

/// 起点の下の項目。`start` はリンクを辿った起点自身の項目で、循環の判定にその番号を使う
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtree {
    pub start: RawEntry,
    pub entries: Vec<RawEntry>,
    /// 読めなかったディレクトリ
    pub unreadable: Vec<UnreadableDir>,
    /// 情報（種類や更新時刻）を読めなかった項目。`path` はその項目自身の、問いのパスからの相対
    /// パス。exclude に当たるかは規則が決める
    pub unreadable_entries: Vec<UnreadableDir>,
}

/// 読めなかったディレクトリ。`path` は問いのパスからの相対パス（起点そのものは空）
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnreadableDir {
    pub path: Vec<u8>,
    pub cause: ReadCause,
}

impl UnreadableDir {
    pub fn new(path: Vec<u8>, cause: ReadCause) -> Self {
        Self { path, cause }
    }
}

/// 問いへの答え
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// 問いのパスそのもの。root_dir はリンクを辿った項目、それ以外は辿らない項目
    pub entry: Result<RawEntry, Unreached>,
    /// 問いのパスが symlink のとき、そのリンク先（root_dir では問わない）
    pub link: Option<LinkTarget>,
    /// `descend` で、パスがリンクを辿ってディレクトリのときの下の項目
    pub subtree: Option<Subtree>,
}

/// 生の項目を集める経路。問いは一度にまとめて渡す（C7）。`exclude` に当たるディレクトリには
/// 降りないことが望ましいが、降りても規則の側で除く（REQ-config-044）
pub trait Collector {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, CollectError>;
}

/// 集め方そのものの失敗（接続・道具の出力を読めない・時間切れ）。走査はこれで止まる
#[derive(Debug)]
pub struct CollectError(pub anyhow::Error);

impl From<anyhow::Error> for CollectError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

/// 相対パス `base` の下の `name`（`base` が空なら `name` そのもの）
pub fn join_relative(base: &[u8], name: &[u8]) -> Vec<u8> {
    super::parts::join(base, name)
}
