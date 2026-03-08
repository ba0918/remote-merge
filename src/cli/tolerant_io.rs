//! エラートレラントなファイル読み込みユーティリティ。
//!
//! CLI サブコマンド（diff, merge, status）で共通して使用する
//! エラーをスキップするバッチファイル読み込み関数を提供する。

use std::collections::HashMap;

use crate::app::Side;
use crate::runtime::CoreRuntime;

/// 複数ファイルのコンテンツをバッチ取得する（エラーはスキップ）。
///
/// 読み込みに失敗したファイルは結果に含まれず、debug ログのみ出力する。
/// ref badge 計算やコンテンツ比較など、読み込み失敗が致命的でない場面で使用する。
pub fn fetch_contents_tolerant(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
) -> HashMap<String, String> {
    let mut contents = HashMap::new();
    for path in paths {
        match core.read_file(side, path) {
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
