//! diff サブコマンドの実装。

use crate::app::Side;
use crate::cli::ref_guard;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::diff::build_diff_output;
use crate::service::output::{format_json, format_multi_diff_text, OutputFormat};
use crate::service::path_resolver::{filter_changed_files, resolve_target_files_from_statuses};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    compute_status_from_trees, is_sensitive, needs_content_compare, refine_status_with_content,
};
use crate::service::types::{exit_code, MultiDiffOutput, MultiDiffSummary};
use std::collections::HashMap;

/// diff サブコマンドの引数
pub struct DiffArgs {
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub format: String,
    pub max_lines: Option<usize>,
    pub max_files: usize,
}

/// diff サブコマンドを実行する
pub fn run_diff(args: DiffArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let mut core = CoreRuntime::new(config.clone());
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    // Fetch trees and compute status to identify diff files
    let left_tree = core.fetch_tree_recursive(&pair.left, 50_000)?;
    let right_tree = core.fetch_tree_recursive(&pair.right, 50_000)?;
    let left_info = build_source_info(&pair.left, &core)?;
    let right_info = build_source_info(&pair.right, &core)?;

    // Compute statuses first (covers both left and right trees)
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

    // Refine statuses with content comparison for metadata-ambiguous files
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    if !paths_to_compare.is_empty() {
        let mut compare_pairs = HashMap::new();
        for path in &paths_to_compare {
            let (left_content, _) =
                read_file_tolerant(&mut core, &pair.left, path, /* quiet */ true);
            let (right_content, _) =
                read_file_tolerant(&mut core, &pair.right, path, /* quiet */ true);
            compare_pairs.insert(path.clone(), (left_content, right_content));
        }
        refine_status_with_content(&mut statuses, &compare_pairs);
    }

    // Resolve paths to file list using statuses (includes right-only files)
    let target_files =
        resolve_target_files_from_statuses(&args.paths, &statuses, &left_tree, &right_tree)?;

    // Filter to only files with differences (not Equal)
    let diff_files = filter_changed_files(&target_files, &statuses);

    // Apply max-files truncation
    let truncated = args.max_files > 0 && diff_files.len() > args.max_files;
    let total_files = if truncated {
        Some(diff_files.len())
    } else {
        None
    };
    let process_files = if truncated {
        &diff_files[..args.max_files]
    } else {
        &diff_files
    };

    // Ref server handling
    let ref_side = resolve_ref_source(args.ref_server.as_deref(), &config)?;
    let ref_side = ref_guard::validate_ref_side(ref_side, &pair);
    let ref_info_opt = if let Some(ref_s) = &ref_side {
        core.connect_if_remote(ref_s)?;
        Some(build_source_info(ref_s, &core)?)
    } else {
        None
    };

    // Build diff for each file
    let mut file_diffs = Vec::new();
    let mut has_read_error = false;
    for path in process_files {
        let (left_content, left_ok) =
            read_file_tolerant(&mut core, &pair.left, path, /* quiet */ false);
        let (right_content, right_ok) =
            read_file_tolerant(&mut core, &pair.right, path, /* quiet */ false);
        // 両方読めなかったファイルはエラーとして記録
        if !left_ok && !right_ok {
            has_read_error = true;
        }
        let sensitive = is_sensitive(path, &config.filter.sensitive);

        let ref_content = if let Some(ref_s) = &ref_side {
            core.read_file(ref_s, path).ok()
        } else {
            None
        };

        let output = build_diff_output(
            path,
            left_info.clone(),
            right_info.clone(),
            &left_content,
            &right_content,
            sensitive,
            args.max_lines,
            ref_info_opt.clone(),
            ref_content.as_deref(),
        );
        file_diffs.push(output);
    }

    let files_with_changes = file_diffs
        .iter()
        .filter(|d| d.binary || d.symlink || !d.hunks.is_empty())
        .count();
    let multi_output = MultiDiffOutput {
        summary: MultiDiffSummary {
            total_files: diff_files.len(),
            files_with_changes,
        },
        files: file_diffs,
        truncated,
        total_files,
    };

    let code = if has_read_error {
        exit_code::ERROR
    } else if multi_output.summary.files_with_changes > 0 {
        exit_code::DIFF_FOUND
    } else {
        exit_code::SUCCESS
    };

    let text = match format {
        OutputFormat::Text => format_multi_diff_text(&multi_output),
        OutputFormat::Json => format_json(&multi_output)?,
    };
    println!("{}", text);

    core.disconnect_all();
    Ok(code)
}

/// ファイル読み込みを試み、失敗時は空文字列を返す。
///
/// 全エラー（PathNotFound, SSH切断, パーミッション拒否等）を空文字列にフォールバックする。
/// diff は読み取り専用操作であり、片側が読めなくても全行追加/削除として表示できるため、
/// エラー種別による分岐は行わない。
///
/// `quiet` が false の場合、失敗時に stderr に Warning を出力する。
/// content compare フェーズではターゲット外ファイルも含むため quiet=true で抑制し、
/// diff build フェーズではターゲットファイルのみなので quiet=false で警告する。
///
/// 返り値: (コンテンツ, 読み込み成功したか)
fn read_file_tolerant(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    quiet: bool,
) -> (String, bool) {
    match core.read_file(side, path) {
        Ok(content) => (content, true),
        Err(e) => {
            if !quiet {
                eprintln!(
                    "Warning: {}: {}: {:#} (treating as empty)",
                    side.display_name(),
                    path,
                    e
                );
            }
            (String::new(), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::service::path_resolver::filter_changed_files;
    use crate::service::types::{FileStatus, FileStatusKind};

    fn make_status(path: &str, kind: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            status: kind,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }
    }

    #[test]
    fn test_filter_excludes_equal() {
        let targets = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("b.txt", FileStatusKind::Equal),
            make_status("c.txt", FileStatusKind::LeftOnly),
        ];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["a.txt", "c.txt"]);
    }

    #[test]
    fn test_filter_includes_unknown_paths() {
        let targets = vec!["unknown.txt".into()];
        let statuses = vec![];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["unknown.txt"]);
    }
}
