//! diff サブコマンドの実装。

use crate::cli::content_reads::{read_contents, read_failure_reason, read_one};
use crate::cli::diff_children::children_to_expand;
use crate::cli::diff_kinds::{directory_against_file, FileSide};
use crate::cli::diff_listing::{self, ReferenceKind};
use crate::cli::diff_scans::{combine, DiffScanResult, PathsNotFound, PlainFileSizes};
use crate::cli::ref_guard;
use crate::config::{resolve_max_entries, AppConfig};
use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
use crate::runtime::include_check::resolved_path_outside_root;
use crate::runtime::size_limit::SizeLimit;
use crate::runtime::target_io::TargetPath;
use crate::runtime::{CoreRuntime, RuntimeTargets};
use crate::service::content_reads::verified_content_pairs;
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
use crate::service::split_diff_paths;
use crate::service::status::{
    compute_status_from_trees, needs_content_compare, refine_status_with_content,
    status_from_read_results,
};
use crate::service::types::{
    exit_code, DiffError, DiffOutput, FileStatus, FileStatusKind, LinkTargets, MultiDiffOutput,
    MultiDiffSummary,
};
use crate::service::unreadable_names::requested_unreadable_names;
use crate::side::Side;
use crate::tree::{FileNode, FileTree};
use remote_merge_core::listing::ChildListing;
use remote_merge_engine::local_io::ScanTruncated;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

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

    // 名前で指定したパスは一つずつ比べ、root_dir 全体を指すものがあれば全体も走査する
    let limit = SizeLimit::from_force(args.force);
    let paths = split_diff_paths(&args.paths);
    let whole = paths.whole.as_ref().map(|whole| {
        run_diff_full_scan(
            whole,
            &pair.left,
            &pair.right,
            &mut core,
            max_entries,
            limit,
        )
    });
    let named = (!paths.named.is_empty()).then(|| {
        run_diff_fast_path(
            &paths.named,
            (&pair.left, &left_root),
            (&pair.right, &right_root),
            &mut core,
            &config,
            args.follow_external_links,
            limit,
        )
    });
    let (left_tree, right_tree, _, existing_files, diff_files, plain_file_sizes, scan_errors) =
        combine(whole, named, &paths.named)?;

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
    // 比べる対象に入れたパス。ディレクトリを展開した子が既にあれば、もう一度は入れない
    let mut queued: HashSet<String> = diff_files.iter().cloned().collect();
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
            let left_listing = listed_children(&mut core, &pair.left, path, left_absent)?;
            let right_listing = listed_children(&mut core, &pair.right, path, right_absent)?;
            // 一覧を取れなかったものは、ファイルとしてもない側としても比べない
            if let Some(reason) =
                unlistable_reason((&pair.left, &left_listing), (&pair.right, &right_listing))
            {
                errors.push(DiffError {
                    path: path.clone(),
                    reason,
                });
                has_read_error = true;
                continue;
            }
            let left_children = diff_listing::children(left_listing);
            let right_children = diff_listing::children(right_listing);
            // 比べるファイルのないディレクトリ（両側か、もう一方にないとき）は差分なしとする（REQ-cli-087）
            let missing = (
                left_children.is_none() && is_missing(&mut core, &pair.left, path, left_absent)?,
                right_children.is_none() && is_missing(&mut core, &pair.right, path, right_absent)?,
            );
            if without_files(&left_children, &right_children, missing) {
                scanned_files = scanned_files.saturating_sub(1);
                continue;
            }
            // 残るディレクトリは子を持つか、子がなくてもう一方がファイル（REQ-cli-085）
            if left_children.is_some() || right_children.is_some() {
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
                // ディレクトリでなくパスのある側はファイルで、その側には子がない。本文を種類の差と
                // ともに示す
                let left_file = (!left_absent && left_children.is_none() && !missing.0)
                    .then(|| file_body(&mut core, &pair.left, path, limit));
                let right_file = (!right_absent && right_children.is_none() && !missing.1)
                    .then(|| file_body(&mut core, &pair.right, path, limit));
                let left_below_absent = left_absent || left_file.is_some();
                let right_below_absent = right_absent || right_file.is_some();
                let file = match (&left_file, &right_file) {
                    (Some(Ok(body)), _) => Some(Ok(FileSide::Left(body))),
                    (_, Some(Ok(body))) => Some(Ok(FileSide::Right(body))),
                    (Some(Err(reason)), _) => Some(Err((&pair.left, reason))),
                    (_, Some(Err(reason))) => Some(Err((&pair.right, reason))),
                    _ => None,
                };
                match file {
                    Some(Ok(file)) => file_diffs.push(directory_against_file(
                        path,
                        (left_info.clone(), right_info.clone()),
                        file,
                        args.max_lines,
                    )),
                    Some(Err((side, reason))) => {
                        errors.push(DiffError {
                            path: path.clone(),
                            reason: read_failure_reason(&[(side, Some(reason.as_str()))]),
                        });
                        has_read_error = true;
                    }
                    None => {}
                }
                let children =
                    children_to_expand(left_children.as_deref(), right_children.as_deref());
                for (child, counted) in children {
                    visited_entries += usize::from(counted);
                    if visited_entries > max_entries {
                        errors.push(DiffError {
                            path: path.clone(),
                            reason: ScanTruncated { max_entries }.to_string(),
                        });
                        has_read_error = true;
                        break;
                    }
                    let child_path = format!("{path}/{child}");
                    expanded_children.insert(child_path.clone());
                    absent_sides
                        .insert(child_path.clone(), (left_below_absent, right_below_absent));
                    active_directories.insert(
                        child_path.clone(),
                        (left_ancestors.clone(), right_ancestors.clone()),
                    );
                    // 全体の走査で既に比べる対象にある子は、二度比べず二度数えない
                    if queued.insert(child_path.clone()) {
                        pending.push_back(child_path);
                        scanned_files += 1;
                    }
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
            let (left_children, left_content) =
                diff_listing::children_of_unread(&mut core, &pair.left, path, left_content)?;
            let (right_children, right_content) =
                diff_listing::children_of_unread(&mut core, &pair.right, path, right_content)?;
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
                    let cause = read_failure_reason(&[
                        (
                            &pair.left,
                            left_content
                                .as_ref()
                                .err()
                                .filter(|_| left_children.is_none())
                                .map(String::as_str),
                        ),
                        (
                            &pair.right,
                            right_content
                                .as_ref()
                                .err()
                                .filter(|_| right_children.is_none())
                                .map(String::as_str),
                        ),
                    ]);
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
                let children =
                    children_to_expand(left_children.as_deref(), right_children.as_deref());
                for (child, counted) in children {
                    visited_entries += usize::from(counted);
                    if visited_entries > max_entries {
                        let reason = ScanTruncated { max_entries }.to_string();
                        errors.push(DiffError {
                            path: path.clone(),
                            reason: reason.clone(),
                        });
                        link_diff.note = Some(reason);
                        has_read_error = true;
                        break;
                    }
                    let child_path = format!("{path}/{child}");
                    expanded_children.insert(child_path.clone());
                    absent_sides
                        .insert(child_path.clone(), (left_below_absent, right_below_absent));
                    active_directories.insert(
                        child_path.clone(),
                        (left_ancestors.clone(), right_ancestors.clone()),
                    );
                    // 全体の走査で既に比べる対象にある子は、二度比べず二度数えない
                    if queued.insert(child_path.clone()) {
                        pending.push_back(child_path);
                        scanned_files += 1;
                    }
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
                push_unless_identical_link(&mut file_diffs, link_diff);
                continue;
            }
            if left_content.is_err() || right_content.is_err() {
                let note = "symlink target unreadable; resolved content not compared";
                link_diff.note = Some(note.into());
                let cause = read_failure_reason(&[
                    (&pair.left, left_content.as_ref().err().map(String::as_str)),
                    (
                        &pair.right,
                        right_content.as_ref().err().map(String::as_str),
                    ),
                ]);
                errors.push(DiffError {
                    path: path.clone(),
                    reason: format!("{note}: {cause}"),
                });
                has_read_error = true;
                file_diffs.push(link_diff);
                continue;
            }
            // 読めなかった側は上で誤りにしたので、ここでは両側とも読めている
            let (left_content, right_content) = (
                left_content.unwrap_or_default(),
                right_content.unwrap_or_default(),
            );
            if left_content.as_ref().is_some_and(|bytes| is_binary(bytes))
                || right_content.as_ref().is_some_and(|bytes| is_binary(bytes))
            {
                link_diff.binary = true;
                link_diff.left_hash = left_content.as_ref().map(|bytes| compute_sha256(bytes));
                link_diff.right_hash = right_content.as_ref().map(|bytes| compute_sha256(bytes));
                push_unless_identical_link(&mut file_diffs, link_diff);
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
            push_unless_identical_link(&mut file_diffs, link_diff);
            continue;
        }

        // バイト列で読み込み、事前にバイナリ判定を行う。読めない側は空とみなさず誤りにする
        let left_read = if left_absent {
            Ok(None)
        } else {
            read_compared_side(&mut core, &pair.left, path, limit)
        };
        let right_read = if right_absent {
            Ok(None)
        } else {
            read_compared_side(&mut core, &pair.right, path, limit)
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
                    reason: read_failure_reason(&[
                        (&pair.left, left_read.as_ref().err().map(String::as_str)),
                        (&pair.right, right_read.as_ref().err().map(String::as_str)),
                    ]),
                });
                has_read_error = true;
                continue;
            }
        };
        let (left_ok, right_ok) = (left_read.is_some(), right_read.is_some());
        let left_bytes = left_read.unwrap_or_default();
        let right_bytes = right_read.unwrap_or_default();
        // 両側にあり中身の同じファイルは、どの走査方式でも項目に出さない（打ち切られた走査で
        // 片側にだけ載ったファイルも、両側を読んで同じなら差分ではない）
        if left_ok && right_ok && left_bytes == right_bytes {
            continue;
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
                one_sided: false,
            }
        } else {
            // テキストファイル: String に変換して既存の build_diff_output を呼ぶ
            let left_content = String::from_utf8_lossy(&left_bytes).into_owned();
            let right_content = String::from_utf8_lossy(&right_bytes).into_owned();

            // 参照先にないときと種類の違うときは "ref" だけを出し、ほかの理由で読めないときは
            // errors にも載せる
            let ref_content = match &ref_side {
                Some(ref_s) => {
                    let read = match diff_listing::reference_kind(&mut core, ref_s, path)? {
                        ReferenceKind::Differs => Ok(None),
                        ReferenceKind::Unlistable(reason) => Err(reason),
                        ReferenceKind::Readable => {
                            read_compared_side(&mut core, ref_s, path, limit)
                        }
                    };
                    match read {
                        Ok(content) => {
                            content.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        }
                        Err(reason) => {
                            errors.push(DiffError {
                                path: path.clone(),
                                reason: read_failure_reason(&[(ref_s, Some(reason.as_str()))]),
                            });
                            has_read_error = true;
                            None
                        }
                    }
                }
                None => None,
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
        // 片側にだけあるファイルは、中身が空でも片側だけの項目として差分ありとする（REQ-cli-086）
        let mut output = output;
        if left_ok != right_ok {
            output.one_sided = true;
            if output.hunks.is_empty() && !output.binary {
                let side = if left_ok { "left" } else { "right" };
                output.note = Some(format!("empty file only on the {side}"));
            }
        }
        if !expanded_children.contains(path)
            || output.one_sided
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

/// symlink の項目を、リンク文字列も参照先の中身も左右で同じで補足もないときは出さない
fn push_unless_identical_link(file_diffs: &mut Vec<DiffOutput>, link_diff: DiffOutput) {
    if link_diff.has_changes() || link_diff.note.is_some() {
        file_diffs.push(link_diff);
    }
}

/// 左右とも子の一覧が取れて（ディレクトリで）どちらにも子がないか。
///
/// 子の一覧は include で絞った後のため、include の外のディレクトリもここに当たる。
fn are_empty_directories(left: &Option<Vec<FileNode>>, right: &Option<Vec<FileNode>>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left.is_empty() && right.is_empty())
}

/// 比べるファイルのないディレクトリか。左右とも中身の空なディレクトリか、片側が中身の空な
/// ディレクトリでもう一方にないとき。`missing` は（左, 右）にないか
fn without_files(
    left: &Option<Vec<FileNode>>,
    right: &Option<Vec<FileNode>>,
    (left_missing, right_missing): (bool, bool),
) -> bool {
    let empty = |children: &Option<Vec<FileNode>>| children.as_ref().is_some_and(Vec::is_empty);
    are_empty_directories(left, right)
        || (empty(left) && right_missing)
        || (empty(right) && left_missing)
}

/// その側にパスがないか。子を持てない側の下に展開したパスは調べずにないものとする
fn is_missing(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    absent: bool,
) -> anyhow::Result<bool> {
    Ok(absent || matches!(core.inspect_path(side, path)?, TargetPath::Missing { .. }))
}

/// 子の一覧。子を持てない側の下に展開したパスは、その側を調べずにディレクトリでないものとする。
fn listed_children(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    absent: bool,
) -> anyhow::Result<ChildListing> {
    if absent {
        return Ok(ChildListing::NotDirectory);
    }
    diff_listing::list(core, side, path)
}

/// 左右のどちらかの一覧を取れなかった理由。どちらも取れていれば None
fn unlistable_reason(
    (left_side, left): (&Side, &ChildListing),
    (right_side, right): (&Side, &ChildListing),
) -> Option<String> {
    let left = diff_listing::unlistable_reason(left);
    let right = diff_listing::unlistable_reason(right);
    (left.is_some() || right.is_some())
        .then(|| read_failure_reason(&[(left_side, left), (right_side, right)]))
}

/// ディレクトリでなくパスのある側を通常ファイルとして読んだ本文。読めなかったとき（大きさの上限で
/// 読まなかったときを含む）は、読めなかった理由
fn file_body(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    limit: SizeLimit,
) -> Result<Vec<u8>, String> {
    core.read_file_bytes(side, path, limit == SizeLimit::Lifted)
        .map_err(|error| format!("{error:#}"))
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
) -> anyhow::Result<HashSet<String>> {
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
        let left_children = diff_listing::children(diff_listing::list(core, left, normalized)?);
        let right_children = diff_listing::children(diff_listing::list(core, right, normalized)?);
        if are_empty_directories(&left_children, &right_children) {
            directories.insert(path.clone());
        }
    }
    Ok(directories)
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
        let left_listing = diff_listing::list(core, left, path)?;
        let right_listing = diff_listing::list(core, right, path)?;
        // 一覧を取れなかったものは、ファイルとしてもない側としても比べない
        if let Some(reason) = unlistable_reason((left, &left_listing), (right, &right_listing)) {
            read_errors.push(DiffError {
                path: path.clone(),
                reason,
            });
            continue;
        }
        let left_children = diff_listing::children(left_listing);
        let right_children = diff_listing::children(right_listing);
        // ディレクトリのある側があれば、比べるファイルのないものも含めて展開する側で扱う
        if left_children.is_some() || right_children.is_some() {
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
                    reason: read_failure_reason(&[
                        (left, left_read.as_ref().err().map(String::as_str)),
                        (right, right_read.as_ref().err().map(String::as_str)),
                    ]),
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
        return Err(PathsNotFound.into());
    }

    let diff_files = filter_changed_files(&existing, &statuses);
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
) -> Result<Option<Vec<u8>>, String> {
    let inspected = core
        .inspect_path(side, path)
        .map_err(|error| format!("{error:#}"))?;
    read_inspected(core, side, path, &inspected, limit)
}

/// 一件ずつの比較で片側の中身を読む。読めなかったとき、パスの確認でその側にないと分かれば
/// None（「ない」はエラーの文でなくパスの確認の結果で決める）、それ以外は読めなかった理由
fn read_compared_side(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    limit: SizeLimit,
) -> Result<Option<Vec<u8>>, String> {
    match read_one(core, side, path, limit) {
        Ok(content) => Ok(Some(content)),
        Err(reason) => match core.inspect_path(side, path) {
            Ok(TargetPath::Missing { .. }) => Ok(None),
            _ => Err(reason),
        },
    }
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

/// FullScan: 従来通り全ツリーを取得する。
fn run_diff_full_scan(
    paths: &[String],
    left: &Side,
    right: &Side,
    core: &mut CoreRuntime,
    max_entries: usize,
    limit: SizeLimit,
) -> anyhow::Result<DiffScanResult> {
    let (left_tree, left_truncated) = fetch_tree_keeping_read(core, left, max_entries)?;
    let (right_tree, right_truncated) = fetch_tree_keeping_read(core, right, max_entries)?;
    // UTF-8 として読めない名前のファイルは比べられないので、理由とともに誤りに載せる
    let unreadable: Vec<DiffError> = requested_unreadable_names(
        paths,
        &[
            (left.display_name(), &left_tree),
            (right.display_name(), &right_tree),
        ],
    )
    .into_iter()
    .map(|(path, reason)| DiffError { path, reason })
    .collect();
    // 上限を超えても読めた分は比べ、上限の超過を一度だけ誤りに載せる
    let truncation = left_truncated
        .or(right_truncated)
        .map(|truncated| DiffError {
            path: ".".into(),
            reason: truncated.to_string(),
        });
    let scan_errors = truncation.into_iter().chain(unreadable).collect();
    compute_statuses_and_resolve(
        paths,
        (left, right),
        core,
        (left_tree, right_tree),
        scan_errors,
        limit,
    )
}

/// 側の root_dir 全体を走査する。件数の上限を超えたら、上限までに読めたツリーと上限の超過を返す
fn fetch_tree_keeping_read(
    core: &mut CoreRuntime,
    side: &Side,
    max_entries: usize,
) -> anyhow::Result<(FileTree, Option<ScanTruncated>)> {
    match core.fetch_tree_recursive(side, max_entries, true) {
        Ok(tree) => Ok((tree, None)),
        Err(error) => match error.downcast_ref::<ScanTruncated>().copied() {
            Some(truncated) => Ok((
                core.fetch_tree_recursive(side, max_entries, false)?,
                Some(truncated),
            )),
            None => Err(error),
        },
    }
}

/// ツリーからステータス計算 → パス解決 → diff ファイルリスト構築（FullScan）。`scan_errors` は
/// 走査で見つけた誤り（上限の超過と読めない名前）で、返す誤りの先頭に置く
fn compute_statuses_and_resolve(
    paths: &[String],
    (left, right): (&Side, &Side),
    core: &mut CoreRuntime,
    (left_tree, right_tree): (FileTree, FileTree),
    scan_errors: Vec<DiffError>,
    limit: SizeLimit,
) -> anyhow::Result<DiffScanResult> {
    let mut statuses = compute_status_from_trees(&left_tree, &right_tree);

    // Refine statuses with content comparison for metadata-ambiguous files
    // 読めなかったパスは状態を変えずに残し（名前で指定したパスを「ない」にしない）、指定した
    // パスに当たるものだけを読めなかった理由とともに誤りにする。読めなかったパスは一件ずつの
    // 比較で読み直さない
    let paths_to_compare = needs_content_compare(&statuses, &left_tree, &right_tree);
    let left_reads = read_contents(core, left, &paths_to_compare, limit);
    let right_reads = read_contents(core, right, &paths_to_compare, limit);
    let verified = verified_content_pairs(&paths_to_compare, &left_reads, &right_reads);
    refine_status_with_content(&mut statuses, &verified.pairs);

    let target_files =
        resolve_target_files_from_statuses(paths, &statuses, &left_tree, &right_tree)?;

    let (existing_files, missing_files) = partition_existing_files(&target_files, &statuses);
    let empty_directories = directories_without_files(paths, &existing_files, left, right, core)?;
    for path in missing_files
        .iter()
        .filter(|path| !empty_directories.contains(*path))
    {
        eprintln!("Warning: '{}' not found on either side", path);
    }

    // 走査が誤りを見つけたときは、比べるパスがなくても見つからなかったとせず、その誤りを返す
    // （root_dir 全体と名前で指定したパスを合わせるときに、誤りが落ちないように）
    if existing_files.is_empty()
        && empty_directories.is_empty()
        && !paths.is_empty()
        && scan_errors.is_empty()
    {
        return Err(PathsNotFound.into());
    }

    // 走査は指定の外のファイルも読み比べるので、誤りは指定したパスに当たるものだけにする
    let read_errors: Vec<DiffError> = verified
        .failures
        .iter()
        .filter(|failure| existing_files.contains(&failure.path))
        .map(|failure| DiffError {
            path: failure.path.clone(),
            reason: read_failure_reason(&[
                (left, left_reads.failure(&failure.path)),
                (right, right_reads.failure(&failure.path)),
            ]),
        })
        .collect();
    let diff_files: Vec<String> = filter_changed_files(&existing_files, &statuses)
        .into_iter()
        .filter(|path| !read_errors.iter().any(|error| error.path == *path))
        .collect();
    let plain_file_sizes = diff_files
        .iter()
        .filter_map(|path| {
            plain_file_sizes(path, &left_tree, &right_tree).map(|sizes| (path.clone(), sizes))
        })
        .collect();
    let errors = scan_errors.into_iter().chain(read_errors).collect();
    Ok((
        left_tree,
        right_tree,
        statuses,
        existing_files,
        diff_files,
        plain_file_sizes,
        errors,
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
) -> Result<Option<Vec<u8>>, String> {
    match inspected {
        TargetPath::Missing { .. } => Ok(None),
        TargetPath::File { .. } | TargetPath::Symlink { .. } => {
            read_one(core, side, path, limit).map(Some)
        }
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

    // 走査の後に読み比べるファイルの大きさをエージェントがエラーで答えたら、その理由のまま
    // 誤りにし、一件ずつの比較に回して読み直さない（読み直せばエージェントに二度目を尋ねる）
    #[cfg(unix)]
    #[test]
    fn an_agent_error_in_the_scan_comparison_is_reported_without_reading_again() {
        use crate::runtime::size_limit::SizeLimit;
        use crate::side::Side;
        use std::sync::atomic::Ordering;

        let local = tempfile::TempDir::new().unwrap();
        std::fs::write(local.path().join("a.txt"), "left\n").unwrap();
        let mut core = crate::runtime::CoreRuntime::new_for_test();
        core.config.local.root_dir = local.path().to_path_buf();
        core.config.servers.insert(
            "develop".into(),
            crate::config::ServerConfig {
                host: "10.0.0.1".into(),
                port: 22,
                user: "deploy".into(),
                auth: crate::config::AuthMethod::Key,
                password: None,
                key: None,
                root_dir: "/srv/app".into(),
                ssh_options: None,
                sudo: false,
                file_permissions: None,
                dir_permissions: None,
            },
        );
        let received = crate::runtime::scripted_agent::connect(
            &mut core,
            "develop",
            Some(crate::agent::protocol::AgentResponse::Error {
                message: "stat failed".into(),
            }),
        );
        // 大きさの分からない左右のファイルは中身を読み比べる
        let both = || tree(vec![FileNode::new_file("a.txt")]);

        let (_, _, _, _, diff_files, _, read_errors) = super::compute_statuses_and_resolve(
            &["a.txt".to_string()],
            (&Side::Local, &Side::Remote("develop".into())),
            &mut core,
            (both(), both()),
            Vec::new(),
            SizeLimit::Enforced,
        )
        .unwrap();

        assert_eq!(read_errors.len(), 1, "{read_errors:?}");
        assert_eq!(read_errors[0].path, "a.txt");
        assert!(
            read_errors[0].reason.contains("stat failed"),
            "{read_errors:?}"
        );
        assert!(diff_files.is_empty(), "{diff_files:?}");
        assert_eq!(received.load(Ordering::SeqCst), 1);
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
