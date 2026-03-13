//! diff サブコマンドの実装。

use crate::app::Side;
use crate::cli::ref_guard;
use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::{resolve_max_entries, AppConfig};
use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
use crate::runtime::CoreRuntime;
use crate::service::diff::{
    build_diff_output, build_masked_diff_output, build_symlink_diff_output,
};
use crate::service::merge::find_symlink_target;
use crate::service::output::{format_json, format_multi_diff_text, OutputFormat};
use crate::service::path_resolver::{
    check_path_traversal, filter_changed_files, partition_existing_files,
    resolve_target_files_from_statuses,
};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    compute_status_from_trees, is_sensitive, needs_content_compare, refine_status_with_content,
    status_from_read_results,
};
use crate::service::types::{
    exit_code, DiffOutput, FileStatus, FileStatusKind, MultiDiffOutput, MultiDiffSummary,
};
use crate::service::{resolve_scan_strategy, ScanStrategy};
use crate::tree::FileTree;
use std::collections::HashMap;

/// diff の ScanStrategy 分岐結果（left_tree, right_tree, statuses, existing_files, diff_files）
type DiffScanResult = (
    FileTree,
    FileTree,
    Vec<FileStatus>,
    Vec<String>,
    Vec<String>,
);

/// diff サブコマンドの引数
pub struct DiffArgs {
    pub paths: Vec<String>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub ref_server: Option<String>,
    pub format: String,
    pub max_lines: Option<usize>,
    pub max_files: usize,
    pub force: bool,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

/// diff サブコマンドを実行する
pub fn run_diff(args: DiffArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let mut core = CoreRuntime::new(config.clone());
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    let left_info = build_source_info(&pair.left, &core)?;
    let right_info = build_source_info(&pair.right, &core)?;

    // ScanStrategy で分岐: FastPath / PartialScan / FullScan
    let strategy = resolve_scan_strategy(&args.paths, false);

    let (left_tree, right_tree, statuses, existing_files, diff_files) = match strategy {
        ScanStrategy::FastPath(ref target_paths) => {
            check_path_traversal(target_paths)?;
            run_diff_fast_path(target_paths, &pair.left, &pair.right, &mut core, &config)?
        }
        ScanStrategy::PartialScan(ref dir_paths) => run_diff_partial_scan(
            dir_paths,
            &args.paths,
            &pair.left,
            &pair.right,
            &mut core,
            &config,
            max_entries,
        )?,
        ScanStrategy::FullScan => run_diff_full_scan(
            &args.paths,
            &pair.left,
            &pair.right,
            &mut core,
            &config,
            max_entries,
        )?,
    };

    // Apply max-files truncation
    let truncated = args.max_files > 0 && diff_files.len() > args.max_files;
    let changed_files_total = if truncated {
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

        let sensitive = is_sensitive(path, &config.filter.sensitive);

        // symlink 判定（ツリー情報から）— sensitive でもターゲットパスは機密情報ではないため先に判定
        let left_symlink_target = find_symlink_target(&left_tree, path);
        let right_symlink_target = find_symlink_target(&right_tree, path);
        if left_symlink_target.is_some() || right_symlink_target.is_some() {
            file_diffs.push(build_symlink_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
                left_symlink_target.as_deref(),
                right_symlink_target.as_deref(),
                sensitive,
            ));
            continue;
        }

        // sensitive マスク（--force なしの場合、内容を読み込まずにマスク）
        if sensitive && !args.force {
            file_diffs.push(build_masked_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
            ));
            continue;
        }

        // バイト列で読み込み、事前にバイナリ判定を行う
        let (left_bytes, left_ok) =
            read_file_bytes_tolerant(&mut core, &pair.left, path, left_quiet);
        let (right_bytes, right_ok) =
            read_file_bytes_tolerant(&mut core, &pair.right, path, right_quiet);
        // 両方読めなかったファイルはエラーとして記録
        if !left_ok && !right_ok {
            has_read_error = true;
        }

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
                note: None,
                conflict_count: 0,
                conflict_regions: vec![],
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
        .filter(|d| {
            d.binary || d.symlink || !d.hunks.is_empty() || (d.sensitive && d.note.is_some())
        })
        .count();
    let multi_output = MultiDiffOutput {
        summary: MultiDiffSummary {
            scanned_files: existing_files.len(),
            files_with_changes,
        },
        files: file_diffs,
        truncated,
        changed_files_total,
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

/// FastPath: 指定ファイルだけ直接読んでステータスを判定する（ツリースキャンなし）。
///
/// 返り値: (left_tree, right_tree, statuses, existing_files, diff_files)
/// ツリーは空（FastPath ではツリーを使わないため）。
fn run_diff_fast_path(
    target_paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    config: &AppConfig,
) -> anyhow::Result<DiffScanResult> {
    let left_tree = FileTree::new(&config.local.root_dir);
    let right_tree = FileTree::new(&config.local.root_dir);

    let mut statuses = Vec::new();
    let mut existing = Vec::new();

    for path in target_paths {
        let (left_bytes, left_ok) = read_file_bytes_tolerant(core, left, path, true);
        let (right_bytes, right_ok) = read_file_bytes_tolerant(core, right, path, true);

        let left_exists = left_ok;
        let right_exists = right_ok;

        let left_content = if left_ok {
            Some(left_bytes.as_slice())
        } else {
            None
        };
        let right_content = if right_ok {
            Some(right_bytes.as_slice())
        } else {
            None
        };

        match status_from_read_results(left_exists, right_exists, left_content, right_content) {
            Ok(kind) => {
                statuses.push(FileStatus {
                    path: path.clone(),
                    status: kind,
                    sensitive: is_sensitive(path, &config.filter.sensitive),
                    hunks: None,
                    ref_badge: None,
                });
                existing.push(path.clone());
            }
            Err(_) => {
                // both missing → warn
                eprintln!("Warning: '{}' not found on either side", path);
            }
        }
    }

    if existing.is_empty() && !target_paths.is_empty() {
        anyhow::bail!("specified path(s) not found on either side");
    }

    let diff_files = filter_changed_files(&existing, &statuses);
    Ok((left_tree, right_tree, statuses, existing, diff_files))
}

/// PartialScan: 指定ディレクトリ配下のみツリー取得して既存フローに接続する。
fn run_diff_partial_scan(
    dir_paths: &[String],
    original_paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    config: &AppConfig,
    max_entries: usize,
) -> anyhow::Result<DiffScanResult> {
    // 各ディレクトリのサブツリーを取得して結合
    let mut left_tree = FileTree::new(&config.local.root_dir);
    let mut right_tree = FileTree::new(&config.local.root_dir);

    for dir_path in dir_paths {
        let lt = core.fetch_tree_for_subpath(left, dir_path, max_entries, true)?;
        let rt = core.fetch_tree_for_subpath(right, dir_path, max_entries, true)?;
        left_tree.nodes.extend(lt.nodes);
        right_tree.nodes.extend(rt.nodes);
    }
    left_tree.sort();
    left_tree.nodes.dedup_by_key(|n| n.name.clone());
    right_tree.sort();
    right_tree.nodes.dedup_by_key(|n| n.name.clone());

    // 以降は FullScan と同じフロー
    compute_statuses_and_resolve(
        original_paths,
        left,
        right,
        core,
        config,
        left_tree,
        right_tree,
    )
}

/// FullScan: 従来通り全ツリーを取得する。
fn run_diff_full_scan(
    paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    config: &AppConfig,
    max_entries: usize,
) -> anyhow::Result<DiffScanResult> {
    let left_tree = core.fetch_tree_recursive(left, max_entries, true)?;
    let right_tree = core.fetch_tree_recursive(right, max_entries, true)?;
    compute_statuses_and_resolve(paths, left, right, core, config, left_tree, right_tree)
}

/// ツリーからステータス計算 → パス解決 → diff ファイルリスト構築（PartialScan / FullScan 共通）
fn compute_statuses_and_resolve(
    paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    config: &AppConfig,
    left_tree: FileTree,
    right_tree: FileTree,
) -> anyhow::Result<DiffScanResult> {
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree, &config.filter.sensitive);

    // Refine statuses with content comparison for metadata-ambiguous files
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    if !paths_to_compare.is_empty() {
        let left_batch = fetch_contents_tolerant(left, &paths_to_compare, core);
        let right_batch = fetch_contents_tolerant(right, &paths_to_compare, core);
        let mut compare_pairs: HashMap<String, (Vec<u8>, Vec<u8>)> = HashMap::new();
        for path in &paths_to_compare {
            let left_bytes = left_batch.get(path).cloned().unwrap_or_default();
            let right_bytes = right_batch.get(path).cloned().unwrap_or_default();
            compare_pairs.insert(path.clone(), (left_bytes, right_bytes));
        }
        refine_status_with_content(&mut statuses, &compare_pairs);
    }

    let target_files =
        resolve_target_files_from_statuses(paths, &statuses, &left_tree, &right_tree)?;

    let (existing_files, missing_files) = partition_existing_files(&target_files, &statuses);
    for path in &missing_files {
        eprintln!("Warning: '{}' not found on either side", path);
    }

    if existing_files.is_empty() && !paths.is_empty() {
        anyhow::bail!("specified path(s) not found on either side");
    }

    let diff_files = filter_changed_files(&existing_files, &statuses);
    Ok((left_tree, right_tree, statuses, existing_files, diff_files))
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

    use crate::service::path_resolver::partition_existing_files;

    #[test]
    fn test_all_missing_triggers_error() {
        // 指定パスが全て status にない → existing_files が空 → エラー
        let target_files = vec!["nonexistent.txt".into()];
        let statuses: Vec<FileStatus> = vec![];
        let (existing, missing) = partition_existing_files(&target_files, &statuses);
        assert!(existing.is_empty());
        assert_eq!(missing, vec!["nonexistent.txt"]);
    }

    #[test]
    fn test_equal_file_is_existing_not_error() {
        // Equal ファイルは存在する（エラーではない）
        let target_files = vec!["a.txt".into()];
        let statuses = vec![make_status("a.txt", FileStatusKind::Equal)];
        let (existing, missing) = partition_existing_files(&target_files, &statuses);
        assert_eq!(existing, vec!["a.txt"]);
        assert!(missing.is_empty());
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

    // ── additional quiet_flags tests ──

    #[test]
    fn test_quiet_flags_equal_no_suppression() {
        // Equal ステータスでは両方 quiet=false
        let (left_quiet, right_quiet) = quiet_flags_for_status(Some(FileStatusKind::Equal));
        assert!(!left_quiet, "left should not be quiet for Equal");
        assert!(!right_quiet, "right should not be quiet for Equal");
    }

    // ── additional filter tests ──

    #[test]
    fn test_filter_all_equal_returns_empty() {
        // 全ファイルが Equal → filter 後は空
        let targets = vec!["a.txt".into(), "b.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Equal),
            make_status("b.txt", FileStatusKind::Equal),
        ];
        let result = filter_changed_files(&targets, &statuses);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_empty_statuses() {
        // statuses が空 → 全ファイルが通過（unknown 扱い）
        let targets = vec!["x.txt".into(), "y.txt".into()];
        let statuses: Vec<FileStatus> = vec![];
        let result = filter_changed_files(&targets, &statuses);
        assert_eq!(result, vec!["x.txt", "y.txt"]);
    }

    // ── additional partition tests ──

    #[test]
    fn test_partition_mixed_files() {
        // 一部存在・一部不在
        let target_files = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("c.txt", FileStatusKind::LeftOnly),
        ];
        let (existing, missing) = partition_existing_files(&target_files, &statuses);
        assert_eq!(existing, vec!["a.txt", "c.txt"]);
        assert_eq!(missing, vec!["b.txt"]);
    }

    #[test]
    fn test_partition_all_existing() {
        // 全ファイルが存在 → missing が空
        let target_files = vec!["a.txt".into(), "b.txt".into()];
        let statuses = vec![
            make_status("a.txt", FileStatusKind::Modified),
            make_status("b.txt", FileStatusKind::RightOnly),
        ];
        let (existing, missing) = partition_existing_files(&target_files, &statuses);
        assert_eq!(existing, vec!["a.txt", "b.txt"]);
        assert!(missing.is_empty());
    }

    // ── ScanStrategy 分岐テスト ──

    use crate::service::{resolve_scan_strategy, ScanStrategy};

    #[test]
    fn test_diff_strategy_single_file_returns_fast_path() {
        let paths = vec!["app/main.rs".to_string()];
        let strategy = resolve_scan_strategy(&paths, false);
        assert_eq!(
            strategy,
            ScanStrategy::FastPath(vec!["app/main.rs".to_string()])
        );
    }

    #[test]
    fn test_diff_strategy_directory_returns_partial_scan() {
        let paths = vec!["app/controllers/".to_string()];
        let strategy = resolve_scan_strategy(&paths, false);
        assert_eq!(
            strategy,
            ScanStrategy::PartialScan(vec!["app/controllers/".to_string()])
        );
    }

    #[test]
    fn test_diff_strategy_dot_returns_full_scan() {
        let paths = vec![".".to_string()];
        let strategy = resolve_scan_strategy(&paths, false);
        assert_eq!(strategy, ScanStrategy::FullScan);
    }

    #[test]
    fn test_diff_strategy_empty_paths_returns_full_scan() {
        let strategy = resolve_scan_strategy(&[], false);
        assert_eq!(strategy, ScanStrategy::FullScan);
    }

    #[test]
    fn test_diff_strategy_glob_returns_full_scan() {
        let paths = vec!["*.php".to_string()];
        let strategy = resolve_scan_strategy(&paths, false);
        assert_eq!(strategy, ScanStrategy::FullScan);
    }

    #[test]
    fn test_diff_strategy_multiple_files_returns_fast_path() {
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let strategy = resolve_scan_strategy(&paths, false);
        assert_eq!(
            strategy,
            ScanStrategy::FastPath(vec!["a.rs".to_string(), "b.rs".to_string()])
        );
    }
}
