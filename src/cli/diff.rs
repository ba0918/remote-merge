//! diff サブコマンドの実装。

use crate::app::Side;
use crate::cli::ref_guard;
use crate::config::AppConfig;
use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
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
use crate::service::types::{
    exit_code, DiffOutput, FileStatusKind, MultiDiffOutput, MultiDiffSummary,
};
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
    // バイト列比較でバイナリファイルも正しく判定する
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    if !paths_to_compare.is_empty() {
        let mut compare_pairs: HashMap<String, (Vec<u8>, Vec<u8>)> = HashMap::new();
        for path in &paths_to_compare {
            let (left_bytes, _) =
                read_file_bytes_tolerant(&mut core, &pair.left, path, /* quiet */ true);
            let (right_bytes, _) =
                read_file_bytes_tolerant(&mut core, &pair.right, path, /* quiet */ true);
            compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
        }
        refine_status_with_content(&mut statuses, &compare_pairs);
    }

    // Resolve paths to file list using statuses (includes right-only files)
    let target_files =
        resolve_target_files_from_statuses(&args.paths, &statuses, &left_tree, &right_tree)?;

    // Filter to only files with differences (not Equal)
    let diff_files = filter_changed_files(&target_files, &statuses);

    // 指定パスがどちらにも存在しない場合はエラー
    if diff_files.is_empty() && target_files.is_empty() {
        eprintln!("Error: specified path(s) not found on either side");
        core.disconnect_all();
        return Ok(exit_code::ERROR);
    }

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
        // LeftOnly/RightOnly の場合、存在しない側の読み込み失敗は予想通りなので Warning を抑制
        let status = statuses.iter().find(|s| s.path == *path).map(|s| s.status);
        let (left_quiet, right_quiet) = quiet_flags_for_status(status);

        // バイト列で読み込み、事前にバイナリ判定を行う
        let (left_bytes, left_ok) =
            read_file_bytes_tolerant(&mut core, &pair.left, path, left_quiet);
        let (right_bytes, right_ok) =
            read_file_bytes_tolerant(&mut core, &pair.right, path, right_quiet);
        // 両方読めなかったファイルはエラーとして記録
        if !left_ok && !right_ok {
            has_read_error = true;
        }
        let sensitive = is_sensitive(path, &config.filter.sensitive);

        let output = if is_binary(&left_bytes) || is_binary(&right_bytes) {
            // バイナリファイル: SHA-256 ハッシュを計算して直接 DiffOutput を構築
            let left_hash = if !left_bytes.is_empty() || left_ok {
                Some(compute_sha256(&left_bytes))
            } else {
                None
            };
            let right_hash = if !right_bytes.is_empty() || right_ok {
                Some(compute_sha256(&right_bytes))
            } else {
                None
            };

            // ref server 指定時のバイナリ diff では ref_hunks を None にする
            let (ref_info_out, ref_hunks_out) = if let Some(ri) = ref_info_opt.clone() {
                (Some(ri), None)
            } else {
                (None, None)
            };

            DiffOutput {
                path: path.to_string(),
                left: left_info.clone(),
                right: right_info.clone(),
                ref_: ref_info_out,
                sensitive,
                binary: true,
                symlink: false,
                truncated: false,
                hunks: vec![],
                ref_hunks: ref_hunks_out,
                left_hash,
                right_hash,
            }
        } else {
            // テキストファイル: String に変換して既存の build_diff_output を呼ぶ
            let left_content = String::from_utf8_lossy(&left_bytes).into_owned();
            let right_content = String::from_utf8_lossy(&right_bytes).into_owned();

            let ref_content = if let Some(ref_s) = &ref_side {
                core.read_file(ref_s, path).ok()
            } else {
                None
            };

            build_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
                &left_content,
                &right_content,
                sensitive,
                args.max_lines,
                ref_info_opt.clone(),
                ref_content.as_deref(),
            )
        };
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

/// LeftOnly/RightOnly に基づき、存在しない側の quiet フラグを決定する。
///
/// 返り値: (left_quiet, right_quiet)
fn quiet_flags_for_status(status: Option<FileStatusKind>) -> (bool, bool) {
    let left_quiet = status == Some(FileStatusKind::RightOnly);
    let right_quiet = status == Some(FileStatusKind::LeftOnly);
    (left_quiet, right_quiet)
}

/// バイト列でファイル読み込みを試み、失敗時は空バイト列を返す。
///
/// 全エラー（PathNotFound, SSH切断, パーミッション拒否等）を空バイト列にフォールバックする。
/// diff は読み取り専用操作であり、片側が読めなくても全行追加/削除として表示できるため、
/// エラー種別による分岐は行わない。
///
/// バイト列で返すことで、バイナリファイルの NUL バイトが lossy 変換で消えることを防ぐ。
/// `is_binary()` 判定が正しく動作し、テキストファイルは呼び出し側で String に変換する。
///
/// `quiet` が false の場合、失敗時に stderr に Warning を出力する。
///
/// 返り値: (バイト列, 読み込み成功したか)
fn read_file_bytes_tolerant(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    quiet: bool,
) -> (Vec<u8>, bool) {
    match core.read_file_bytes(side, path, false) {
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
            (Vec::new(), false)
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

    // ── both-sides-missing detection ──

    #[test]
    fn test_both_empty_means_not_found() {
        // target_files も diff_files も空 = 指定パスがどちらにもない
        let diff_files: Vec<String> = vec![];
        let target_files: Vec<String> = vec![];
        assert!(
            diff_files.is_empty() && target_files.is_empty(),
            "both empty should trigger error path"
        );
    }

    #[test]
    fn test_target_files_present_but_no_diff_is_not_error() {
        // target_files があるが diff_files が空 = Equal ファイルのみ（エラーではない）
        let diff_files: Vec<String> = vec![];
        let target_files = ["a.txt".to_string()];
        assert!(
            !(diff_files.is_empty() && target_files.is_empty()),
            "should not trigger error when target_files is non-empty"
        );
    }

    // ── quiet flags for status ──

    use super::quiet_flags_for_status;

    #[test]
    fn test_quiet_flags_left_only_suppresses_right_warning() {
        let (left_quiet, right_quiet) = quiet_flags_for_status(Some(FileStatusKind::LeftOnly));
        assert!(!left_quiet, "left side should not be quiet for LeftOnly");
        assert!(right_quiet, "right side should be quiet for LeftOnly");
    }

    #[test]
    fn test_quiet_flags_right_only_suppresses_left_warning() {
        let (left_quiet, right_quiet) = quiet_flags_for_status(Some(FileStatusKind::RightOnly));
        assert!(left_quiet, "left side should be quiet for RightOnly");
        assert!(!right_quiet, "right side should not be quiet for RightOnly");
    }

    #[test]
    fn test_quiet_flags_modified_no_suppression() {
        let (left_quiet, right_quiet) = quiet_flags_for_status(Some(FileStatusKind::Modified));
        assert!(!left_quiet);
        assert!(!right_quiet);
    }

    #[test]
    fn test_quiet_flags_none_no_suppression() {
        let (left_quiet, right_quiet) = quiet_flags_for_status(None);
        assert!(!left_quiet);
        assert!(!right_quiet);
    }
}
