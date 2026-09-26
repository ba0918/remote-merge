//! diff サブコマンドの実装。

use crate::app::Side;
use crate::cli::ref_guard;
use crate::cli::tolerant_io::fetch_contents_tolerant;
use crate::config::{resolve_max_entries, AppConfig};
use crate::diff::binary::compute_sha256;
use crate::diff::engine::is_binary;
use crate::runtime::target_io::TargetPath;
use crate::runtime::{CoreRuntime, RuntimeTargets};
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
    exit_code, DiffError, DiffOutput, FileStatus, FileStatusKind, LinkTargets, MultiDiffOutput,
    MultiDiffSummary,
};
use crate::service::{resolve_scan_strategy, ScanStrategy};
use crate::tree::FileTree;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

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
        OutputFormat::Text => format_multi_diff_text(&multi_output),
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

    // ScanStrategy で分岐: FastPath / PartialScan / FullScan
    let strategy = resolve_scan_strategy(&args.paths, false);

    let (left_tree, right_tree, statuses, existing_files, diff_files) = match strategy {
        ScanStrategy::FastPath(ref target_paths) => {
            check_path_traversal(target_paths)?;
            run_diff_fast_path(
                target_paths,
                &pair.left,
                &pair.right,
                &mut core,
                &config,
                args.follow_external_links,
            )?
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
    let mut errors = Vec::new();
    let mut has_read_error = false;
    let mut pending: VecDeque<String> = process_files.iter().cloned().collect();
    let mut scanned_files = existing_files.len();
    let mut visited_entries = 0;
    let mut expanded_children = HashSet::new();
    let mut active_directories: HashMap<String, (Vec<PathBuf>, Vec<PathBuf>)> = HashMap::new();
    while let Some(owned_path) = pending.pop_front() {
        let path = &owned_path;
        if !args.follow_external_links
            && (path_escapes_root(&mut core, &pair.left, &config, path)?
                || path_escapes_root(&mut core, &pair.right, &config, path)?)
        {
            let reason = "content not compared (outside root_dir; use --follow-external-links)";
            let mut output = build_masked_diff_output(path, left_info.clone(), right_info.clone());
            output.sensitive = false;
            output.note = Some(reason.into());
            let left_target = link_target_for_diff(&mut core, &pair.left, &left_tree, path)?;
            let right_target = link_target_for_diff(&mut core, &pair.right, &right_tree, path)?;
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
        // LeftOnly/RightOnly の場合、存在しない側の読み込み失敗は予想通りなので Warning を抑制
        let status = statuses.iter().find(|s| s.path == *path).map(|s| s.status);
        let (left_quiet, right_quiet) = quiet_flags_for_status(status);

        let sensitive = is_sensitive(path, &config.filter.sensitive);

        // symlink 判定（ツリー情報から）— sensitive でもターゲットパスは機密情報ではないため先に判定
        let left_symlink_target = link_target_for_diff(&mut core, &pair.left, &left_tree, path)?;
        let right_symlink_target = link_target_for_diff(&mut core, &pair.right, &right_tree, path)?;
        if left_symlink_target.is_none() && right_symlink_target.is_none() {
            let left_children = core.fetch_children(&pair.left, path).unwrap_or_default();
            let right_children = core.fetch_children(&pair.right, path).unwrap_or_default();
            if !left_children.is_empty() || !right_children.is_empty() {
                let (mut left_ancestors, mut right_ancestors) =
                    active_directories.remove(path).unwrap_or_default();
                let left_real = inspected_real_path(core.inspect_path(&pair.left, path)?);
                let right_real = inspected_real_path(core.inspect_path(&pair.right, path)?);
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
                let children: BTreeSet<_> = left_children
                    .iter()
                    .chain(&right_children)
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
                sensitive,
            );
            let target_sensitive = [&left_symlink_target, &right_symlink_target]
                .into_iter()
                .flatten()
                .any(|target| is_sensitive(target, &config.filter.sensitive))
                || sensitive_link_chain(&mut core, &pair.left, path, &config)
                || sensitive_link_chain(&mut core, &pair.right, path, &config);
            if (sensitive || target_sensitive) && !args.force {
                link_diff.sensitive = true;
                link_diff.note =
                    Some("Content hidden (sensitive file). Use --force to show.".into());
                file_diffs.push(link_diff);
                continue;
            }
            let external = [&pair.left, &pair.right].into_iter().any(|side| {
                let root = match side {
                    Side::Local => &config.local.root_dir,
                    Side::Remote(name) => &config.servers[name].root_dir,
                };
                match core.inspect_path(side, path) {
                    Ok(TargetPath::Symlink { real_path, .. }) => !real_path.starts_with(root),
                    _ => false,
                }
            });
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
            let left_content = read_existing_diff_file(&mut core, &pair.left, path);
            let right_content = read_existing_diff_file(&mut core, &pair.right, path);
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
                    let reason = "symlink target unreadable; resolved content not compared";
                    link_diff.note = Some(reason.into());
                    errors.push(DiffError {
                        path: path.clone(),
                        reason: reason.into(),
                    });
                    has_read_error = true;
                    file_diffs.push(link_diff);
                    continue;
                }
                let (mut left_ancestors, mut right_ancestors) =
                    active_directories.remove(path).unwrap_or_default();
                let left_real = inspected_real_path(core.inspect_path(&pair.left, path)?);
                let right_real = inspected_real_path(core.inspect_path(&pair.right, path)?);
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
                    pending.push_back(child_path);
                    active_directories.insert(
                        format!("{path}/{child}"),
                        (left_ancestors.clone(), right_ancestors.clone()),
                    );
                    scanned_files += 1;
                }
                link_diff.hunks.clear();
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
                    link_diff.hunks = build_diff_output(
                        path,
                        left_info.clone(),
                        right_info.clone(),
                        &left_text,
                        &right_text,
                        sensitive,
                        args.max_lines,
                        None,
                        None,
                    )
                    .hunks;
                }
                file_diffs.push(link_diff);
                continue;
            }
            if left_content.is_err() || right_content.is_err() {
                let reason = "symlink target unreadable; resolved content not compared";
                link_diff.hunks.clear();
                link_diff.note = Some(reason.into());
                errors.push(DiffError {
                    path: path.clone(),
                    reason: reason.into(),
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
                link_diff.hunks.clear();
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
                sensitive,
                args.max_lines,
                None,
                None,
            );
            link_diff.hunks = content_diff.hunks;
            file_diffs.push(link_diff);
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
        if !expanded_children.contains(path)
            || output.binary && output.left_hash != output.right_hash
            || !output.hunks.is_empty()
            || output.note.is_some()
        {
            file_diffs.push(output);
        }
    }

    let files_with_changes = file_diffs
        .iter()
        .filter(|d| {
            (d.binary && d.left_hash != d.right_hash)
                || (d.symlink
                    && d.link_targets
                        .as_ref()
                        .is_some_and(|targets| targets.left != targets.right))
                || !d.hunks.is_empty()
                || (d.sensitive && d.note.is_some())
        })
        .count();
    let multi_output = MultiDiffOutput {
        summary: MultiDiffSummary {
            scanned_files,
            files_with_changes,
        },
        files: file_diffs,
        errors,
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

    core.disconnect_all();
    Ok((multi_output, code))
}

fn inspected_real_path(path: TargetPath) -> Option<PathBuf> {
    match path {
        TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => Some(real_path),
        TargetPath::Missing { .. } => None,
    }
}

fn sensitive_link_chain(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
    config: &AppConfig,
) -> bool {
    let root = match side {
        Side::Local => &config.local.root_dir,
        Side::Remote(name) => &config.servers[name].root_dir,
    };
    let mut current = PathBuf::from(path);
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let inspected = match core.inspect_path(side, &current.to_string_lossy()) {
            Ok(inspected) => inspected,
            Err(_) => return false,
        };
        let TargetPath::Symlink {
            link_target,
            real_path,
        } = inspected
        else {
            return false;
        };
        if is_sensitive(&link_target.to_string_lossy(), &config.filter.sensitive)
            || is_sensitive(&real_path.to_string_lossy(), &config.filter.sensitive)
        {
            return true;
        }
        let candidate = if link_target.is_absolute() {
            link_target
        } else {
            current.parent().unwrap_or(Path::new("")).join(link_target)
        };
        let relative = if candidate.is_absolute() {
            match candidate.strip_prefix(root) {
                Ok(relative) => relative,
                Err(_) => return false,
            }
        } else {
            candidate.as_path()
        };
        let mut next = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(name) => next.push(name),
                Component::CurDir => {}
                Component::ParentDir if next.pop() => {}
                _ => return false,
            }
        }
        current = next;
    }
    false
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
    follow_external_links: bool,
) -> anyhow::Result<DiffScanResult> {
    let left_tree = FileTree::new(&config.local.root_dir);
    let right_tree = FileTree::new(&config.local.root_dir);

    let mut statuses = Vec::new();
    let mut existing = Vec::new();
    let mut equal_binary_paths = Vec::new();

    for path in target_paths {
        let left_path = core.inspect_path(left, path)?;
        let right_path = core.inspect_path(right, path)?;
        if (!follow_external_links
            && (resolved_path_outside_root(&left_path, left, config)
                || resolved_path_outside_root(&right_path, right, config)))
            || matches!(left_path, TargetPath::Symlink { .. })
            || matches!(right_path, TargetPath::Symlink { .. })
        {
            statuses.push(FileStatus {
                path: path.clone(),
                status: FileStatusKind::Modified,
                sensitive: is_sensitive(path, &config.filter.sensitive),
                hunks: None,
                ref_badge: None,
            });
            existing.push(path.clone());
            continue;
        }
        let left_children = core.fetch_children(left, path).unwrap_or_default();
        let right_children = core.fetch_children(right, path).unwrap_or_default();
        if !left_children.is_empty() || !right_children.is_empty() {
            statuses.push(FileStatus {
                path: path.clone(),
                status: FileStatusKind::Modified,
                sensitive: is_sensitive(path, &config.filter.sensitive),
                hunks: None,
                ref_badge: None,
            });
            existing.push(path.clone());
            continue;
        }
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
                if kind == FileStatusKind::Equal
                    && (is_binary(&left_bytes) || is_binary(&right_bytes))
                {
                    equal_binary_paths.push(path.clone());
                }
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

    let mut diff_files = filter_changed_files(&existing, &statuses);
    diff_files.extend(equal_binary_paths);
    Ok((left_tree, right_tree, statuses, existing, diff_files))
}

fn resolved_path_outside_root(path: &TargetPath, side: &Side, config: &AppConfig) -> bool {
    let root = match side {
        Side::Local => &config.local.root_dir,
        Side::Remote(name) => &config.servers[name].root_dir,
    };
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
    config: &AppConfig,
    path: &str,
) -> anyhow::Result<bool> {
    let inspected = core.inspect_path(side, path)?;
    Ok(resolved_path_outside_root(&inspected, side, config))
}

fn read_existing_diff_file(
    core: &mut CoreRuntime,
    side: &Side,
    path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    match core.inspect_path(side, path)? {
        TargetPath::Missing { .. } => Ok(None),
        TargetPath::File { .. } | TargetPath::Symlink { .. } => {
            core.read_file_bytes(side, path, false).map(Some)
        }
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
    use crate::service::types::FileStatusKind;

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
}
