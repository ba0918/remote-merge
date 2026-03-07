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
use crate::config::AppConfig;
use crate::merge::executor;
use crate::runtime::CoreRuntime;
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{resolve_source_pair, SourceArgs};
use crate::service::status::{
    build_status_output, compute_status_from_trees, needs_content_compare,
    refine_status_with_content, status_exit_code,
};
use crate::service::types::SourceInfo;
use crate::tree::FileTree;
use crate::{config, local};

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

    // 左側ツリー取得（再帰走査でメタデータ付き）
    let (left_tree, left_info) = fetch_side_tree_recursive(&pair.left, &config, &mut core)?;

    // 右側ツリー取得（再帰走査でメタデータ付き）
    let (right_tree, right_info) = fetch_side_tree_recursive(&pair.right, &config, &mut core)?;

    // ステータス計算（メタデータ比較）
    let mut files = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

    // コンテンツ比較が必要なファイルを抽出
    let paths_to_compare = needs_content_compare(&files, &left_tree, &right_tree);

    if !paths_to_compare.is_empty() {
        // コンテンツ取得・比較
        let contents = fetch_contents_for_compare(
            &pair.left,
            &pair.right,
            &paths_to_compare,
            &config,
            &mut core,
        )?;
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

/// Side から再帰走査ツリーと SourceInfo を取得する
fn fetch_side_tree_recursive(
    side: &Side,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<(FileTree, SourceInfo)> {
    match side {
        Side::Local => {
            let (nodes, truncated) = local::scan_local_tree_recursive(
                &config.local.root_dir,
                &config.filter.exclude,
                MAX_SCAN_ENTRIES,
            )?;
            if truncated {
                tracing::warn!("Local tree scan truncated at {} entries", MAX_SCAN_ENTRIES);
            }
            let mut tree = FileTree::new(&config.local.root_dir);
            tree.nodes = nodes;
            tree.sort();
            let info = SourceInfo {
                label: "local".into(),
                root: config.local.root_dir.to_string_lossy().to_string(),
            };
            Ok((tree, info))
        }
        Side::Remote(server_name) => {
            core.connect(server_name)?;
            let tree = core.fetch_remote_tree_recursive(server_name, MAX_SCAN_ENTRIES)?;
            let server_config = core.get_server_config(server_name)?;
            let info = SourceInfo {
                label: server_name.clone(),
                root: format!(
                    "{}:{}",
                    server_config.host,
                    server_config.root_dir.to_string_lossy()
                ),
            };
            Ok((tree, info))
        }
    }
}

/// 左右のファイルコンテンツを取得して比較用ペアにまとめる
fn fetch_contents_for_compare(
    left_side: &Side,
    right_side: &Side,
    paths: &[String],
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<HashMap<String, (String, String)>> {
    let left_contents = fetch_side_contents(left_side, paths, config, core)?;
    let right_contents = fetch_side_contents(right_side, paths, config, core)?;

    let mut result = HashMap::new();
    for path in paths {
        if let (Some(l), Some(r)) = (left_contents.get(path), right_contents.get(path)) {
            result.insert(path.clone(), (l.clone(), r.clone()));
        }
    }
    Ok(result)
}

/// 片側のファイルコンテンツをバッチ取得する
fn fetch_side_contents(
    side: &Side,
    paths: &[String],
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<HashMap<String, String>> {
    match side {
        Side::Local => {
            let mut contents = HashMap::new();
            for path in paths {
                match executor::read_local_file(&config.local.root_dir, path) {
                    Ok(content) => {
                        contents.insert(path.clone(), content);
                    }
                    Err(e) => {
                        tracing::debug!("Failed to read local file {}: {}", path, e);
                    }
                }
            }
            Ok(contents)
        }
        Side::Remote(server_name) => core.read_remote_files_batch(server_name, paths),
    }
}
