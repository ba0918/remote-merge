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
use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    build_status_output, compute_ref_badges, compute_status_from_trees, needs_content_compare,
    needs_content_compare_all, refine_status_with_content, status_exit_code,
};
use crate::service::types::FileStatusKind;

/// 再帰走査の最大エントリ数
const MAX_SCAN_ENTRIES: usize = 50_000;

/// status サブコマンドの引数
pub struct StatusArgs {
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub format: String,
    pub summary: bool,
    pub all: bool,
    /// 全ファイルのコンテンツ比較を強制する（メタデータベースの quick check をバイパス）
    pub checksum: bool,
}

/// status サブコマンドを実行する
pub fn run_status(args: StatusArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;

    let source_args = SourceArgs {
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
    // --checksum: 全ファイルを比較（メタデータ quick check をバイパス）
    let paths_to_compare = if args.checksum {
        needs_content_compare_all(&files)
    } else {
        needs_content_compare(&files, &left_tree, &right_tree)
    };

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
        fetch_contents_tolerant(&pair.left, &content_paths, &mut core)
    } else {
        HashMap::new()
    };
    let right_contents = if !content_paths.is_empty() {
        fetch_contents_tolerant(&pair.right, &content_paths, &mut core)
    } else {
        HashMap::new()
    };

    // コンテンツ比較で status を精緻化（バイト列比較でバイナリも正しく判定）
    if !paths_to_compare.is_empty() {
        let mut compare_pairs: HashMap<String, (Vec<u8>, Vec<u8>)> = HashMap::new();
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
        let ref_contents = fetch_contents_tolerant(ref_s, &ref_paths, &mut core);

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

    let mut output = build_status_output(
        left_info,
        right_info,
        files,
        args.summary,
        ref_info,
        ref_badges.as_ref(),
    );

    // Agent 接続状態を設定
    output.agent = determine_agent_status(&pair.right, &core);

    let code = status_exit_code(&output.summary);

    // Filter out Equal files unless --all is specified
    filter_equal_files(&mut output, args.all);

    // 出力
    let text = match format {
        OutputFormat::Text => format_status_text(&output, args.summary),
        OutputFormat::Json => format_json(&output)?,
    };
    println!("{}", text);

    core.disconnect_all();
    Ok(code)
}

/// 右側ソースの Agent 接続状態を判定する。
///
/// リモートサーバの場合のみ判定し、ローカルの場合は None を返す。
fn determine_agent_status(
    right: &Side,
    core: &CoreRuntime,
) -> Option<crate::service::types::AgentStatus> {
    use crate::service::types::AgentStatus;
    match right.server_name() {
        Some(name) => {
            if core.has_agent(name) {
                Some(AgentStatus::Connected)
            } else {
                Some(AgentStatus::Fallback)
            }
        }
        None => None, // ローカル同士の比較
    }
}

/// StatusOutput から Equal ファイルを除外する。
/// `all` が true の場合はフィルタしない。
fn filter_equal_files(output: &mut crate::service::types::StatusOutput, all: bool) {
    if !all {
        if let Some(ref mut files) = output.files {
            files.retain(|f| f.status != FileStatusKind::Equal);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::service::status::compute_summary;
    use crate::service::types::{FileStatus, FileStatusKind, SourceInfo, StatusOutput};

    fn make_file(path: &str, status: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            status,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }
    }

    fn make_output(files: Vec<FileStatus>) -> StatusOutput {
        let summary = compute_summary(&files);
        StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: "/tmp".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "dev:/app".into(),
            },
            ref_: None,
            agent: None,
            files: Some(files),
            summary,
        }
    }

    #[test]
    fn test_equal_files_excluded_by_default() {
        let mut output = make_output(vec![
            make_file("a.txt", FileStatusKind::Modified),
            make_file("b.txt", FileStatusKind::Equal),
            make_file("c.txt", FileStatusKind::LeftOnly),
        ]);
        super::filter_equal_files(&mut output, false);
        let files = output.files.unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.status != FileStatusKind::Equal));
    }

    #[test]
    fn test_all_flag_includes_equal_files() {
        let mut output = make_output(vec![
            make_file("a.txt", FileStatusKind::Modified),
            make_file("b.txt", FileStatusKind::Equal),
        ]);
        super::filter_equal_files(&mut output, true);
        assert_eq!(output.files.unwrap().len(), 2);
    }

    #[test]
    fn test_determine_agent_status_remote_no_agent() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::app::Side::Remote("develop".to_string());
        let result = super::determine_agent_status(&side, &core);
        assert_eq!(result, Some(crate::service::types::AgentStatus::Fallback));
    }

    #[test]
    fn test_determine_agent_status_local() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::app::Side::Local;
        let result = super::determine_agent_status(&side, &core);
        assert_eq!(result, None);
    }

    #[test]
    fn test_summary_equal_count_preserved_after_filter() {
        let mut output = make_output(vec![
            make_file("a.txt", FileStatusKind::Modified),
            make_file("b.txt", FileStatusKind::Equal),
            make_file("c.txt", FileStatusKind::Equal),
        ]);
        // summary は filter 前に計算されるので equal=2 のまま
        assert_eq!(output.summary.equal, 2);
        super::filter_equal_files(&mut output, false);
        assert_eq!(output.files.unwrap().len(), 1);
        // summary は変わらない
        assert_eq!(output.summary.equal, 2);
    }
}
