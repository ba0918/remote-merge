//! エラートレラントなファイル読み込みユーティリティ。
//!
//! CLI サブコマンド（diff, merge, status）で共通して使用する
//! エラーをスキップするバッチファイル読み込み関数を提供する。

use std::collections::HashMap;

use crate::runtime::size_limit::{SizeLimit, SizeQueryFailed};
use crate::runtime::target_io::TargetPath;
use crate::runtime::CoreRuntime;
use crate::service::status::RequiredContents;
use crate::side::Side;

/// 中身を読む。読めず、パスの確認でその側にないと分かったときだけ None を返し、
/// それ以外の読めなかったことは誤りのまま返す（エラーの文で「ない」を見分けない）
pub fn read_unless_missing(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    limit: SizeLimit,
) -> anyhow::Result<Option<Vec<u8>>> {
    match core.read_file_bytes(side, path, limit == SizeLimit::Lifted) {
        Ok(content) => Ok(Some(content)),
        Err(error) => match core.inspect_path(side, path) {
            Ok(TargetPath::Missing { .. }) => Ok(None),
            _ => Err(error),
        },
    }
}

/// 読めなかった側ごとに "側: 理由" を並べた、読めなかったファイルの理由
pub fn read_failure_reason(
    left: (&Side, Option<&anyhow::Error>),
    right: (&Side, Option<&anyhow::Error>),
) -> String {
    [left, right]
        .into_iter()
        .filter_map(|(side, error)| {
            error.map(|error| format!("{}: {error:#}", side.display_name()))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn fetch_contents_required(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
    limit: SizeLimit,
) -> RequiredContents {
    let mut result = RequiredContents::default();
    match core.read_files_bytes_batch(side, paths, limit) {
        Ok(contents) => result.contents = contents,
        Err(error) if error.is::<SizeQueryFailed>() => {
            result.errors = paths
                .iter()
                .map(|path| (path.clone(), error.to_string()))
                .collect();
            return result;
        }
        Err(_) => {}
    }
    for path in paths {
        if result.contents.contains_key(path) {
            continue;
        }
        match core.read_file_bytes(side, path, limit == SizeLimit::Lifted) {
            Ok(content) => {
                result.contents.insert(path.clone(), content);
            }
            Err(error) => {
                result.errors.insert(path.clone(), error.to_string());
            }
        }
    }
    result
}

/// 複数ファイルのバイト列コンテンツをバッチ取得する（エラーはスキップ）。
///
/// 読み込みに失敗したファイルは結果に含まれず、debug ログのみ出力する。
/// ref badge 計算やコンテンツ比較など、読み込み失敗が致命的でない場面で使用する。
/// バイト列で返すため、バイナリファイルも lossy 変換なしで正しく比較できる。
pub fn fetch_contents_tolerant(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
    limit: SizeLimit,
) -> HashMap<String, Vec<u8>> {
    // バッチバイト列読み込みを試行
    match core.read_files_bytes_batch(side, paths, limit) {
        Ok(batch) => batch,
        Err(e) if e.is::<SizeQueryFailed>() => {
            tracing::debug!("File size query failed, not reading: {}", e);
            HashMap::new()
        }
        Err(e) => {
            tracing::debug!("Batch read failed, falling back to individual reads: {}", e);
            // フォールバック: 1ファイルずつ（既存ロジック）
            let mut contents = HashMap::new();
            for path in paths {
                match core.read_file_bytes(side, path, limit == SizeLimit::Lifted) {
                    Ok(content) => {
                        contents.insert(path.clone(), content);
                    }
                    Err(e) => {
                        tracing::debug!("Failed to read {}: {}", path, e);
                    }
                }
            }
            contents
        }
    }
}
