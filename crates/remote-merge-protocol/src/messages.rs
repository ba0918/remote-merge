//! 要求と応答

use serde::{Deserialize, Serialize};

use crate::{
    AgentEntryLink, AgentPathCheck, AgentPathInspection, FileHashResult, FileReadResult,
    ListedEntry, RelPathError, ScanFailure, ScanRequest, WireFailure,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentRequest {
    /// root_dir の下を走査の規則で走査する。`exclude` は root_dir からのパスに当てる
    ListTree {
        request: ScanRequest,
        exclude: Vec<String>,
    },
    /// `size_limit` を超える大きさのファイルは中身を送らずに上限を超える失敗にする
    ReadFiles {
        paths: Vec<String>,
        chunk_size_limit: usize,
        size_limit: Option<u64>,
    },
    HashFiles {
        paths: Vec<String>,
    },
    /// パスごとの中身の SHA-256。末尾の symlink も辿った先の中身のハッシュを取る（比べる流れが使う）。
    /// 応答は FileHashes で、Symlink の結果は返さない
    HashContents {
        paths: Vec<String>,
    },
    WriteFile {
        path: String,
        content: Vec<u8>,
        is_binary: bool,
        more_to_follow: bool,
    },
    InspectPath {
        path: String,
    },
    /// いくつものパスを一度に確かめる。答えは同じ順の `PathInspections`
    InspectPaths {
        paths: Vec<String>,
    },
    Symlink {
        path: String,
        target: String,
    },
    /// root_dir からの `path` を消す。末尾の symlink ならリンクだけを消す
    Remove {
        path: String,
    },
    /// root_dir からの `path` の権限を `mode` にする
    Chmod {
        path: String,
        mode: u32,
    },
    /// root_dir からの `path` の項目そのもの（最後の要素のリンクは辿らない）が symlink なら、その
    /// リンク先を読む。親ディレクトリは一覧しない
    ReadEntryLink {
        path: String,
    },
    Shutdown,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentResponse {
    /// ListTree の一覧。項目が多いときは分けて返し、`is_last` の応答に読めない名前と打ち切りを
    /// 載せる
    TreeChunk {
        entries: Vec<ListedEntry>,
        is_last: bool,
        /// UTF-8 として読めない名前の項目の root_dir からのパス（読めない部分を `\xNN` と書いた形）
        unreadable_names: Vec<String>,
        /// 件数の上限を超えて打ち切った
        truncated: bool,
        /// 循環に出会って止めたパス
        cycle: Option<String>,
    },
    /// 走査を止めた（root_dir がない・読めないディレクトリ・循環など）。一覧は返さない
    TreeFailed {
        failure: ScanFailure,
    },
    FileContents {
        results: Vec<FileReadResult>,
        /// ストリーミング対応: true で最後のチャンク。
        is_last: bool,
        /// 入力の形の検査で拒んだパス。読まずに、最後のチャンクで返す
        refused: Vec<RelPathError>,
    },
    FileHashes {
        results: Vec<FileHashResult>,
        /// ストリーミング対応: true で最後のチャンク。
        is_last: bool,
        /// 入力の形の検査で拒んだパス
        refused: Vec<RelPathError>,
    },
    /// WriteFile・Symlink・Remove・Chmod の結果。失敗ならその種類
    Changed {
        failure: Option<WireFailure>,
    },
    PathInspection {
        result: AgentPathInspection,
    },
    /// InspectPaths の応答。尋ねたパスと同じ順
    PathInspections {
        results: Vec<AgentPathCheck>,
    },
    /// ReadEntryLink の応答
    EntryLink {
        result: AgentEntryLink,
    },
    /// 一つのパスの要求で、そのパスを入力の形の検査で拒んだ。何も読み書きしていない
    PathRefused {
        refusal: RelPathError,
    },
    Pong,
    Error {
        message: String,
    },
}
