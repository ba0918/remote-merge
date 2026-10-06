//! CLI サブコマンド（diff, merge, sync, status）が共通して使う、中身の読み込み。
//!
//! 中身はすべて `read_contents` で読み、パスごとの結果（読めた中身か読めなかった理由）を
//! `ContentReads` で受け取る。呼び出し元は失敗を捨てず、読めなかったパスを読み直してその失敗を
//! 消さない（読めた中身を後の段でもう一度読むことは妨げない）。

use crate::runtime::size_limit::{SizeLimit, SizeQueryFailed};
use crate::runtime::CoreRuntime;
use crate::service::content_reads::ContentReads;
use crate::side::Side;

/// `side` の `paths` の中身を読む。
///
/// まとめて読み、まとめての結果に入らなかったパス（大きさの上限を超えたもの、経路が読めなかった
/// もの）は一件ずつ読んでその理由を受け取る。大きさを尋ねる操作そのものが失敗したときは、
/// 一件ずつ読み直さずに尋ねたパスすべての失敗にする（読み直すと次の問い合わせの結果で失敗が
/// 消えうる）。
pub fn read_contents(
    core: &mut CoreRuntime,
    side: &Side,
    paths: &[String],
    limit: SizeLimit,
) -> ContentReads {
    let mut reads = ContentReads::default();
    if paths.is_empty() {
        return reads;
    }
    match core.read_files_bytes_batch(side, paths, limit) {
        Ok(contents) => {
            for (path, content) in contents {
                reads.record_read(path, content);
            }
        }
        Err(error) if error.is::<SizeQueryFailed>() => {
            for path in paths {
                reads.record_failure(path.as_str(), format!("{error:#}"));
            }
            return reads;
        }
        Err(error) => {
            tracing::debug!("Batch read failed, reading one by one: {error:#}");
        }
    }
    for path in paths {
        if reads.contains(path) {
            continue;
        }
        match core.read_file_bytes(side, path, limit == SizeLimit::Lifted) {
            Ok(content) => reads.record_read(path.as_str(), content),
            Err(error) => reads.record_failure(path.as_str(), format!("{error:#}")),
        }
    }
    reads
}

/// `side` の `path` 一つの中身を `read_contents` で読む
pub fn read_one(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    limit: SizeLimit,
) -> Result<Vec<u8>, String> {
    read_contents(core, side, &[path.to_string()], limit)
        .take(path)
        .unwrap_or_else(|| Err("no content returned".into()))
}

/// 読めなかった側ごとに "側: 理由" を並べた、読めなかったファイルの理由
pub fn read_failure_reason(sides: &[(&Side, Option<&str>)]) -> String {
    sides
        .iter()
        .filter_map(|(side, reason)| {
            reason.map(|reason| format!("{}: {reason}", side.display_name()))
        })
        .collect::<Vec<_>>()
        .join("; ")
}
