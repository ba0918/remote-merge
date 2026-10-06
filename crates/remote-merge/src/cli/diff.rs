//! diff サブコマンドの実装。

use crate::cli::ref_guard;
use crate::cli::tolerant_io::{fetch_contents_required, read_failure_reason, read_unless_missing};
use crate::config::{resolve_max_entries, AppConfig};
use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
use crate::runtime::size_limit::{FileTooLarge, SizeLimit};
use crate::runtime::target_io::TargetPath;
use crate::runtime::{CoreRuntime, RuntimeTargets};
use crate::service::diff::{
    build_diff_output, build_symlink_diff_output, build_uncompared_diff_output,
};
use crate::service::max_files::{changes_without_reading, limit_changed_files, ChangedFileBudget};
use crate::service::merge::find_symlink_target;
use crate::service::output::{format_json, format_multi_diff_text, OutputFormat};
use crate::service::path_resolver::{
    check_path_arguments, filter_changed_files, partition_existing_files,
    resolve_target_files_from_statuses,
};
use crate::service::source_pair::{
    build_source_info, resolve_ref_source, resolve_source_pair, SourceArgs,
};
use crate::service::status::{
    compute_status_from_trees, needs_content_compare, refine_status_with_content,
    status_from_read_results, verified_content_pairs,
};
use crate::service::types::{
    exit_code, DiffError, DiffOutput, FileStatus, FileStatusKind, LinkTargets, MultiDiffOutput,
    MultiDiffSummary,
};
use crate::service::{resolve_scan_strategy, ScanStrategy};
use crate::side::Side;
use crate::tree::{FileNode, FileTree};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// diff の ScanStrategy 分岐結果（left_tree, right_tree, statuses, existing_files, diff_files,
/// plain_file_sizes, read_errors）。plain_file_sizes は diff_files のうち、中身を読む前に通常の
/// ファイルと分かっているパスの左右の大きさ（ない側は None）。--max-files の枠が埋まった後に
/// 読まずに数えてよいかの判定に使う。read_errors は名前で指定したファイルの状態を決めるために
/// 中身を読めなかったパスで、diff_files には入れない。
type DiffScanResult = (
    FileTree,
    FileTree,
    Vec<FileStatus>,
    Vec<String>,
    Vec<String>,
    PlainFileSizes,
    Vec<DiffError>,
);

/// パスごとの、通常のファイルとしてある側の大きさ（左, 右）
type PlainFileSizes = HashMap<String, (Option<u64>, Option<u64>)>;

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
    pub follow_external_links: bool,
    /// スキャン最大エントリ数（1–1,000,000）。config の max_scan_entries を上書きする。
    pub max_entries: Option<usize>,
}

/// diff サブコマンドを実行する
pub fn run_diff(args: DiffArgs, config: AppConfig) -> anyhow::Result<i32> {
    run_diff_with_targets(args, config, RuntimeTargets::production())
}

pub fn run_diff_with_targets(
    args: DiffArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let (multi_output, code) = execute_diff(args, config, targets)?;
    let text = match format {
        OutputFormat::Text => {
            for error in &multi_output.errors {
                eprintln!("Error: {}: {}", error.path, error.reason);
            }
            format_multi_diff_text(&multi_output)
        }
        OutputFormat::Json => format_json(&multi_output)?,
    };
    println!("{}", text);
    Ok(code)
}

pub fn execute_diff(
    mut args: DiffArgs,
    config: AppConfig,
    targets: RuntimeTargets,
) -> anyhow::Result<(MultiDiffOutput, i32)> {
    OutputFormat::parse(&args.format)?;
    let max_entries = resolve_max_entries(args.max_entries, &config)?;
    check_path_arguments(&args.paths)?;
    for path in &mut args.paths {
        if !path.trim_end_matches('/').is_empty() {
            *path = path.trim_end_matches('/').to_string();
        } else if path == "./" {
            *path = ".".into();
        }
    }

    let source_args = SourceArgs {
        left: args.left,
        right: args.right,
    };
    let pair = resolve_source_pair(&source_args, &config)?;

    let mut core = CoreRuntime::with_targets(config.clone(), targets);
    core.connect_if_remote(&pair.left)?;
    core.connect_if_remote(&pair.right)?;

    let left_info = build_source_info(&pair.left, &core)?;
    let right_info = build_source_info(&pair.right, &core)?;
    let left_root = real_root_dir(&mut core, &pair.left, &config)?;
    let right_root = real_root_dir(&mut core, &pair.right, &config)?;

    // ScanStrategy で分岐: FastPath / PartialScan / FullScan
    let strategy = resolve_scan_strategy(&args.paths, false);
    let limit = SizeLimit::from_force(args.force);

    let (left_tree, right_tree, _, existing_files, diff_files, plain_file_sizes, scan_errors) =
        match strategy {
            ScanStrategy::FastPath(ref target_paths) => run_diff_fast_path(
                target_paths,
                (&pair.left, &left_root),
                (&pair.right, &right_root),
                &mut core,
                &config,
                args.follow_external_links,
                limit,
            )?,
            ScanStrategy::PartialScan(ref dir_paths) => run_diff_partial_scan(
                dir_paths,
                &args.paths,
                &pair.left,
                &pair.right,
                &mut core,
                &config,
                max_entries,
                limit,
            )?,
            ScanStrategy::FullScan => run_diff_full_scan(
                &args.paths,
                &pair.left,
                &pair.right,
                &mut core,
                max_entries,
                limit,
            )?,
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
    let mut has_read_error = !scan_errors.is_empty();
    let mut errors = scan_errors;
    let mut pending: VecDeque<String> = diff_files.iter().cloned().collect();
    let mut scanned_files = existing_files.len();
    let mut visited_entries = 0;
    let mut expanded_children = HashSet::new();
    let mut active_directories: HashMap<String, (Vec<PathBuf>, Vec<PathBuf>)> = HashMap::new();
    // 展開した子パスごとに、親がディレクトリでないと分かっている側か。その側にはその子がない。
    // 一覧に失敗しただけの側（読む権限がない、接続の失敗）は当てず、その側でも子を読む
    let mut absent_sides: HashMap<String, (bool, bool)> = HashMap::new();
    // 中身を読まなくても変更のあるファイルと分かるもの
    let known_changed_files: HashSet<&str> = plain_file_sizes
        .iter()
        .filter(|(_, (left_size, right_size))| changes_without_reading(*left_size, *right_size))
        .map(|(path, _)| path.as_str())
        .collect();
    let mut budget = ChangedFileBudget::new(args.max_files);
    while let Some(owned_path) = pending.pop_front() {
        let path = &owned_path;
        let (left_absent, right_absent) = absent_sides.get(path).copied().unwrap_or_default();
        if known_changed_files.contains(path.as_str())
            && !expanded_children.contains(path)
            && budget.skip_unread(&file_diffs)
        {
            continue;
        }
        if !args.follow_external_links
            && ((!left_absent && path_escapes_root(&mut core, &pair.left, &left_root, path)?)
                || (!right_absent && path_escapes_root(&mut core, &pair.right, &right_root, path)?))
        {
            let reason = "content not compared (outside root_dir; use --follow-external-links)";
            let mut output =
                build_uncompared_diff_output(path, left_info.clone(), right_info.clone(), reason);
            let left_target =
                link_target_unless_absent(&mut core, &pair.left, &left_tree, path, left_absent)?;
            let right_target =
                link_target_unless_absent(&mut core, &pair.right, &right_tree, path, right_absent)?;
            if left_target.is_some() || right_target.is_some() {
                output.symlink = true;
                output.link_targets = Some(LinkTargets {
                    left: left_target,
                    right: right_target,
                });
            }
            errors.push(DiffError {
                path: path.clone(),
                reason: reason.into(),
            });
            has_read_error = true;
            file_diffs.push(output);
            continue;
        }
        // symlink 判定（ツリー情報から）
        let left_symlink_target =
            link_target_unless_absent(&mut core, &pair.left, &left_tree, path, left_absent)?;
        let right_symlink_target =
            link_target_unless_absent(&mut core, &pair.right, &right_tree, path, right_absent)?;
        if left_symlink_target.is_none() && right_symlink_target.is_none() {
            let left_children = listed_children(&mut core, &pair.left, path, left_absent);
            let right_children = listed_children(&mut core, &pair.right, path, right_absent);
            if are_empty_directories(&left_children, &right_children) {
                scanned_files = scanned_files.saturating_sub(1);
                continue;
            }
            let has_children = |children: &Option<Vec<FileNode>>| {
                children.as_ref().is_some_and(|nodes| !nodes.is_empty())
            };
            if has_children(&left_children) || has_children(&right_children) {
                let (mut left_ancestors, mut right_ancestors) =
                    active_directories.remove(path).unwrap_or_default();
                let left_real = real_path_unless_absent(&mut core, &pair.left, path, left_absent)?;
                let right_real =
                    real_path_unless_absent(&mut core, &pair.right, path, right_absent)?;
                if left_real
                    .as_ref()
                    .is_some_and(|real| left_ancestors.contains(real))
                    || right_real
                        .as_ref()
                        .is_some_and(|real| right_ancestors.contains(real))
                {
                    let reason = "directory comparison incomplete (symlink cycle)";
                    errors.push(DiffError {
                        path: path.clone(),
                        reason: reason.into(),
                    });
                    has_read_error = true;
                    continue;
                }
                left_ancestors.extend(left_real);
                right_ancestors.extend(right_real);
                scanned_files = scanned_files.saturating_sub(1);
                let left_below_absent = left_absent
                    || left_children.is_none() && reads_as_file(&mut core, &pair.left, path, limit);
                let right_below_absent = right_absent
                    || right_children.is_none()
                        && reads_as_file(&mut core, &pair.right, path, limit);
                let children: BTreeSet<_> = left_children
                    .iter()
                    .flatten()
                    .chain(right_children.iter().flatten())
                    .map(|node| node.name.as_str())
                    .collect();
                for child in children {
                    visited_entries += 1;
                    if visited_entries > max_entries {
                        let reason = "directory comparison incomplete (entry limit exceeded)";
                        errors.push(DiffError {
                            path: path.clone(),
                            reason: reason.into(),
                        });
                        has_read_error = true;
                        break;
                    }
                    let child_path = format!("{path}/{child}");
                    expanded_children.insert(child_path.clone());
                    pending.push_back(child_path.clone());
                    absent_sides
                        .insert(child_path.clone(), (left_below_absent, right_below_absent));
                    active_directories.insert(
                        child_path,
                        (left_ancestors.clone(), right_ancestors.clone()),
                    );
                    scanned_files += 1;
                }
                continue;
            }
        }
        if left_symlink_target.is_some() || right_symlink_target.is_some() {
            let mut link_diff = build_symlink_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
                left_symlink_target.as_deref(),
                right_symlink_target.as_deref(),
            );
            let mut inspection_error = None;
            let external = [(&pair.left, &left_root), (&pair.right, &right_root)]
                .into_iter()
                .zip([left_absent, right_absent])
                .filter(|(_, absent)| !absent)
                .any(|((side, root), _)| match core.inspect_path(side, path) {
                    Ok(TargetPath::Symlink { real_path, .. }) => !real_path.starts_with(root),
                    Ok(_) => false,
                    Err(error) => {
                        inspection_error = Some(error.to_string());
                        true
                    }
                });
            if let Some(reason) = inspection_error {
                errors.push(DiffError {
                    path: path.clone(),
                    reason,
                });
                has_read_error = true;
                file_diffs.push(link_diff);
                continue;
            }
            if external && !args.follow_external_links {
                let reason = "content not compared (outside root_dir; use --follow-external-links)";
                link_diff.note = Some(reason.into());
                errors.push(DiffError {
                    path: path.clone(),
                    reason: reason.into(),
                });
                has_read_error = true;
                file_diffs.push(link_diff);
                continue;
            }
            let left_content = if left_absent {
                Ok(None)
            } else {
                read_existing_diff_file(&mut core, &pair.left, path, limit)
            };
            let right_content = if right_absent {
                Ok(None)
            } else {
                read_existing_diff_file(&mut core, &pair.right, path, limit)
            };
            let left_children = left_content
                .as_ref()
                .err()
                .and_then(|_| core.fetch_children(&pair.left, path).ok());
            let right_children = right_content
                .as_ref()
                .err()
                .and_then(|_| core.fetch_children(&pair.right, path).ok());
            if left_children.is_some() || right_children.is_some() {
                if left_symlink_target.is_some() != right_symlink_target.is_some() {
                    link_diff.note = Some("type mismatch: symlink vs directory".into());
                }
                if (left_content.is_err() && left_children.is_none())
                    || (right_content.is_err() && right_children.is_none())
                {
                    // 読めない側があっても、もう一方の側の子の差分は示す
                    let note = "symlink target unreadable; resolved content not compared";
                    link_diff.note = Some(note.into());
                    let cause = read_failure_reason(
                        (
                            &pair.left,
                            left_content
                                .as_ref()
                                .err()
                                .filter(|_| left_children.is_none()),
                        ),
                        (
                            &pair.right,
                            right_content
                                .as_ref()
                                .err()
                                .filter(|_| right_children.is_none()),
                        ),
                    );
                    errors.push(DiffError {
                        path: path.clone(),
                        reason: format!("{note}: {cause}"),
                    });
                    has_read_error = true;
                }
                let (mut left_ancestors, mut right_ancestors) =
                    active_directories.remove(path).unwrap_or_default();
                let left_real = real_path_unless_absent(&mut core, &pair.left, path, left_absent)?;
                let right_real =
                    real_path_unless_absent(&mut core, &pair.right, path, right_absent)?;
                if left_real
                    .as_ref()
                    .is_some_and(|real| left_ancestors.contains(real))
                    || right_real
                        .as_ref()
                        .is_some_and(|real| right_ancestors.contains(real))
                {
                    let reason = "directory comparison incomplete (symlink cycle)";
                    errors.push(DiffError {
                        path: path.clone(),
                        reason: reason.into(),
                    });
                    link_diff.note = Some(reason.into());
                    has_read_error = true;
                    file_diffs.push(link_diff);
                    continue;
                }
                left_ancestors.extend(left_real);
                right_ancestors.extend(right_real);
                // 既にないものとした側、ファイルとして読めた側、
                // 参照先を読めないリンクとして誤りを記録した側だけに子がない
                let left_below_absent = left_absent
                    || matches!(left_content, Ok(Some(_)))
                    || left_symlink_target.is_some()
                        && left_content.is_err()
                        && left_children.is_none();
                let right_below_absent = right_absent
                    || matches!(right_content, Ok(Some(_)))
                    || right_symlink_target.is_some()
                        && right_content.is_err()
                        && right_children.is_none();
                let children: BTreeSet<_> = left_children
                    .iter()
                    .flatten()
                    .chain(right_children.iter().flatten())
                    .map(|node| node.name.as_str())
                    .collect();
                for child in children {
                    visited_entries += 1;
                    if visited_entries > max_entries {
                        let reason = "directory comparison incomplete (entry limit exceeded)";
                        errors.push(DiffError {
                            path: path.clone(),
                            reason: reason.into(),
                        });
                        link_diff.note = Some(reason.into());
                        has_read_error = true;
                        break;
                    }
                    let child_path = format!("{path}/{child}");
                    expanded_children.insert(child_path.clone());
                    pending.push_back(child_path.clone());
                    absent_sides.insert(child_path, (left_below_absent, right_below_absent));
                    active_directories.insert(
                        format!("{path}/{child}"),
                        (left_ancestors.clone(), right_ancestors.clone()),
                    );
                    scanned_files += 1;
                }
                let left_bytes = left_content.ok().flatten();
                let right_bytes = right_content.ok().flatten();
                if left_bytes.as_ref().is_some_and(|bytes| is_binary(bytes))
                    || right_bytes.as_ref().is_some_and(|bytes| is_binary(bytes))
                {
                    link_diff.binary = true;
                    link_diff.left_hash = left_bytes.as_ref().map(|bytes| compute_sha256(bytes));
                    link_diff.right_hash = right_bytes.as_ref().map(|bytes| compute_sha256(bytes));
                } else if left_bytes.is_some() || right_bytes.is_some() {
                    let left_text =
                        String::from_utf8_lossy(left_bytes.as_deref().unwrap_or_default());
                    let right_text =
                        String::from_utf8_lossy(right_bytes.as_deref().unwrap_or_default());
                    let content_diff = build_diff_output(
                        path,
                        left_info.clone(),
                        right_info.clone(),
                        &left_text,
                        &right_text,
                        args.max_lines,
                        None,
                        None,
                    );
                    link_diff.hunks = content_diff.hunks;
                    link_diff.truncated = content_diff.truncated;
                }
                file_diffs.push(link_diff);
                continue;
            }
            if left_content.is_err() || right_content.is_err() {
                let note = "symlink target unreadable; resolved content not compared";
                link_diff.note = Some(note.into());
                let cause = read_failure_reason(
                    (&pair.left, left_content.as_ref().err()),
                    (&pair.right, right_content.as_ref().err()),
                );
                errors.push(DiffError {
                    path: path.clone(),
                    reason: format!("{note}: {cause}"),
                });
                has_read_error = true;
                file_diffs.push(link_diff);
                continue;
            }
            let (left_content, right_content) = (left_content?, right_content?);
            if left_content.as_ref().is_some_and(|bytes| is_binary(bytes))
                || right_content.as_ref().is_some_and(|bytes| is_binary(bytes))
            {
                link_diff.binary = true;
                link_diff.left_hash = left_content.as_ref().map(|bytes| compute_sha256(bytes));
                link_diff.right_hash = right_content.as_ref().map(|bytes| compute_sha256(bytes));
                file_diffs.push(link_diff);
                continue;
            }
            let left_text = String::from_utf8_lossy(left_content.as_deref().unwrap_or_default());
            let right_text = String::from_utf8_lossy(right_content.as_deref().unwrap_or_default());
            let content_diff = build_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
                &left_text,
                &right_text,
                args.max_lines,
                None,
                None,
            );
            link_diff.hunks = content_diff.hunks;
            link_diff.truncated = content_diff.truncated;
            file_diffs.push(link_diff);
            continue;
        }

        // バイト列で読み込み、事前にバイナリ判定を行う。読めない側は空とみなさず誤りにする
        let left_read = if left_absent {
            Ok(None)
        } else {
            read_unless_missing(&mut core, &pair.left, path, limit)
        };
        let right_read = if right_absent {
            Ok(None)
        } else {
            read_unless_missing(&mut core, &pair.right, path, limit)
        };
        let (left_read, right_read) = match (left_read, right_read) {
            (Ok(None), Ok(None)) => {
                errors.push(DiffError {
                    path: path.clone(),
                    reason: "not found on either side".into(),
                });
                has_read_error = true;
                continue;
            }
            (Ok(left_read), Ok(right_read)) => (left_read, right_read),
            (left_read, right_read) => {
                errors.push(DiffError {
                    path: path.clone(),
                    reason: read_failure_reason(
                        (&pair.left, left_read.as_ref().err()),
                        (&pair.right, right_read.as_ref().err()),
                    ),
                });
                has_read_error = true;
                continue;
            }
        };
        let (left_ok, right_ok) = (left_read.is_some(), right_read.is_some());
        let left_bytes = left_read.unwrap_or_default();
        let right_bytes = right_read.unwrap_or_default();

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
                binary: true,
                symlink: false,
                link_targets: None,
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
                core.read_file(ref_s, path, limit).ok()
            } else {
                None
            };

            build_diff_output(
                path,
                left_info.clone(),
                right_info.clone(),
                &left_content,
                &right_content,
                args.max_lines,
                ref_info_opt.clone(),
                ref_content.as_deref(),
            )
        };
        if !expanded_children.contains(path)
            || output.binary && output.left_hash != output.right_hash
            || !output.hunks.is_empty()
            || output.note.is_some()
        {
            file_diffs.push(output);
        }
    }

    // --max-files はディレクトリを展開した後の変更のあるファイルを数える
    let limited = limit_changed_files(file_diffs, args.max_files, budget.unread());
    let files_with_changes = limited.files.iter().filter(|d| d.has_changes()).count();
    let multi_output = MultiDiffOutput {
        summary: MultiDiffSummary {
            scanned_files,
            files_with_changes,
        },
        files: limited.files,
        errors,
        truncated: limited.changed_files_total.is_some(),
        changed_files_total: limited.changed_files_total,
    };

    let code = if has_read_error {
        exit_code::ERROR
    } else if multi_output.summary.files_with_changes > 0 {
        exit_code::DIFF_FOUND
    } else {
        exit_code::SUCCESS
    };

    core.disconnect_all();
    Ok((multi_output, code))
}

/// 左右とも子の一覧が取れて（ディレクトリで）どちらにも子がないか。
///
/// 子の一覧は include で絞った後のため、include の外のディレクトリもここに当たる。
fn are_empty_directories(left: &Option<Vec<FileNode>>, right: &Option<Vec<FileNode>>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left.is_empty() && right.is_empty())
}

/// 子の一覧。取れない側と、子を持てない側の下に展開したパスは None。
fn listed_children(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    absent: bool,
) -> Option<Vec<FileNode>> {
    if absent {
        return None;
    }
    core.fetch_children(side, path).ok()
}

/// 一覧の取れなかった側の中身を読めるか。読めればディレクトリでなく通常ファイルで、子を持てない。
/// 大きさの上限で読まなかったものも、大きさの分かる通常ファイルなので子を持てない
fn reads_as_file(core: &mut CoreRuntime, side: &Side, path: &str, limit: SizeLimit) -> bool {
    match core.read_file_bytes(side, path, limit == SizeLimit::Lifted) {
        Ok(_) => true,
        Err(error) => error.is::<FileTooLarge>(),
    }
}

/// 子を持てない側の下に展開したパスは、その側を調べずにないものとする。
fn link_target_unless_absent(
    core: &mut CoreRuntime,
    side: &Side,
    tree: &FileTree,
    path: &str,
    absent: bool,
) -> anyhow::Result<Option<String>> {
    if absent {
        return Ok(None);
    }
    link_target_for_diff(core, side, tree, path)
}

/// 子を持てない側の下に展開したパスは、その側を調べずに実パスなしとする。
fn real_path_unless_absent(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    absent: bool,
) -> anyhow::Result<Option<PathBuf>> {
    if absent {
        return Ok(None);
    }
    Ok(inspected_real_path(core.inspect_path(side, path)?))
}

/// 指定したパスのうち、比べるファイルを一つも解決せず、左右ともディレクトリで
/// 配下を include で絞った一覧が空のもの。ツリーに現れない include の外のディレクトリも含む。
fn directories_without_files(
    paths: &[String],
    existing_files: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
) -> HashSet<String> {
    let mut directories = HashSet::new();
    for path in paths {
        let normalized = path.trim_end_matches('/');
        let prefix = format!("{normalized}/");
        // ルートの指定は一覧で確かめず、ツリーから解決した結果だけで決める
        if normalized.is_empty()
            || normalized == "."
            || normalized.contains(['*', '?', '['])
            || existing_files
                .iter()
                .any(|file| file == normalized || file.starts_with(&prefix))
        {
            continue;
        }
        let left_children = core.fetch_children(left, normalized).ok();
        let right_children = core.fetch_children(right, normalized).ok();
        if are_empty_directories(&left_children, &right_children) {
            directories.insert(path.clone());
        }
    }
    directories
}

fn inspected_real_path(path: TargetPath) -> Option<PathBuf> {
    match path {
        TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => Some(real_path),
        TargetPath::Missing { .. } => None,
    }
}

/// FastPath: 指定ファイルだけ直接読んでステータスを判定する（ツリースキャンなし）。
///
/// 返り値: (left_tree, right_tree, statuses, existing_files, diff_files, plain_file_sizes,
/// read_errors)。ツリーは空（FastPath ではツリーを使わないため）。plain_file_sizes は読んだ大きさから作る。
///
/// `left`・`right` は比べる側と、その root_dir を実パスに直したもの。
fn run_diff_fast_path(
    target_paths: &[String],
    (left, left_root): (&Side, &Path),
    (right, right_root): (&Side, &Path),
    core: &mut CoreRuntime,
    config: &AppConfig,
    follow_external_links: bool,
    limit: SizeLimit,
) -> anyhow::Result<DiffScanResult> {
    let left_tree = FileTree::new(&config.local.root_dir);
    let right_tree = FileTree::new(&config.local.root_dir);

    let mut statuses = Vec::new();
    let mut existing = Vec::new();
    let mut equal_binary_paths = Vec::new();
    let mut plain_file_sizes = PlainFileSizes::new();
    let mut read_errors = Vec::new();

    for path in target_paths {
        let left_path = core.inspect_path(left, path)?;
        let right_path = core.inspect_path(right, path)?;
        if (!follow_external_links
            && (resolved_path_outside_root(&left_path, left_root)
                || resolved_path_outside_root(&right_path, right_root)))
            || matches!(left_path, TargetPath::Symlink { .. })
            || matches!(right_path, TargetPath::Symlink { .. })
        {
            statuses.push(FileStatus {
                path: path.clone(),
                status: FileStatusKind::Modified,
                hunks: None,
                ref_badge: None,
            });
            existing.push(path.clone());
            continue;
        }
        let left_children = core.fetch_children(left, path).ok();
        let right_children = core.fetch_children(right, path).ok();
        if are_empty_directories(&left_children, &right_children)
            || left_children
                .as_ref()
                .is_some_and(|nodes| !nodes.is_empty())
            || right_children
                .as_ref()
                .is_some_and(|nodes| !nodes.is_empty())
        {
            statuses.push(FileStatus {
                path: path.clone(),
                status: FileStatusKind::Modified,
                hunks: None,
                ref_badge: None,
            });
            existing.push(path.clone());
            continue;
        }
        // 「ない」はパスの確認の結果で決め、読めない側は「ない」ともせず誤りにする
        let left_read = read_inspected(core, left, path, &left_path, limit);
        let right_read = read_inspected(core, right, path, &right_path, limit);
        let (left_content, right_content) = match (left_read, right_read) {
            (Ok(left_content), Ok(right_content)) => (left_content, right_content),
            (left_read, right_read) => {
                read_errors.push(DiffError {
                    path: path.clone(),
                    reason: read_failure_reason(
                        (left, left_read.as_ref().err()),
                        (right, right_read.as_ref().err()),
                    ),
                });
                continue;
            }
        };

        match status_from_read_results(
            left_content.is_some(),
            right_content.is_some(),
            left_content.as_deref(),
            right_content.as_deref(),
        ) {
            Ok(kind) => {
                if kind == FileStatusKind::Equal
                    && (left_content.as_deref().is_some_and(is_binary)
                        || right_content.as_deref().is_some_and(is_binary))
                {
                    equal_binary_paths.push(path.clone());
                }
                if kind != FileStatusKind::Equal {
                    let left_size = left_content.as_ref().map(|bytes| bytes.len() as u64);
                    let right_size = right_content.as_ref().map(|bytes| bytes.len() as u64);
                    plain_file_sizes.insert(path.clone(), (left_size, right_size));
                }
                statuses.push(FileStatus {
                    path: path.clone(),
                    status: kind,
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

    if existing.is_empty() && read_errors.is_empty() && !target_paths.is_empty() {
        anyhow::bail!("specified path(s) not found on either side");
    }

    let mut diff_files = filter_changed_files(&existing, &statuses);
    diff_files.extend(equal_binary_paths);
    Ok((
        left_tree,
        right_tree,
        statuses,
        existing,
        diff_files,
        plain_file_sizes,
        read_errors,
    ))
}

/// 側の root_dir を、ファイルと同じ inspect_path で実パスに直す。
///
/// root_dir がまだない場合は、設定に書いた root_dir をそのまま使う。
fn real_root_dir(
    core: &mut CoreRuntime,
    side: &Side,
    config: &AppConfig,
) -> anyhow::Result<PathBuf> {
    let configured = match side {
        Side::Local => &config.local.root_dir,
        Side::Remote(name) => &config.servers[name].root_dir,
    };
    Ok(inspected_real_path(core.inspect_path(side, "")?).unwrap_or_else(|| configured.clone()))
}

/// `root` は実パスに直した root_dir。
fn resolved_path_outside_root(path: &TargetPath, root: &Path) -> bool {
    match path {
        TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => {
            !real_path.starts_with(root)
        }
        TargetPath::Missing { real_parent } => !real_parent.starts_with(root),
    }
}

fn path_escapes_root(
    core: &mut CoreRuntime,
    side: &Side,
    real_root: &Path,
    path: &str,
) -> anyhow::Result<bool> {
    let inspected = core.inspect_path(side, path)?;
    Ok(resolved_path_outside_root(&inspected, real_root))
}

fn read_existing_diff_file(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    limit: SizeLimit,
) -> anyhow::Result<Option<Vec<u8>>> {
    let inspected = core.inspect_path(side, path)?;
    read_inspected(core, side, path, &inspected, limit)
}

fn link_target_for_diff(
    core: &mut CoreRuntime,
    side: &Side,
    tree: &FileTree,
    path: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(target) = find_symlink_target(tree, path) {
        return Ok(Some(target));
    }
    Ok(match core.inspect_path(side, path)? {
        TargetPath::Symlink { link_target, .. } => Some(link_target.to_string_lossy().into_owned()),
        _ => None,
    })
}

/// PartialScan: 指定ディレクトリ配下のみツリー取得して既存フローに接続する。
#[allow(clippy::too_many_arguments)]
fn run_diff_partial_scan(
    dir_paths: &[String],
    original_paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    config: &AppConfig,
    max_entries: usize,
    limit: SizeLimit,
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
        (left, right),
        core,
        left_tree,
        right_tree,
        limit,
    )
}

/// FullScan: 従来通り全ツリーを取得する。
fn run_diff_full_scan(
    paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    max_entries: usize,
    limit: SizeLimit,
) -> anyhow::Result<DiffScanResult> {
    let left_tree = core.fetch_tree_recursive(left, max_entries, true)?;
    let right_tree = core.fetch_tree_recursive(right, max_entries, true)?;
    compute_statuses_and_resolve(paths, (left, right), core, left_tree, right_tree, limit)
}

/// ツリーからステータス計算 → パス解決 → diff ファイルリスト構築（PartialScan / FullScan 共通）
fn compute_statuses_and_resolve(
    paths: &[String],
    (left, right): (&Side, &Side),
    core: &mut CoreRuntime,
    left_tree: FileTree,
    right_tree: FileTree,
    limit: SizeLimit,
) -> anyhow::Result<DiffScanResult> {
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree);

    // Refine statuses with content comparison for metadata-ambiguous files
    // 読めなかったパスは状態を変えずに残す（名前で指定したパスを「ない」にしない）。変更のある
    // ファイルとして一件ずつの比較に回り、そこで他の経路と同じ理由で誤りになる
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    if !paths_to_compare.is_empty() {
        let left_batch = fetch_contents_required(left, &paths_to_compare, core, limit);
        let right_batch = fetch_contents_required(right, &paths_to_compare, core, limit);
        let verified = verified_content_pairs(&paths_to_compare, &left_batch, &right_batch);
        refine_status_with_content(&mut statuses, &verified.pairs);
    }

    let target_files =
        resolve_target_files_from_statuses(paths, &statuses, &left_tree, &right_tree)?;

    let (existing_files, missing_files) = partition_existing_files(&target_files, &statuses);
    let empty_directories = directories_without_files(paths, &existing_files, left, right, core);
    for path in missing_files
        .iter()
        .filter(|path| !empty_directories.contains(*path))
    {
        eprintln!("Warning: '{}' not found on either side", path);
    }

    if existing_files.is_empty() && empty_directories.is_empty() && !paths.is_empty() {
        anyhow::bail!("specified path(s) not found on either side");
    }

    let diff_files = filter_changed_files(&existing_files, &statuses);
    let plain_file_sizes = diff_files
        .iter()
        .filter_map(|path| {
            plain_file_sizes(path, &left_tree, &right_tree).map(|sizes| (path.clone(), sizes))
        })
        .collect();
    Ok((
        left_tree,
        right_tree,
        statuses,
        existing_files,
        diff_files,
        plain_file_sizes,
        Vec::new(),
    ))
}

/// 左右のツリーの少なくとも一方にあり、ある側ではどちらも大きさの分かる通常のファイルなら、
/// その左右の大きさ（ない側は None）
fn plain_file_sizes(
    path: &str,
    left_tree: &FileTree,
    right_tree: &FileTree,
) -> Option<(Option<u64>, Option<u64>)> {
    let size_of = |node: Option<&FileNode>| -> Result<Option<u64>, ()> {
        match node {
            None => Ok(None),
            Some(node) if node.is_file() => node.size.map(Some).ok_or(()),
            Some(_) => Err(()),
        }
    };
    let left = size_of(left_tree.find_node(path)).ok()?;
    let right = size_of(right_tree.find_node(path)).ok()?;
    (left.is_some() || right.is_some()).then_some((left, right))
}

/// パスの確認の結果 `inspected` がない側は読まずに None、ある側は中身を読む
fn read_inspected(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    inspected: &TargetPath,
    limit: SizeLimit,
) -> anyhow::Result<Option<Vec<u8>>> {
    match inspected {
        TargetPath::Missing { .. } => Ok(None),
        TargetPath::File { .. } | TargetPath::Symlink { .. } => core
            .read_file_bytes(side, path, limit == SizeLimit::Lifted)
            .map(Some),
    }
}

#[cfg(test)]
mod tests {
    use crate::tree::{FileNode, FileTree};

    fn tree(nodes: Vec<FileNode>) -> FileTree {
        let mut tree = FileTree::new("/root");
        tree.nodes = nodes;
        tree
    }

    // ── plain file sizes ──

    fn file(name: &str, size: u64) -> FileNode {
        let mut node = FileNode::new_file(name);
        node.size = Some(size);
        node
    }

    #[test]
    fn plain_file_sizes_are_taken_from_the_sides_that_have_it() {
        let left = tree(vec![FileNode::new_dir_with_children(
            "a",
            vec![file("1.txt", 3)],
        )]);
        let right = tree(vec![]);

        assert_eq!(
            super::plain_file_sizes("a/1.txt", &left, &right),
            Some((Some(3), None))
        );
    }

    #[test]
    fn directory_symlink_or_missing_path_has_no_plain_file_sizes() {
        let left = tree(vec![FileNode::new_dir("dir"), file("mixed", 1)]);
        let right = tree(vec![
            FileNode::new_symlink("link", "target"),
            FileNode::new_dir("mixed"),
        ]);

        assert_eq!(super::plain_file_sizes("dir", &left, &right), None);
        assert_eq!(super::plain_file_sizes("link", &left, &right), None);
        assert_eq!(super::plain_file_sizes("mixed", &left, &right), None);
        assert_eq!(super::plain_file_sizes("missing", &left, &right), None);
    }
}
