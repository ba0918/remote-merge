//! エラートレラントなファイル読み込みユーティリティ。
//!
//! CLI サブコマンド（diff, merge, status）で共通して使用する
//! エラーをスキップするバッチファイル読み込み関数を提供する。

use std::collections::HashMap;

use crate::app::Side;
use crate::runtime::CoreRuntime;
use crate::service::status::RequiredContents;

pub fn fetch_contents_required(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
    force: bool,
) -> RequiredContents {
    let mut result = RequiredContents::default();
    if let Ok(contents) = core.read_files_bytes_batch(side, paths) {
        result.contents = contents;
    }
    for path in paths {
        if result.contents.contains_key(path) {
            continue;
        }
        match core.read_file_bytes(side, path, force) {
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
) -> HashMap<String, Vec<u8>> {
    // バッチバイト列読み込みを試行
    match core.read_files_bytes_batch(side, paths) {
        Ok(batch) => batch,
        Err(e) => {
            tracing::debug!("Batch read failed, falling back to individual reads: {}", e);
            // フォールバック: 1ファイルずつ（既存ロジック）
            let mut contents = HashMap::new();
            for path in paths {
                match core.read_file_bytes(side, path, false) {
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
