//! CLI サブコマンド（diff, merge, sync, status）が共通して使う、中身の読み込み。
//!
//! 中身はすべて `read_contents` で読み、パスごとの結果（読めた中身か読めなかった理由）を
//! `ContentReads` で受け取る。呼び出し元は失敗を捨てず、読めなかったパスを読み直してその失敗を
//! 消さない（読めた中身を後の段でもう一度読むことは妨げない）。

use crate::runtime::size_limit::SizeLimit;
use crate::runtime::CoreRuntime;
use crate::service::content_reads::ContentReads;
use crate::side::Side;

/// `side` の `paths` の中身を読む。
///
/// 経路はパスごとに中身か失敗の種類を返すので、読めなかったパスを読み直さない。尋ねる操作
/// そのものがやり直しても `接続断` だったときなどは、尋ねたパスすべてをその理由の失敗にする
/// （docs/ir/ssh/reconnect.md の REQ-ssh-003）。
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
    match core.read_files(side, paths, limit) {
        Ok(results) => {
            for (path, content) in results {
                match content {
                    Ok(content) => reads.record_read(path, content),
                    Err(error) => reads.record_failure(path.as_str(), format!("{error:#}")),
                }
            }
        }
        Err(error) => {
            for path in paths {
                reads.record_failure(path.as_str(), format!("{error:#}"));
            }
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
