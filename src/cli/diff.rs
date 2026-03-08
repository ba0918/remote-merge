//! diff サブコマンドの実装。

use crate::config;
use crate::runtime::CoreRuntime;
use crate::service::diff::{build_diff_output, diff_exit_code};
use crate::service::output::{format_diff_text, format_json, OutputFormat};
use crate::service::source_pair::{build_source_info, resolve_source_pair, SourceArgs};
use crate::service::status::is_sensitive;

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

    // 接続
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // 左側コンテンツ取得
    let left_content = core.read_file(&pair.left, &args.path)?;
    let left_info = build_source_info(&pair.left, &core)?;

    // 右側コンテンツ取得
    let right_content = core.read_file(&pair.right, &args.path)?;
    let right_info = build_source_info(&pair.right, &core)?;

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
