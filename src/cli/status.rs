//! status サブコマンドの実装。
//!
//! CoreRuntime でツリーを取得し、Service 層で比較結果を計算、
//! フォーマッターで出力する。

use crate::app::Side;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{resolve_source_pair, SourceArgs};
use crate::service::status::{build_status_output, compute_status_from_trees, status_exit_code};
use crate::service::types::SourceInfo;
use crate::tree::FileTree;
use crate::{config, local};

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

    // 左側ツリー取得
    let (left_tree, left_info) = fetch_side_tree(&pair.left, &config, &mut core)?;

    // 右側ツリー取得
    let (right_tree, right_info) = fetch_side_tree(&pair.right, &config, &mut core)?;

    // ステータス計算
    let files = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);
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

/// Side から ツリーと SourceInfo を取得する
fn fetch_side_tree(
    side: &Side,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<(FileTree, SourceInfo)> {
    match side {
        Side::Local => {
            let tree = local::scan_local_tree(&config.local.root_dir, &config.filter.exclude)?;
            let info = SourceInfo {
                label: "local".into(),
                root: config.local.root_dir.to_string_lossy().to_string(),
            };
            Ok((tree, info))
        }
        Side::Remote(server_name) => {
            core.connect(server_name)?;
            let tree = core.fetch_remote_tree(server_name)?;
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
