//! UI 状態の永続化（~/.config/remote-merge/state.toml）。
//! lazygit 方式: 設定(config.toml)とは別に、テーマ名などの UI 状態を保存する。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 永続化する UI 状態。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedState {
    /// 選択中のシンタックスハイライトテーマ名
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    crate::theme::DEFAULT_THEME.to_string()
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

/// state.toml のパスを返す。
pub fn state_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("remote-merge").join("state.toml"))
}

/// state.toml を読み込む。ファイルが存在しない・パース失敗時はデフォルトを返す。
pub fn load_state() -> PersistedState {
    let Some(path) = state_file_path() else {
        return PersistedState::default();
    };
    if !path.exists() {
        return PersistedState::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => PersistedState::default(),
    }
}

/// state.toml に状態を保存する。ディレクトリがなければ作成する。
/// 保存失敗は無視する（UI 状態の保存失敗でクラッシュさせない）。
pub fn save_state(state: &PersistedState) {
    let Some(path) = state_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(state) {
        let _ = std::fs::write(&path, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_state() {
        let state = PersistedState::default();
        assert_eq!(state.theme, crate::theme::DEFAULT_THEME);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let state = PersistedState {
            theme: "InspiredGitHub".to_string(),
        };
        let toml_str = toml::to_string_pretty(&state).unwrap();
        let restored: PersistedState = toml::from_str(&toml_str).unwrap();
        assert_eq!(state, restored);
    }

    #[test]
    fn test_deserialize_empty_uses_default() {
        let restored: PersistedState = toml::from_str("").unwrap();
        assert_eq!(restored.theme, crate::theme::DEFAULT_THEME);
    }

    #[test]
    fn test_save_and_load_to_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.toml");

        let state = PersistedState {
            theme: "Solarized (dark)".to_string(),
        };
        let content = toml::to_string_pretty(&state).unwrap();
        std::fs::write(&path, &content).unwrap();

        let loaded: PersistedState =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.theme, "Solarized (dark)");
    }

    #[test]
    fn test_deserialize_unknown_fields_ignored() {
        let toml_str = r#"
theme = "base16-ocean.dark"
unknown_field = "value"
"#;
        let restored: PersistedState = toml::from_str(toml_str).unwrap();
        assert_eq!(restored.theme, "base16-ocean.dark");
    }
}
