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
use crate::cli::ref_guard;
use crate::config;
use crate::runtime::CoreRuntime;
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    build_status_output, compute_ref_badges, compute_status_from_trees, needs_content_compare,
    refine_status_with_content, status_exit_code,
};

/// 再帰走査の最大エントリ数
const MAX_SCAN_ENTRIES: usize = 50_000;

/// status サブコマンドの引数
pub struct StatusArgs {
    pub server: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
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

    // Ref server handling
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);

    // ref 指定時は全非 sensitive ファイルのコンテンツが必要（badge 計算用）。
    // ref 未指定時は paths_to_compare のみ読めばよい。
    // いずれの場合も左右コンテンツを1回だけ読み、両方の用途で再利用する。
    let content_paths = if ref_side.is_some() {
        files
            .iter()
            .filter(|f| !f.sensitive)
            .map(|f| f.path.clone())
            .collect::<Vec<_>>()
    } else {
        paths_to_compare.clone()
    };

    let left_contents = if !content_paths.is_empty() {
        fetch_side_contents_tolerant(&pair.left, &content_paths, &mut core)
    } else {
        HashMap::new()
    };
    let right_contents = if !content_paths.is_empty() {
        fetch_side_contents_tolerant(&pair.right, &content_paths, &mut core)
    } else {
        HashMap::new()
    };

    // コンテンツ比較で status を精緻化
    if !paths_to_compare.is_empty() {
        let mut compare_pairs = HashMap::new();
        for path in &paths_to_compare {
            if let (Some(l), Some(r)) = (left_contents.get(path), right_contents.get(path)) {
                compare_pairs.insert(path.clone(), (l.clone(), r.clone()));
            }
        }
        refine_status_with_content(&mut files, &compare_pairs);
    }

    let mut ref_info = None;
    let mut ref_badges = None;

    if let Some(ref_s) = &ref_side {
        // Connect to ref server
        core.connect_if_remote(ref_s)?;

        // Fetch ref tree
        let ref_tree = core.fetch_tree_recursive(ref_s, MAX_SCAN_ENTRIES)?;
        ref_info = Some(build_source_info(ref_s, &core)?);

        // Fetch ref contents for non-sensitive files
        let ref_paths: Vec<String> = files
            .iter()
            .filter(|f| !f.sensitive)
            .map(|f| f.path.clone())
            .collect();
        let ref_contents = fetch_side_contents_tolerant(ref_s, &ref_paths, &mut core);

        // Compute ref badges — left/right contents は既に取得済みのものを再利用
        let badges = compute_ref_badges(
            &files,
            &left_tree,
            &right_tree,
            &ref_tree,
            &left_contents,
            &right_contents,
            &ref_contents,
        );
        ref_badges = Some(badges);
    }

    let output = build_status_output(
        left_info,
        right_info,
        files,
        args.summary,
        ref_info,
        ref_badges.as_ref(),
    );
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
