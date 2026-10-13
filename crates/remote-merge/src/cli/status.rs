//! status サブコマンドの実装。
//!
//! CoreRuntime でツリーを取得し、Service 層で比較結果を計算、
//! フォーマッターで出力する。
//!
//! 比較は3段階で行う:
//! 1. ツリー存在チェック → LeftOnly / RightOnly を確定
//! 2. メタデータ比較（size, mtime） → Equal / Modified の事前判定
//! 3. コンテンツ比較（メタデータで判定できないファイルのみ） → 最終判定

use crate::cli::content_reads::read_contents;
use crate::cli::ref_guard;
use crate::config::{resolve_max_entries, AppConfig};
use crate::runtime::size_limit::SizeLimit;
use crate::runtime::{CoreRuntime, RuntimeTargets};
use crate::service::content_reads::{not_absent_in, verified_content_pairs};
use crate::service::other_kinds::{set_aside_other_kinds, OTHER_KIND_REASON};
use crate::service::output::{format_json, format_status_text, OutputFormat};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    build_status_output, compute_status_from_trees, needs_content_compare,
    needs_content_compare_all, paths_without_hashes, refine_status_with_content,
    refine_status_with_hashes, status_exit_code,
};
use crate::service::status_ref_badge::{compute_ref_badges, ref_badge_content_paths};
use crate::service::status_unreadable::{take_unreadable_files, with_unreadable_files, SideReads};
use crate::service::types::FileStatusKind;
use crate::service::types::StatusOutput;
use crate::service::types::UnreadableFile;
use crate::service::unreadable_names::unreadable_names;
use crate::side::Side;

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
    /// 冗長出力レベル（0=通常, 1+=agent 情報を含む）
    pub verbose: u8,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

pub struct StatusCommandResult {
    pub output: StatusOutput,
    pub exit_code: i32,
    pub summary: bool,
}

/// status サブコマンドを実行する
pub fn run_status(args: StatusArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let result = execute_status(args, config, RuntimeTargets::production())?;
    print_status_result(&result, format)?;
    Ok(result.exit_code)
}

pub fn execute_status(
    args: StatusArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<StatusCommandResult> {
    OutputFormat::parse(&args.format)?;

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let max_entries = resolve_max_entries(args.max_entries, &config)?;

    let mut core = CoreRuntime::with_targets(config.clone(), targets);

    // 接続
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // 左側ツリー取得（再帰走査でメタデータ付き）
    let mut left_tree = core.fetch_tree_recursive(&pair.left, max_entries, true)?;
    let left_info = build_source_info(&pair.left, &core)?;

    // 右側ツリー取得（再帰走査でメタデータ付き）
    let mut right_tree = core.fetch_tree_recursive(&pair.right, max_entries, true)?;
    let right_info = build_source_info(&pair.right, &core)?;
    // その他の種類は比べず、判定できないものとして errors に出す
    let other_kinds = set_aside_other_kinds(&[], &mut [&mut left_tree, &mut right_tree]);

    // ステータス計算（メタデータ比較）
    let mut files = compute_status_from_trees(&left_tree, &right_tree);

    // コンテンツ比較が必要なファイルを抽出
    // --checksum: 全ファイルを比較（メタデータ quick check をバイパス）
    let paths_to_compare = if args.checksum {
        needs_content_compare_all(&files, &left_tree, &right_tree)
    } else {
        needs_content_compare(&files, &left_tree, &right_tree)
    };

    // Ref server handling
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);

    // ハッシュ比較を試行（ref 未使用時のみ — ref 使用時はコンテンツも必要）
    // ハッシュ比較が成功すれば全ファイルのコンテンツ転送を回避できる。ハッシュの得られなかった
    // パスは中身を読んで比べ、読めなければ読めなかったファイルとして出す
    let hashes = if ref_side.is_none() && !paths_to_compare.is_empty() {
        // `接続断` で止めたときは、そのサーバの読み込みは行わずに理由付きの失敗になり、
        // 比べるはずだったファイルは理由とともに errors に出る
        core.try_hash_compare(&pair.left, &pair.right, &paths_to_compare)
            .map(Some)
            .unwrap_or_else(|error| {
                tracing::warn!("Hash comparison stopped: {error:#}");
                None
            })
    } else {
        None
    };
    let compare_paths = match &hashes {
        Some((local_hashes, remote_hashes)) => {
            refine_status_with_hashes(&mut files, local_hashes, remote_hashes);
            paths_without_hashes(&paths_to_compare, local_hashes, remote_hashes)
        }
        None => paths_to_compare.clone(),
    };

    // 参照先のツリーを先に取り、バッジに中身の要るファイル（三つともにあるもの）を決める
    let reference = match &ref_side {
        Some(ref_s) => {
            core.connect_if_remote(ref_s)?;
            let ref_tree = core.fetch_tree_recursive(ref_s, max_entries, true)?;
            let info = build_source_info(ref_s, &core)?;
            Some((ref_s, ref_tree, info))
        }
        None => None,
    };
    let badge_paths = reference
        .as_ref()
        .map(|(_, ref_tree, _)| ref_badge_content_paths(&files, &left_tree, &right_tree, ref_tree))
        .unwrap_or_default();

    let mut content_paths = compare_paths.clone();
    content_paths.extend(
        badge_paths
            .iter()
            .filter(|path| !compare_paths.contains(path))
            .cloned(),
    );

    let left_contents = read_contents(
        &mut core,
        &pair.left,
        &not_absent_in(&content_paths, &left_tree),
        SizeLimit::Lifted,
    );
    let right_contents = read_contents(
        &mut core,
        &pair.right,
        &not_absent_in(&content_paths, &right_tree),
        SizeLimit::Lifted,
    );

    // コンテンツ比較で status を精緻化（ハッシュで未解決のものだけ）
    let verified = verified_content_pairs(&compare_paths, &left_contents, &right_contents);
    refine_status_with_content(&mut files, &verified.pairs);

    let mut ref_info = None;
    let mut ref_badges = None;
    let mut ref_contents = None;

    let mut ref_scan = None;

    if let Some((ref_s, ref_tree, info)) = reference {
        ref_info = Some(info);
        let reads = read_contents(&mut core, ref_s, &badge_paths, SizeLimit::Lifted);
        let badges = compute_ref_badges(
            &files,
            &left_tree,
            &right_tree,
            &ref_tree,
            &left_contents,
            &right_contents,
            &reads,
        );
        ref_badges = Some(badges);
        ref_contents = Some((ref_s.display_name().to_string(), reads));
        ref_scan = Some((ref_s.display_name().to_string(), ref_tree));
    }

    // 読めなかったファイルは理由とともに errors に出し、読み比べるはずだったものは判定しない
    let compared_sides: [SideReads<'_>; 2] = [
        (pair.left.display_name(), &left_contents),
        (pair.right.display_name(), &right_contents),
    ];
    let reference_sides: Vec<SideReads<'_>> = ref_contents
        .iter()
        .map(|(name, reads)| (name.as_str(), reads))
        .collect();
    let mut unreadable = take_unreadable_files(
        &mut files,
        &compare_paths,
        &compared_sides,
        &reference_sides,
    );
    // 走査で見つけた UTF-8 として読めない名前のファイルも、読めないファイルとして出す
    let mut scanned = vec![
        (pair.left.display_name(), &left_tree),
        (pair.right.display_name(), &right_tree),
    ];
    if let Some((name, tree)) = &ref_scan {
        scanned.push((name.as_str(), tree));
    }
    unreadable.extend(
        unreadable_names(&scanned)
            .into_iter()
            .map(|(path, reason)| UnreadableFile { path, reason }),
    );
    unreadable.extend(other_kinds.into_iter().map(|path| UnreadableFile {
        path,
        reason: OTHER_KIND_REASON.to_string(),
    }));

    let mut output = with_unreadable_files(
        build_status_output(
            left_info,
            right_info,
            files,
            args.summary,
            ref_info,
            ref_badges.as_ref(),
        ),
        unreadable,
    );

    // Agent 接続状態を設定（verbose 時のみ）
    if args.verbose > 0 {
        output.agent = determine_agent_status(&pair.right, &core);
    }

    let code = status_exit_code(&output.summary);

    // Filter out Equal files unless --all is specified
    filter_equal_files(&mut output, args.all);

    core.disconnect_all();
    Ok(StatusCommandResult {
        output,
        exit_code: code,
        summary: args.summary,
    })
}

fn print_status_result(result: &StatusCommandResult, format: OutputFormat) -> anyhow::Result<()> {
    if format == OutputFormat::Text {
        for error in &result.output.errors {
            eprintln!("Error: {}: {}", error.path, error.reason);
        }
    }
    let text = match format {
        OutputFormat::Text => format_status_text(&result.output, result.summary),
        OutputFormat::Json => format_json(&result.output)?,
    };
    println!("{}", text);
    Ok(())
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
            errors: Vec::new(),
        }
    }

    #[test]
    fn test_determine_agent_status_remote_no_agent() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::side::Side::Remote("develop".to_string());
        let result = super::determine_agent_status(&side, &core);
        assert_eq!(result, Some(crate::service::types::AgentStatus::Fallback));
    }

    #[test]
    fn test_determine_agent_status_local() {
        let core = crate::runtime::CoreRuntime::new_for_test();
        let side = crate::side::Side::Local;
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
