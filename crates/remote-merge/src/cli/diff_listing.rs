//! diff が使う子の一覧（docs/ir/cli/directory-paths.md の REQ-cli-096）。
//!
//! 「ディレクトリではない」と「一覧を取れなかった」を分ける。一覧の途中の `接続断` は、やり直しの
//! 後もだめなら一覧を取れなかったものとしてそのパスを errors に出す。一覧の中のリンク先か更新時刻を
//! 読めないときは `Err` のまま返し、diff を止める。

use remote_merge_core::listing::ChildListing;

use crate::runtime::connection::classify;
use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::side::Side;
use crate::tree::FileNode;

/// `side` の `path` の子の一覧
pub(crate) fn list(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
) -> anyhow::Result<ChildListing> {
    match core.list_children(side, path) {
        Err(error) if classify(&error).is_disconnection() => {
            Ok(ChildListing::Unlistable(format!("{error:#}")))
        }
        result => result,
    }
}

/// ディレクトリの直下の子。ディレクトリでないか一覧を取れなかったら None
pub(crate) fn children(listing: ChildListing) -> Option<Vec<FileNode>> {
    match listing {
        ChildListing::Children(nodes) => Some(nodes),
        ChildListing::NotDirectory | ChildListing::Unlistable(_) => None,
    }
}

/// 一覧を取れなかった理由
pub(crate) fn unlistable_reason(listing: &ChildListing) -> Option<&str> {
    match listing {
        ChildListing::Unlistable(reason) => Some(reason),
        _ => None,
    }
}

/// 片側の中身の読み込みの結果。読めなかったときは理由
pub(crate) type ContentRead = Result<Option<Vec<u8>>, String>;

/// 中身を読めなかった側の子の一覧。一覧を取れなかったときは、読めなかった理由をその理由に替える
pub(crate) fn children_of_unread(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    content: ContentRead,
) -> anyhow::Result<(Option<Vec<FileNode>>, ContentRead)> {
    if content.is_ok() {
        return Ok((None, content));
    }
    Ok(match list(core, side, path)? {
        ChildListing::Unlistable(reason) => (None, Err(reason)),
        listing => (children(listing), content),
    })
}

/// 参照先の同じパスの種類
pub(crate) enum ReferenceKind {
    /// テキストとして比べる通常ファイルと同じか、確かめられないので読みに進める
    Readable,
    /// 末尾の symlink かディレクトリ（REQ-cli-090）
    Differs,
    /// ディレクトリかもしれないのに一覧を取れなかった。値は理由
    Unlistable(String),
}

/// 参照先の同じパスが、テキストとして比べる通常ファイルと種類の違うものか。
/// 種類を確かめられないときは読みに進め、その読み込みの失敗を出す
pub(crate) fn reference_kind(
    core: &mut CoreRuntime,
    ref_side: &Side,
    path: &str,
) -> anyhow::Result<ReferenceKind> {
    Ok(match core.inspect_path(ref_side, path) {
        Ok(TargetPath::Symlink { .. }) => ReferenceKind::Differs,
        Ok(TargetPath::File { .. }) => match list(core, ref_side, path)? {
            ChildListing::Children(_) => ReferenceKind::Differs,
            ChildListing::NotDirectory => ReferenceKind::Readable,
            ChildListing::Unlistable(reason) => ReferenceKind::Unlistable(reason),
        },
        Ok(TargetPath::Missing { .. }) | Err(_) => ReferenceKind::Readable,
    })
}
