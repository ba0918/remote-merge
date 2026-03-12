//! rollback サブコマンドの実装。
//!
//! CoreRuntime でバックアップセッションを取得し、Service 層で
//! 期限判定・復元計画を立案、フォーマッターで出力する。

use crate::app::Side;
use crate::config::AppConfig;
use crate::runtime::CoreRuntime;
use crate::service::output::{
    format_backup_list_text, format_json, format_rollback_text, OutputFormat,
};
use crate::service::rollback::{mark_expired, plan_restore, rollback_exit_code};
use crate::service::source_pair::build_source_info;
use crate::service::types::{
    BackupListOutput, RollbackFailure, RollbackOutput, RollbackSkipped, SourceInfo,
};

/// rollback サブコマンドの引数
pub struct RollbackArgs {
    pub target: Option<String>,
    pub list: bool,
    pub session: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub format: String,
}

/// --target を Side に解決する。
/// --list モードでは省略時にローカルをデフォルトとする。
fn resolve_target(target: Option<&str>, is_list: bool) -> anyhow::Result<Side> {
    match target {
        Some(name) => Ok(Side::new(name)),
        None if is_list => Ok(Side::Local),
        None => anyhow::bail!("--target is required (e.g. --target local or --target develop)"),
    }
}

/// rollback サブコマンドを実行する
pub fn run_rollback(args: RollbackArgs, config: AppConfig) -> anyhow::Result<i32> {
    let format = OutputFormat::parse(&args.format)?;
    let side = resolve_target(args.target.as_deref(), args.list)?;

    let mut core = CoreRuntime::new(config.clone());
    core.connect_if_remote(&side)?;

    let target_info = build_source_info(&side, &core)?;

    // セッション一覧取得 + expired マーク
    let mut sessions = core.list_backup_sessions(&side)?;
    mark_expired(
        &mut sessions,
        config.backup.retention_days,
        chrono::Utc::now(),
    );

    if args.list {
        return run_list_mode(&target_info, sessions, format, &mut core);
    }

    // 復元計画
    let plan = plan_restore(
        &sessions,
        args.session.as_deref(),
        &config.filter.sensitive,
        args.force,
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    if args.dry_run {
        return run_dry_run_mode(&target_info, &plan, format, &mut core);
    }

    // 確認プロンプト（--force なし）
    if !args.force {
        let prompt = format!(
            "Restore {} file(s) from session {} to {}? [y/N] ",
            plan.files.len(),
            plan.session_id,
            target_info.label,
        );
        if !confirm_prompt(&prompt)? {
            eprintln!("Aborted.");
            core.disconnect_all();
            return Ok(0);
        }
    }

    // 復元実行
    let mut restored = Vec::new();
    let mut failed: Vec<RollbackFailure> = Vec::new();

    match core.restore_backup(&side, &plan.session_id, &plan.files) {
        Ok((results, failures)) => {
            restored.extend(results);
            failed.extend(failures);
        }
        Err(e) => {
            // 個別ファイルのエラーではなく全体エラー
            for path in &plan.files {
                failed.push(RollbackFailure {
                    path: path.clone(),
                    error: format!("{}", e),
                });
            }
        }
    }

    let skipped: Vec<RollbackSkipped> = plan.skipped;

    let output = RollbackOutput {
        target: target_info,
        session_id: plan.session_id,
        restored,
        skipped,
        failed,
    };

    let code = rollback_exit_code(&output);
    print_rollback_output(&output, format)?;

    core.disconnect_all();
    Ok(code)
}

/// --list モード: セッション一覧を出力して終了
fn run_list_mode(
    target_info: &SourceInfo,
    sessions: Vec<crate::service::types::BackupSession>,
    format: OutputFormat,
    core: &mut CoreRuntime,
) -> anyhow::Result<i32> {
    let output = BackupListOutput {
        target: target_info.clone(),
        sessions,
    };

    match format {
        OutputFormat::Text => println!("{}", format_backup_list_text(&output)),
        OutputFormat::Json => println!("{}", format_json(&output)?),
    }

    core.disconnect_all();
    Ok(0)
}

/// --dry-run モード: 復元計画を出力して終了
fn run_dry_run_mode(
    target_info: &SourceInfo,
    plan: &crate::service::rollback::RestorePlan,
    format: OutputFormat,
    core: &mut CoreRuntime,
) -> anyhow::Result<i32> {
    let output = RollbackOutput {
        target: target_info.clone(),
        session_id: plan.session_id.clone(),
        restored: plan
            .files
            .iter()
            .map(|p| crate::service::types::RollbackFileResult {
                path: p.clone(),
                pre_rollback_backup: None,
            })
            .collect(),
        skipped: plan.skipped.clone(),
        failed: vec![],
    };

    match format {
        OutputFormat::Text => {
            println!("Dry run - would restore from session {}:", plan.session_id);
            for file in &plan.files {
                println!("  \u{2713} {}", file);
            }
            for s in &plan.skipped {
                println!("  - {} (skipped: {})", s.path, s.reason);
            }
            for w in &plan.warnings {
                eprintln!("Warning: {}", w);
            }
        }
        OutputFormat::Json => println!("{}", format_json(&output)?),
    }

    core.disconnect_all();
    Ok(0)
}

/// 確認プロンプトを表示し、ユーザーの応答を返す
fn confirm_prompt(message: &str) -> anyhow::Result<bool> {
    eprint!("{}", message);
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
}

/// RollbackOutput を指定フォーマットで出力する
fn print_rollback_output(output: &RollbackOutput, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => println!("{}", format_rollback_text(output)),
        OutputFormat::Json => println!("{}", format_json(output)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // ── resolve_target ──

    #[test]
    fn test_resolve_target_with_name() {
        let side = resolve_target(Some("develop"), false).unwrap();
        assert_eq!(side, Side::Remote("develop".into()));
    }

    #[test]
    fn test_resolve_target_local() {
        let side = resolve_target(Some("local"), false).unwrap();
        assert_eq!(side, Side::Local);
    }

    #[test]
    fn test_resolve_target_none_list_mode_defaults_to_local() {
        let side = resolve_target(None, true).unwrap();
        assert_eq!(side, Side::Local);
    }

    #[test]
    fn test_resolve_target_none_non_list_mode_errors() {
        let err = resolve_target(None, false).unwrap_err();
        assert!(format!("{}", err).contains("--target is required"));
    }

    // ── invalid format ──

    #[test]
    fn test_invalid_format_rejected() {
        let config = AppConfig {
            servers: std::collections::BTreeMap::new(),
            local: crate::config::LocalConfig::default(),
            filter: crate::config::FilterConfig::default(),
            ssh: crate::config::SshConfig::default(),
            backup: crate::config::BackupConfig::default(),
            agent: crate::config::AgentConfig::default(),
            defaults: crate::config::DefaultsConfig::default(),
        };
        let args = RollbackArgs {
            target: Some("local".into()),
            list: true,
            session: None,
            dry_run: false,
            force: false,
            format: "yaml".into(),
        };
        let err = run_rollback(args, config).unwrap_err();
        assert!(format!("{}", err).contains("Unknown format"));
    }

    // ── additional resolve_target tests ──

    #[test]
    fn test_resolve_target_staging() {
        let side = resolve_target(Some("staging"), false).unwrap();
        assert_eq!(side, Side::Remote("staging".into()));
    }

    // ── rollback_exit_code tests ──

    #[test]
    fn test_rollback_exit_code_success() {
        // restored が非空で failed が空 → 0
        let output = RollbackOutput {
            target: SourceInfo {
                label: "local".into(),
                root: "/tmp".into(),
            },
            session_id: "20260301_120000".into(),
            restored: vec![crate::service::types::RollbackFileResult {
                path: "a.txt".into(),
                pre_rollback_backup: None,
            }],
            skipped: vec![],
            failed: vec![],
        };
        assert_eq!(rollback_exit_code(&output), 0);
    }

    #[test]
    fn test_rollback_exit_code_failure_with_failed() {
        // failed が非空 → 2
        let output = RollbackOutput {
            target: SourceInfo {
                label: "local".into(),
                root: "/tmp".into(),
            },
            session_id: "20260301_120000".into(),
            restored: vec![crate::service::types::RollbackFileResult {
                path: "a.txt".into(),
                pre_rollback_backup: None,
            }],
            skipped: vec![],
            failed: vec![RollbackFailure {
                path: "b.txt".into(),
                error: "permission denied".into(),
            }],
        };
        assert_eq!(rollback_exit_code(&output), 2);
    }

    #[test]
    fn test_rollback_exit_code_empty_restored() {
        // restored が空で failed も空 → 2（restored.is_empty() で非0）
        let output = RollbackOutput {
            target: SourceInfo {
                label: "local".into(),
                root: "/tmp".into(),
            },
            session_id: "20260301_120000".into(),
            restored: vec![],
            skipped: vec![],
            failed: vec![],
        };
        assert_eq!(rollback_exit_code(&output), 2);
    }

    #[test]
    fn test_rollback_exit_code_only_failed() {
        // restored が空、failed のみ → 2
        let output = RollbackOutput {
            target: SourceInfo {
                label: "local".into(),
                root: "/tmp".into(),
            },
            session_id: "20260301_120000".into(),
            restored: vec![],
            skipped: vec![],
            failed: vec![RollbackFailure {
                path: "c.txt".into(),
                error: "not found".into(),
            }],
        };
        assert_eq!(rollback_exit_code(&output), 2);
    }
}
