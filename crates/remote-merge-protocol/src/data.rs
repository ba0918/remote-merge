//! 通信で運ぶ値の型

use remote_merge_core::path_resolution::ExistingKind;
use serde::{Deserialize, Serialize};

use crate::WireFailure;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileReadResult {
    Ok {
        path: String,
        content: Vec<u8>,
        more_to_follow: bool,
    },
    Failed {
        path: String,
        failure: WireFailure,
    },
}

/// HashFiles レスポンス用のハッシュ結果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileHashResult {
    /// 通常ファイル: SHA-256 ハッシュ（hex string）
    Ok { path: String, hash: String },
    /// シンボリックリンク: リンクターゲットパス
    Symlink { path: String, target: String },
    /// 読めなかった
    Failed { path: String, failure: WireFailure },
}

/// InspectPath の応答。"/" から要素ごとに辿った確認の結果の四つの種類か、辿れなかった原因の文言
/// （`Unresolved`）か、確認の前に拒んだ理由（`Error`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentPathInspection {
    Missing {
        real_parent: String,
        parent_exists: bool,
    },
    Exists {
        real_path: String,
        kind: ExistingKind,
    },
    Symlink {
        link_target: String,
        real_path: String,
        /// 辿った先の種類（辿った先が存在しなければ None）
        target_kind: Option<ExistingKind>,
    },
    AncestorNotDirectory {
        real_path: String,
        regular_file: bool,
    },
    Unresolved {
        cause: String,
    },
    Error {
        message: String,
    },
}

/// ReadEntryLink の結果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentEntryLink {
    /// 項目が symlink。リンク先は読んだ文字列そのまま
    Symlink { target: String },
    /// symlink でない
    NotSymlink,
    /// 項目を読めなかった（ない・途中がディレクトリでないを含む）
    Failed { failure: WireFailure },
}
