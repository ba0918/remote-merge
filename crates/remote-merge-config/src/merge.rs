use crate::error::AppError;
use crate::filter::{IncludeValues, normalize_include_paths};
use crate::loading::resolve_local_root;
use crate::schema::{RawConfig, RawDefaultsConfig};
use crate::value::*;
use anyhow::bail;
use std::collections::BTreeMap;

/// exclude を、使うものと "../" を含んで捨てるもの（重ねて書かれたものは一つ）に分ける
pub(crate) fn split_suspicious_excludes(patterns: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut kept = Vec::new();
    let mut suspicious: Vec<String> = Vec::new();
    for pattern in patterns {
        if !pattern.contains("../") {
            kept.push(pattern);
        } else if !suspicious.contains(&pattern) {
            suspicious.push(pattern);
        }
    }
    (kept, suspicious)
}

pub(crate) fn merge_configs(
    global: Option<RawConfig>,
    project: Option<RawConfig>,
) -> crate::error::Result<AppConfig> {
    let (global, mut project) = match (global, project) {
        (Some(g), Some(p)) => (g, Some(p)),
        (Some(g), None) => (g, None),
        (None, Some(p)) => (p, None),
        (None, None) => {
            bail!("internal error: merge_configs called with both global and project as None");
        }
    };

    let mut servers_raw = global.servers.unwrap_or_default();
    if let Some(ref mut proj) = project
        && let Some(proj_servers) = proj.servers.take()
    {
        for (name, server) in proj_servers {
            servers_raw.insert(name, server);
        }
    }
    let mut servers = BTreeMap::new();
    for (name, raw) in servers_raw {
        let config = convert_server_config(&name, raw)?;
        servers.insert(name, config);
    }

    let local_raw = project
        .as_ref()
        .and_then(|p| p.local.as_ref())
        .or(global.local.as_ref())
        .ok_or_else(|| AppError::ConfigValidation {
            field: "local".into(),
            message: "[local] section is required".into(),
        })?;
    // 空の root_dir は基準のディレクトリにつなぐ前に止める（つなぐと設定ファイルの場所になる）
    if local_raw.root_dir.is_empty() {
        bail!(AppError::ConfigValidation {
            field: "local.root_dir".into(),
            message: "root_dir must not be empty".into(),
        });
    }
    let local = LocalConfig {
        root_dir: resolve_local_root(&local_raw.root_dir, &local_raw.config_path)?,
    };

    let mut filter = FilterConfig::default();
    if let Some(gf) = global.filter {
        if let Some(exc) = gf.exclude {
            filter.exclude.extend(exc);
        }
        if let Some(inc) = gf.include {
            for i in inc {
                if !filter.include.contains(&i) {
                    filter.include.push(i);
                }
            }
        }
    }
    if let Some(ref mut proj) = project
        && let Some(pf) = proj.filter.take()
    {
        if let Some(exc) = pf.exclude {
            for e in exc {
                if !filter.exclude.contains(&e) {
                    filter.exclude.push(e);
                }
            }
        }
        if let Some(inc) = pf.include {
            for i in inc {
                if !filter.include.contains(&i) {
                    filter.include.push(i);
                }
            }
        }
    }

    // "../" を含む exclude は読み込みの時にパターンごとに一度だけ警告して捨てる（REQ-config-038）
    let (exclude, suspicious) = split_suspicious_excludes(std::mem::take(&mut filter.exclude));
    for pattern in &suspicious {
        tracing::warn!(
            target: "remote_merge::warning",
            "Skipping suspicious exclude pattern containing '../': {pattern}"
        );
    }
    filter.exclude = exclude;
    if !filter.include.is_empty() {
        let (normalized, warnings) = normalize_include_paths(&filter.include);
        for w in &warnings {
            tracing::warn!(target: "remote_merge::warning", "{}", w);
        }
        filter.include = match normalized {
            IncludeValues::Listed(paths) => paths,
            IncludeValues::WholeRoot => Vec::new(),
            // 指定がないものとして全体に広げない
            IncludeValues::AllInvalid => bail!(AppError::ConfigValidation {
                field: "filter.include".into(),
                message: "every include value is invalid; refusing to scan the whole root_dir"
                    .into(),
            }),
        };
    }

    let ssh_raw = project
        .as_ref()
        .and_then(|p| p.ssh.as_ref())
        .or(global.ssh.as_ref());
    let ssh = match ssh_raw {
        Some(raw) => convert_raw_ssh_config(raw)?,
        None => SshConfig::default(),
    };

    let backup_raw = project
        .as_ref()
        .and_then(|p| p.backup.as_ref())
        .or(global.backup.as_ref());
    let backup = convert_backup_config(backup_raw);

    let agent = if let Some(ref proj) = project {
        proj.agent.as_ref().map_or_else(
            || convert_agent_config(global.agent.as_ref()),
            |a| convert_agent_config(Some(a)),
        )
    } else {
        convert_agent_config(global.agent.as_ref())
    };

    let defaults = {
        let global_defaults = global.defaults.as_ref();
        let project_defaults = project.as_ref().and_then(|p| p.defaults.as_ref());
        let merged = merge_raw_defaults(global_defaults, project_defaults);
        convert_defaults_config(merged.as_ref())?
    };

    let max_scan_entries = {
        let raw_val = project
            .as_ref()
            .and_then(|p| p.max_scan_entries)
            .or(global.max_scan_entries)
            .unwrap_or(DEFAULT_MAX_SCAN_ENTRIES);
        if let Err(msg) = validate_max_scan_entries(raw_val) {
            bail!(AppError::ConfigValidation {
                field: "max_scan_entries".into(),
                message: msg
            });
        }
        raw_val
    };

    let content =
        crate::content::merge_content(global.content, project.and_then(|p| p.content), &servers)?;
    Ok(AppConfig {
        content,
        servers,
        local,
        filter,
        ssh,
        backup,
        agent,
        defaults,
        max_scan_entries,
    })
}

/// グローバルとプロジェクトの RawDefaultsConfig をフィールド単位でマージ
/// プロジェクト側にフィールドがあればそれを優先、なければグローバルの値を使う
pub(crate) fn merge_raw_defaults(
    global: Option<&RawDefaultsConfig>,
    project: Option<&RawDefaultsConfig>,
) -> Option<RawDefaultsConfig> {
    match (global, project) {
        (None, None) => None,
        (Some(g), None) => Some(RawDefaultsConfig {
            file_permissions: g.file_permissions.clone(),
            dir_permissions: g.dir_permissions.clone(),
        }),
        (None, Some(p)) => Some(RawDefaultsConfig {
            file_permissions: p.file_permissions.clone(),
            dir_permissions: p.dir_permissions.clone(),
        }),
        (Some(g), Some(p)) => Some(RawDefaultsConfig {
            file_permissions: p
                .file_permissions
                .clone()
                .or_else(|| g.file_permissions.clone()),
            dir_permissions: p
                .dir_permissions
                .clone()
                .or_else(|| g.dir_permissions.clone()),
        }),
    }
}
