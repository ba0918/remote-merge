//! status サブコマンドの実装。
//!
//! CoreRuntime でツリーを取得し、Service 層で比較結果を計算、
//! フォーマッターで出力する。
//!
//! 比較は3段階で行う:
//! 1. ツリー存在チェック → LeftOnly / RightOnly を確定
//! 2. メタデータ比較（size, mtime） → Equal / Modified の事前判定
//! 3. コンテンツ比較（メタデータで判定できないファイルのみ） → 最終判定

use std::collections::HashMap;

use crate::app::Side;
use crate::config;
use crate::runtime::CoreRuntime;
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{build_source_info, resolve_source_pair, SourceArgs};
use crate::service::status::{
    build_status_output, compute_status_from_trees, needs_content_compare,
    refine_status_with_content, status_exit_code,
};

/// 再帰走査の最大エントリ数
const MAX_SCAN_ENTRIES: usize = 50_000;

/// status サブコマンドの引数
pub struct StatusArgs {
    pub server: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub format: String,
    pub summary: bool,
}

/// status サブコマンドを実行する
pub fn run_status(args: StatusArgs) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let config = config::load_config()?;

    let source_args = SourceArgs {
        server: args.server,
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let mut core = CoreRuntime::new(config.clone());

    // 接続
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // 左側ツリー取得（再帰走査でメタデータ付き）
    let left_tree = core.fetch_tree_recursive(&pair.left, MAX_SCAN_ENTRIES)?;
    let left_info = build_source_info(&pair.left, &core)?;

    // 右側ツリー取得（再帰走査でメタデータ付き）
    let right_tree = core.fetch_tree_recursive(&pair.right, MAX_SCAN_ENTRIES)?;
    let right_info = build_source_info(&pair.right, &core)?;

    // ステータス計算（メタデータ比較）
    let mut files = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

    // コンテンツ比較が必要なファイルを抽出
    let paths_to_compare = needs_content_compare(&files, &left_tree, &right_tree);

    if !paths_to_compare.is_empty() {
        // コンテンツ取得・比較
        let contents =
            fetch_contents_for_compare(&pair.left, &pair.right, &paths_to_compare, &mut core)?;
        refine_status_with_content(&mut files, &contents);
    }

    let output = build_status_output(left_info, right_info, files, args.summary);
    let code = status_exit_code(&output.summary);

    // 出力
    let text = match format {
        OutputFormat::Text => format_status_text(&output, args.summary),
        OutputFormat::Json => format_json(&output)?,
    };
    println!("{}", text);

    core.disconnect_all();
    Ok(code)
}

/// 左右のファイルコンテンツを取得して比較用ペアにまとめる
fn fetch_contents_for_compare(
    left_side: &Side,
    right_side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
) -> anyhow::Result<HashMap<String, (String, String)>> {
    // バッチ読み込み（エラーが起きた場合はスキップ）
    let left_contents = fetch_side_contents_tolerant(left_side, paths, core);
    let right_contents = fetch_side_contents_tolerant(right_side, paths, core);

    let mut result = HashMap::new();
    for path in paths {
        if let (Some(l), Some(r)) = (left_contents.get(path), right_contents.get(path)) {
            result.insert(path.clone(), (l.clone(), r.clone()));
        }
    }
    Ok(result)
}

/// 片側のファイルコンテンツをバッチ取得する（読み込みエラーはスキップ）
fn fetch_side_contents_tolerant(
    side: &Side,
    paths: &[String],
    core: &mut CoreRuntime,
) -> HashMap<String, String> {
    // read_files_batch はエラー時に全体が失敗するため、
    // 個別に読み込んでエラーをスキップする
    let mut contents = HashMap::new();
    for path in paths {
        match core.read_file(side, path) {
            Ok(content) => {
                contents.insert(path.clone(), content);
            }
            Err(e) => {
                tracing::debug!("Failed to read file {} from {:?}: {}", path, side, e);
            }
        }
    }
    contents
}
