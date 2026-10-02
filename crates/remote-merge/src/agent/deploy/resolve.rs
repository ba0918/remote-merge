use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};
use tracing::{info, warn};

use super::remote_target::current_target;

/// バイナリパスを解決する。
///
/// `override_path` が `Some` の場合はそのパスを検証して返す。
/// - ファイルが存在すること
/// - 実行可能であること（Unix のみ）
/// - パストラバーサル（`../`）を含まないこと
///
/// `None` の場合は `current_exe` をそのまま返す。
pub fn resolve_binary_path(override_path: Option<&str>, current_exe: &Path) -> Result<PathBuf> {
    let Some(raw) = override_path else {
        return Ok(current_exe.to_path_buf());
    };

    let path = PathBuf::from(raw);

    // パストラバーサル防止
    if path.components().any(|c| c == Component::ParentDir) {
        bail!("binary path contains path traversal component (..): {raw}");
    }

    // ファイル存在確認
    if !path.is_file() {
        bail!("binary path not found or is not a file: {raw}");
    }

    // Unix: 実行ビット確認
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path)?;
        if meta.permissions().mode() & 0o111 == 0 {
            bail!("binary path is not executable: {raw}");
        }
    }

    Ok(path)
}

/// 解決されたバイナリの情報
#[derive(Debug)]
pub struct ResolvedBinary {
    pub path: PathBuf,
    pub source: ResolutionSource,
}

/// バイナリがどこから解決されたかを示す
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionSource {
    /// REMOTE_MERGE_AGENT_BINARY 環境変数
    EnvVar,
    /// agents/ ディレクトリ
    AgentDir(PathBuf),
    /// 同一ターゲット時の自己参照
    CurrentExe,
}

/// Agent バイナリを解決する（公開 API）。
///
/// 環境変数やファイルシステムから情報を収集し、
/// 純粋関数 `resolve_agent_binary_with` に委譲する。
pub fn resolve_agent_binary(remote_target: &str) -> Result<ResolvedBinary> {
    let env_binary = std::env::var("REMOTE_MERGE_AGENT_BINARY").ok();
    let exe_path = std::env::current_exe().unwrap_or_default();
    let home_dir = dirs::home_dir();
    let env_override = std::env::var("REMOTE_MERGE_AGENT_DIR").ok();
    let local_target = current_target();

    let ctx = ResolveContext {
        remote_target,
        env_binary: env_binary.as_deref(),
        exe_path: &exe_path,
        home_dir: home_dir.as_deref(),
        env_override: env_override.as_deref(),
        local_target,
    };
    let result = resolve_agent_binary_with(&ctx, |p| p.is_file(), validate_agent_binary);

    // ロギング（副作用をサービス層に集約）
    match &result {
        Ok(resolved) => {
            info!(
                path = %resolved.path.display(),
                source = ?resolved.source,
                "Resolved agent binary"
            );
        }
        Err(e) => {
            warn!(error = %e, "Failed to resolve agent binary");
        }
    }

    result
}

/// `resolve_agent_binary_with` の入力パラメータ。
///
/// I/O から収集した値を構造体にまとめてテスト可能にする。
pub struct ResolveContext<'a> {
    pub remote_target: &'a str,
    pub env_binary: Option<&'a str>,
    pub exe_path: &'a Path,
    pub home_dir: Option<&'a Path>,
    pub env_override: Option<&'a str>,
    pub local_target: &'a str,
}

/// Agent バイナリ解決の純粋ロジック。
///
/// I/O を引数で注入し、テスト可能にする。
///
/// 優先順位:
/// 1. `env_binary` (`REMOTE_MERGE_AGENT_BINARY` 環境変数)
/// 2. `agent_dir_candidates()` から `remote-merge-{target}` を探索
/// 3. `exe_path` — `local_target == remote_target` の場合のみ
pub fn resolve_agent_binary_with<F, V>(
    ctx: &ResolveContext<'_>,
    file_exists: F,
    validate: V,
) -> Result<ResolvedBinary>
where
    F: Fn(&Path) -> bool,
    V: Fn(&Path) -> Result<()>,
{
    // 優先順位1: 環境変数
    if let Some(env_path) = ctx.env_binary {
        let path = PathBuf::from(env_path);
        validate(&path)?;
        return Ok(ResolvedBinary {
            path,
            source: ResolutionSource::EnvVar,
        });
    }

    // 優先順位2: agents/ ディレクトリ探索
    let candidates = agent_dir_candidates(ctx.exe_path, ctx.home_dir, ctx.env_override);

    let binary_name = format!("remote-merge-{}", ctx.remote_target);
    for dir in &candidates {
        let candidate = dir.join(&binary_name);
        if file_exists(&candidate) {
            if let Err(_e) = validate(&candidate) {
                continue;
            }
            return Ok(ResolvedBinary {
                path: candidate,
                source: ResolutionSource::AgentDir(dir.clone()),
            });
        }
    }

    // 優先順位3: 同一ターゲットの場合は自己参照
    if ctx.local_target == ctx.remote_target {
        return Ok(ResolvedBinary {
            path: ctx.exe_path.to_path_buf(),
            source: ResolutionSource::CurrentExe,
        });
    }

    // 見つからない
    let candidate_strs: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
    bail!(
        "No agent binary found for target '{}'. \
         Install the agent binary to one of: {dirs}. \
         Or set REMOTE_MERGE_AGENT_BINARY environment variable.",
        ctx.remote_target,
        dirs = candidate_strs.join(", ")
    );
}

/// agents/ ディレクトリの候補一覧を返す純粋関数。
///
/// テスタビリティのため I/O を引数で注入する。
/// 候補順序:
/// 1. `env_override` (`REMOTE_MERGE_AGENT_DIR`) — 絶対パスかつ `..` を含まないこと
/// 2. `{exe_dir}/../share/remote-merge/agents/` — exe 隣接
/// 3. `{home_dir}/.local/share/remote-merge/agents/` — XDG 準拠
/// 4. `/usr/local/share/remote-merge/agents/` — システムワイド
pub fn agent_dir_candidates(
    exe_path: &Path,
    home_dir: Option<&Path>,
    env_override: Option<&str>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. 環境変数オーバーライド
    if let Some(env_dir) = env_override {
        let p = Path::new(env_dir);
        if !p.is_absolute() {
            warn!(
                path = env_dir,
                "REMOTE_MERGE_AGENT_DIR is a relative path, ignoring"
            );
        } else if p.components().any(|c| c == Component::ParentDir) {
            warn!(
                path = env_dir,
                "REMOTE_MERGE_AGENT_DIR contains '..', ignoring"
            );
        } else {
            dirs.push(p.to_path_buf());
        }
    }

    // 2. exe 隣接: {exe_dir}/../share/remote-merge/agents/
    if let Some(exe_dir) = exe_path.parent() {
        dirs.push(exe_dir.join("../share/remote-merge/agents"));
    }

    // 3. XDG 準拠: {home}/.local/share/remote-merge/agents/
    if let Some(home) = home_dir {
        dirs.push(home.join(".local/share/remote-merge/agents"));
    }

    // 4. システムワイド
    dirs.push(PathBuf::from("/usr/local/share/remote-merge/agents"));

    dirs
}

/// Agent バイナリファイルを検証する。
///
/// - ファイル存在確認
/// - 100MB 超 → エラー
/// - 20MB 超 → 警告ログ
/// - Unix: 実行ビット確認
pub fn validate_agent_binary(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!(
            "Agent binary not found or is not a file: {}",
            path.display()
        );
    }

    let metadata = std::fs::metadata(path)?;
    let size_bytes = metadata.len();
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);

    if size_mb > 100.0 {
        bail!(
            "Agent binary exceeds 100MB size limit: {size_mb:.1}MB. \
             This is likely not a valid agent binary."
        );
    }

    if size_mb > 20.0 {
        warn!(
            path = %path.display(),
            size_mb = format!("{size_mb:.1}"),
            "Agent binary is {size_mb:.1}MB — this might be a debug build"
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o111 == 0 {
            bail!(
                "Agent binary is not executable: {} (mode: {:o})",
                path.display(),
                mode
            );
        }
    }

    Ok(())
}

/// ローカルの実行バイナリパスを取得する。
///
/// 環境変数 `REMOTE_MERGE_AGENT_BINARY` が設定されている場合はそのパスを使用する。
pub fn local_binary_path() -> Result<PathBuf> {
    let override_path = std::env::var("REMOTE_MERGE_AGENT_BINARY").ok();
    let exe = std::env::current_exe()?;
    resolve_binary_path(override_path.as_deref(), &exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_binary_path_returns_valid_path() {
        let path = local_binary_path().unwrap();
        assert!(path.is_absolute());
    }

    // --- resolve_binary_path (D-2) ---

    #[test]
    fn resolve_binary_path_without_override_returns_current_exe() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let result = resolve_binary_path(None, &fake_exe).unwrap();
        assert_eq!(result, fake_exe);
    }

    #[test]
    fn resolve_binary_path_nonexistent_path_returns_error() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some("/nonexistent/path/binary"), &fake_exe).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error: {err}"
        );
    }

    #[test]
    fn resolve_binary_path_parent_dir_traversal_returns_error() {
        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some("../something"), &fake_exe).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("path traversal") || msg.contains(".."),
            "expected traversal error in: {msg}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_path_with_override_returns_that_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("fake-binary");
        std::fs::write(&bin_path, b"#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let result = resolve_binary_path(Some(bin_path.to_str().unwrap()), &fake_exe).unwrap();
        assert_eq!(result, bin_path);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_path_non_executable_returns_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("not-executable");
        std::fs::write(&bin_path, b"data").unwrap();
        let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&bin_path, perms).unwrap();

        let fake_exe = PathBuf::from("/usr/bin/remote-merge");
        let err = resolve_binary_path(Some(bin_path.to_str().unwrap()), &fake_exe).unwrap_err();
        assert!(
            err.to_string().contains("not executable"),
            "expected 'not executable' in error: {err}"
        );
    }

    // --- agent_dir_candidates ---

    #[test]
    fn agent_dir_candidates_all_sources() {
        let exe = PathBuf::from("/usr/bin/remote-merge");
        let home = PathBuf::from("/home/user");
        let dirs = agent_dir_candidates(&exe, Some(&home), None);
        assert_eq!(dirs.len(), 3);
        assert_eq!(
            dirs[0],
            PathBuf::from("/usr/bin/../share/remote-merge/agents")
        );
        assert_eq!(
            dirs[1],
            PathBuf::from("/home/user/.local/share/remote-merge/agents")
        );
        assert_eq!(
            dirs[2],
            PathBuf::from("/usr/local/share/remote-merge/agents")
        );
    }

    #[test]
    fn agent_dir_candidates_with_env_override() {
        let exe = PathBuf::from("/usr/bin/remote-merge");
        let dirs = agent_dir_candidates(&exe, None, Some("/opt/my-agents"));
        assert_eq!(dirs[0], PathBuf::from("/opt/my-agents"));
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn agent_dir_candidates_no_home_dir() {
        let exe = PathBuf::from("/usr/bin/remote-merge");
        let dirs = agent_dir_candidates(&exe, None, None);
        assert_eq!(dirs.len(), 2);
        assert_eq!(
            dirs[0],
            PathBuf::from("/usr/bin/../share/remote-merge/agents")
        );
        assert_eq!(
            dirs[1],
            PathBuf::from("/usr/local/share/remote-merge/agents")
        );
    }

    #[test]
    fn agent_dir_candidates_relative_env_override_skipped() {
        let exe = PathBuf::from("/usr/bin/remote-merge");
        let dirs = agent_dir_candidates(&exe, None, Some("relative/path"));
        assert_eq!(dirs.len(), 2);
        assert!(!dirs.iter().any(|d| d == Path::new("relative/path")));
    }

    #[test]
    fn agent_dir_candidates_dotdot_env_override_skipped() {
        let exe = PathBuf::from("/usr/bin/remote-merge");
        let dirs = agent_dir_candidates(&exe, None, Some("/opt/../sneaky"));
        assert_eq!(dirs.len(), 2);
        assert!(!dirs.iter().any(|d| d == Path::new("/opt/../sneaky")));
    }

    #[test]
    fn agent_dir_candidates_exe_path_based() {
        let exe = PathBuf::from("/opt/tools/bin/remote-merge");
        let dirs = agent_dir_candidates(&exe, None, None);
        assert_eq!(
            dirs[0],
            PathBuf::from("/opt/tools/bin/../share/remote-merge/agents")
        );
    }

    // --- validate_agent_binary ---

    #[cfg(unix)]
    #[test]
    fn validate_agent_binary_valid_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("agent");
        std::fs::write(&bin, b"#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin, perms).unwrap();

        assert!(validate_agent_binary(&bin).is_ok());
    }

    #[test]
    fn validate_agent_binary_nonexistent() {
        let result = validate_agent_binary(Path::new("/nonexistent/agent"));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not found"),
            "should mention 'not found'"
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_agent_binary_not_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("not-exec");
        std::fs::write(&bin, b"data").unwrap();
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&bin, perms).unwrap();

        let result = validate_agent_binary(&bin);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not executable"),
            "should mention 'not executable'"
        );
    }

    // --- resolve_agent_binary_with (純粋関数テスト) ---

    fn ok_validate(_: &Path) -> Result<()> {
        Ok(())
    }

    fn fail_validate(_: &Path) -> Result<()> {
        bail!("validation failed")
    }

    fn make_ctx<'a>(
        remote_target: &'a str,
        env_binary: Option<&'a str>,
        exe_path: &'a Path,
        home_dir: Option<&'a Path>,
        local_target: &'a str,
    ) -> ResolveContext<'a> {
        ResolveContext {
            remote_target,
            env_binary,
            exe_path,
            home_dir,
            env_override: None,
            local_target,
        }
    }

    #[test]
    fn resolve_with_env_binary_takes_priority() {
        let ctx = make_ctx(
            "x86_64-unknown-linux-musl",
            Some("/usr/bin/agent"),
            Path::new("/usr/bin/remote-merge"),
            None,
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(&ctx, |_| true, ok_validate).unwrap();
        assert_eq!(result.source, ResolutionSource::EnvVar);
        assert_eq!(result.path, PathBuf::from("/usr/bin/agent"));
    }

    #[test]
    fn resolve_with_env_binary_validation_fails() {
        let ctx = make_ctx(
            "x86_64-unknown-linux-musl",
            Some("/usr/bin/bad-agent"),
            Path::new("/usr/bin/remote-merge"),
            None,
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(&ctx, |_| true, fail_validate);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_with_agents_dir() {
        let ctx = make_ctx(
            "aarch64-unknown-linux-musl",
            None,
            Path::new("/usr/bin/remote-merge"),
            Some(Path::new("/home/user")),
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(
            &ctx,
            |p| p.ends_with("remote-merge-aarch64-unknown-linux-musl"),
            ok_validate,
        )
        .unwrap();
        assert!(matches!(result.source, ResolutionSource::AgentDir(_)));
    }

    #[test]
    fn resolve_with_same_target_uses_current_exe() {
        let ctx = make_ctx(
            "x86_64-unknown-linux-musl",
            None,
            Path::new("/usr/bin/remote-merge"),
            None,
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(&ctx, |_| false, ok_validate).unwrap();
        assert_eq!(result.source, ResolutionSource::CurrentExe);
        assert_eq!(result.path, PathBuf::from("/usr/bin/remote-merge"));
    }

    #[test]
    fn resolve_with_different_target_no_binary_returns_error() {
        let ctx = make_ctx(
            "aarch64-unknown-freebsd",
            None,
            Path::new("/usr/bin/remote-merge"),
            None,
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(&ctx, |_| false, ok_validate);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("No agent binary found"), "{msg}");
        assert!(msg.contains("aarch64-unknown-freebsd"), "{msg}");
        assert!(msg.contains("REMOTE_MERGE_AGENT_BINARY"), "{msg}");
    }

    #[test]
    fn resolve_with_agents_dir_validation_fails_falls_through() {
        let ctx = make_ctx(
            "x86_64-unknown-linux-musl",
            None,
            Path::new("/usr/bin/remote-merge"),
            None,
            "x86_64-unknown-linux-musl",
        );
        let result = resolve_agent_binary_with(&ctx, |_| true, fail_validate).unwrap();
        assert_eq!(result.source, ResolutionSource::CurrentExe);
    }

    // --- current_target ---

    #[test]
    fn current_target_returns_non_empty() {
        let target = current_target();
        assert!(!target.is_empty(), "target should not be empty");
        assert!(target.contains('-'), "target should contain '-': {target}");
    }
}
