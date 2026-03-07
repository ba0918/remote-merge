//! diff サブコマンドの実装。

use crate::app::Side;
use crate::config;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::diff::{build_diff_output, diff_exit_code};
use crate::service::output::{format_diff_text, format_json, OutputFormat};
use crate::service::source_pair::{resolve_source_pair, SourceArgs};
use crate::service::status::is_sensitive;
use crate::service::types::SourceInfo;

/// diff サブコマンドの引数
pub struct DiffArgs {
    pub path: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub format: String,
    pub max_lines: Option<usize>,
}

/// diff サブコマンドを実行する
pub fn run_diff(args: DiffArgs) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let config = config::load_config()?;

    let source_args = SourceArgs {
        server: None,
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let mut core = CoreRuntime::new(config.clone());

    // 左側コンテンツ取得
    let (left_content, left_info) = read_side_file(&pair.left, &args.path, &config, &mut core)?;

    // 右側コンテンツ取得
    let (right_content, right_info) = read_side_file(&pair.right, &args.path, &config, &mut core)?;

    let sensitive = is_sensitive(&args.path, &config.filter.sensitive);

    let output = build_diff_output(
        &args.path,
        left_info,
        right_info,
        &left_content,
        &right_content,
        sensitive,
        args.max_lines,
    );
    let code = diff_exit_code(&output);

    let text = match format {
        OutputFormat::Text => format_diff_text(&output),
        OutputFormat::Json => format_json(&output)?,
    };
    println!("{}", text);

    core.disconnect_all();
    Ok(code)
}

/// Side からファイル内容と SourceInfo を取得する
fn read_side_file(
    side: &Side,
    rel_path: &str,
    config: &AppConfig,
    core: &mut CoreRuntime,
) -> anyhow::Result<(String, SourceInfo)> {
    match side {
        Side::Local => {
            let full_path = config.local.root_dir.join(rel_path);
            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| anyhow::anyhow!("Failed to read local file '{}': {}", rel_path, e))?;
            let info = SourceInfo {
                label: "local".into(),
                root: config.local.root_dir.to_string_lossy().to_string(),
            };
            Ok((content, info))
        }
        Side::Remote(server_name) => {
            if !core.has_client(server_name) {
                core.connect(server_name)?;
            }
            let content = core.read_remote_file(server_name, rel_path)?;
            let server_config = core.get_server_config(server_name)?;
            let info = SourceInfo {
                label: server_name.clone(),
                root: format!(
                    "{}:{}",
                    server_config.host,
                    server_config.root_dir.to_string_lossy()
                ),
            };
            Ok((content, info))
        }
    }
}
