use crate::merge::merge_configs;
use crate::schema::RawConfig;
use crate::{AppConfig, error::AppError, expand_tilde, parse_error};
use anyhow::bail;
use std::path::{Path, PathBuf};

/// グローバル設定ファイルのパスを返す
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("remote-merge").join("config.toml"))
}

/// プロジェクト設定ファイルのパスを返す
pub fn project_config_path() -> PathBuf {
    PathBuf::from(".remote-merge.toml")
}

/// 設定ファイルを読み込んでマージする
///
/// マージ戦略:
/// - `[servers.*]`: プロジェクト設定で上書き
/// - `[local]`: プロジェクト設定で上書き
/// - `[filter].exclude`: 和集合
/// - `[ssh]`: プロジェクト設定で上書き
/// - `[backup]`: プロジェクト設定で上書き
pub fn load_config() -> crate::error::Result<AppConfig> {
    load_config_with_project_override(None)
}

/// プロジェクト設定ファイルのパスを上書きして設定を読み込む。
///
/// `project_override` が `Some` の場合、指定パスをプロジェクト設定として使用し、
/// CWD の `.remote-merge.toml` は無視する。グローバル設定は常にマージされる。
pub fn load_config_with_project_override(
    project_override: Option<&Path>,
) -> crate::error::Result<AppConfig> {
    let global_path = global_config_path();
    let project_path = match project_override {
        Some(p) => {
            // 文言には絶対パスを示すが、OS には書かれた形のまま渡す。作業ディレクトリにつないだ
            // 絶対パスは、相対のパスで読めるときにも PATH_MAX を超えることがある（A64）
            if !p.exists() {
                anyhow::bail!("Config file not found: {}", path_for_messages(p)?.display());
            }
            if !p.is_file() {
                anyhow::bail!(
                    "Config path is not a regular file: {}",
                    path_for_messages(p)?.display()
                );
            }
            Some(p.to_path_buf())
        }
        None => {
            let default = project_config_path();
            if default.exists() {
                Some(default)
            } else {
                None
            }
        }
    };
    load_config_from_paths(global_path.as_deref(), project_path.as_deref())
}

/// 指定パスから設定ファイルを読み込む（テスト用にpub）
pub fn load_config_from_paths(
    global_path: Option<&Path>,
    project_path: Option<&Path>,
) -> crate::error::Result<AppConfig> {
    let global_raw = global_path
        .filter(|p| p.exists())
        .map(load_raw_config)
        .transpose()?;
    let project_raw = project_path
        .filter(|p| p.exists())
        .map(load_raw_config)
        .transpose()?;
    if global_raw.is_none() && project_raw.is_none() {
        let gp = global_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~/.config/remote-merge/config.toml"));
        let pp = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".remote-merge.toml");
        bail!(AppError::ConfigNotFound {
            project_path: pp,
            global_path: gp
        });
    }
    merge_configs(global_raw, project_raw)
}

fn load_raw_config(path: &Path) -> crate::error::Result<RawConfig> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            let shown = path_for_messages(path)?;
            return Err(anyhow::Error::new(error)
                .context(format!("Failed to read config file: {}", shown.display())));
        }
    };
    let mut raw: RawConfig = match toml::from_str(&content) {
        Ok(raw) => raw,
        Err(error) => bail!(AppError::ConfigParse {
            path: path_for_messages(path)?,
            detail: parse_error::describe(&content, &error),
        }),
    };
    if let Some(local) = raw.local.as_mut() {
        local.config_path = path.to_path_buf();
    }
    Ok(raw)
}

/// エラーの文言に示す設定ファイルのパス。相対のパスはカレントディレクトリにつなぐ（A64）。
/// 作業ディレクトリが深くても読み込みを止めないよう、エラーを示すときだけ求める
fn path_for_messages(path: &Path) -> crate::error::Result<PathBuf> {
    Ok(if path.is_relative() {
        std::env::current_dir()?.join(path)
    } else {
        path.to_path_buf()
    })
}

/// 相対の [local] root_dir の基準にする、設定ファイルのあるディレクトリ。書かれた設定ファイルのパス
/// `path` を OS に実パスへ直させたものの親とする（REQ-config-040、A65）。実パスに直せないときは、
/// 絶対パスにした `path` を文言に示して設定の読み込みを止める
fn config_directory(path: &Path) -> crate::error::Result<PathBuf> {
    let real = match std::fs::canonicalize(path) {
        Ok(real) => real,
        Err(error) => {
            let shown = path_for_messages(path)?;
            return Err(anyhow::Error::new(error).context(format!(
                "Failed to resolve the directory of config file: {}",
                shown.display()
            )));
        }
    };
    Ok(real
        .parent()
        .map_or_else(|| real.clone(), Path::to_path_buf))
}

/// [local] の root_dir を解決する。ホーム展開後も相対なら、設定ファイルの実ディレクトリを基準とする
/// （REQ-config-010、REQ-config-040）。".." は辿るときに解決するので残す（A66）
pub(crate) fn resolve_local_root(
    root_dir: &str,
    config_path: &Path,
) -> crate::error::Result<PathBuf> {
    let expanded = expand_tilde(root_dir);
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    Ok(config_directory(config_path)?
        .join(expanded)
        .components()
        .collect())
}
