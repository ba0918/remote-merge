//! diff が使う子の一覧の読み方（docs/ir/cli/directory-paths.md の REQ-cli-096）。一覧そのものは
//! 名前で指定したディレクトリの走査（[`super::diff_expansion`]）から答える。
//!
//! 「ディレクトリではない」と「一覧を取れなかった」を分ける。

use remote_merge_core::listing::ChildListing;

use super::diff_expansion::Expansions;
use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::side::Side;
use crate::tree::FileNode;

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
    expansions: &mut Expansions,
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    content: ContentRead,
) -> anyhow::Result<(Option<Vec<FileNode>>, ContentRead)> {
    if content.is_ok() {
        return Ok((None, content));
    }
    Ok(match expansions.list(core, side, path)? {
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
    expansions: &mut Expansions,
    core: &mut CoreRuntime,
    ref_side: &Side,
    path: &str,
) -> anyhow::Result<ReferenceKind> {
    Ok(match core.inspect_path(ref_side, path) {
        Ok(TargetPath::Symlink { .. }) => ReferenceKind::Differs,
        Ok(TargetPath::File { .. }) => match expansions.list(core, ref_side, path)? {
            ChildListing::Children(_) => ReferenceKind::Differs,
            ChildListing::NotDirectory => ReferenceKind::Readable,
            ChildListing::Unlistable(reason) => ReferenceKind::Unlistable(reason),
        },
        Ok(TargetPath::Missing { .. }) | Err(_) => ReferenceKind::Readable,
    })
}
