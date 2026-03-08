//! diff サブコマンドの実装。

use crate::app::Side;
use crate::cli::ref_guard;
use crate::config;
use crate::runtime::CoreRuntime;
use crate::service::diff::{build_diff_output, diff_exit_code};
use crate::service::output::{format_diff_text, format_json, OutputFormat};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::is_sensitive;

/// diff サブコマンドの引数
pub struct DiffArgs {
    pub path: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
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

    // 左側コンテンツ取得（ファイルが存在しない場合は空として扱う）
    let left_content = read_file_tolerant(&mut core, &pair.left, &args.path);
    let left_info = build_source_info(&pair.left, &core)?;

    // 右側コンテンツ取得（ファイルが存在しない場合は空として扱う）
    let right_content = read_file_tolerant(&mut core, &pair.right, &args.path);
    let right_info = build_source_info(&pair.right, &core)?;

    let sensitive = is_sensitive(&args.path, &config.filter.sensitive);

    // Ref server handling
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);
    let mut ref_info = None;
    let mut ref_content_opt = None;

    if let Some(ref_s) = &ref_side {
        core.connect_if_remote(ref_s)?;
        ref_info = Some(build_source_info(ref_s, &core)?);
        // Read ref content — file not existing is OK (ref_content = None)
        match core.read_file(ref_s, &args.path) {
            Ok(content) => ref_content_opt = Some(content),
            Err(e) => {
                tracing::debug!("Ref file not found: {}", e);
            }
        }
    }

    // Note: sensitive files get ref_hunks just like main hunks.
    // The `sensitive` flag in output is informational only.
    let output = build_diff_output(
        &args.path,
        left_info,
        right_info,
        &left_content,
        &right_content,
        sensitive,
        args.max_lines,
        ref_info,
        ref_content_opt.as_deref(),
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

/// ファイル読み込みを試み、失敗時は警告を出して空文字列を返す。
///
/// 全エラー（PathNotFound, SSH切断, パーミッション拒否等）を空文字列にフォールバックする。
/// diff は読み取り専用操作であり、片側が読めなくても全行追加/削除として表示できるため、
/// エラー種別による分岐は行わない。警告は eprintln で常にユーザーに通知される。
fn read_file_tolerant(core: &mut CoreRuntime, side: &Side, path: &str) -> String {
    match core.read_file(side, path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "Warning: {}: {}: {:#} (treating as empty)",
                side.display_name(),
                path,
                e
            );
            String::new()
        }
    }
}
