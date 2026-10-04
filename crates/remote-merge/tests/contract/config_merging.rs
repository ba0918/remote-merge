//! グローバル設定とプロジェクト設定の合成の単位と、省いたキーの既定値
//! （docs/ir/config/precedence.md、docs/ir/config/defaults.md）の契約テスト。
//!
//! どちらもカレントディレクトリにも環境変数にも依存しないため、公開の読み込み関数に
//! 二つの設定のパスを渡し、返った設定の値で確かめる。

use std::fs;

use remote_merge::config::{load_config_from_paths, AppConfig, AuthMethod, StrictHostKeyChecking};

/// どの設定にも書く [local]（[local] がないときの挙動は FLAG-config-002 の範囲のため避ける）
const LOCAL: &str = "[local]\nroot_dir = \"/srv/local\"\n";

/// どちらの設定を置くか
#[derive(Debug, Clone, Copy)]
enum Layout {
    GlobalOnly,
    ProjectOnly,
    Both,
}

/// global と project の本文に [local] を足して書き、置くと決めたものだけを読み込み関数に渡す
fn load(layout: Layout, global: &str, project: &str) -> AppConfig {
    let dir = tempfile::tempdir().unwrap();
    let global_path = dir.path().join("global.toml");
    let project_path = dir.path().join("project.toml");
    fs::write(&global_path, format!("{LOCAL}{global}")).unwrap();
    fs::write(&project_path, format!("{LOCAL}{project}")).unwrap();
    let (global_arg, project_arg) = match layout {
        Layout::GlobalOnly => (Some(global_path.as_path()), None),
        Layout::ProjectOnly => (None, Some(project_path.as_path())),
        Layout::Both => (Some(global_path.as_path()), Some(project_path.as_path())),
    };
    load_config_from_paths(global_arg, project_arg).unwrap()
}

/// 既定値と違う値を全てのキーに書いた [ssh]・[backup]・[agent]
const GLOBAL_NON_DEFAULT: &str = r#"
[ssh]
timeout_sec = 15
strict_host_key_checking = "no"

[backup]
enabled = false
retention_days = 30

[agent]
enabled = false
deploy_dir = "/opt/agent"
timeout_secs = 5
tree_chunk_size = 10
max_file_chunk_bytes = 1024
"#;

// ─── セクションごとの合成（REQ-config-011） ─────────────────

// @kotowari[REQ-config-011]
#[test]
fn project_ssh_section_replaces_the_global_one_and_omitted_keys_use_defaults() {
    let config = load(
        Layout::Both,
        GLOBAL_NON_DEFAULT,
        "[ssh]\ntimeout_sec = 60\n",
    );
    assert_eq!(config.ssh.timeout_sec, 60);
    assert_eq!(
        config.ssh.strict_host_key_checking,
        StrictHostKeyChecking::Ask
    );

    let config = load(
        Layout::Both,
        GLOBAL_NON_DEFAULT,
        "[ssh]\nstrict_host_key_checking = \"yes\"\n",
    );
    assert_eq!(config.ssh.timeout_sec, 300);
    assert_eq!(
        config.ssh.strict_host_key_checking,
        StrictHostKeyChecking::Yes
    );
}

// @kotowari[REQ-config-011]
#[test]
fn project_backup_section_replaces_the_global_one_and_omitted_keys_use_defaults() {
    let config = load(
        Layout::Both,
        GLOBAL_NON_DEFAULT,
        "[backup]\nretention_days = 3\n",
    );
    assert!(config.backup.enabled);
    assert_eq!(config.backup.retention_days, 3);

    let config = load(
        Layout::Both,
        GLOBAL_NON_DEFAULT,
        "[backup]\nenabled = true\n",
    );
    assert!(config.backup.enabled);
    assert_eq!(config.backup.retention_days, 7);
}

// @kotowari[REQ-config-011]
#[test]
fn project_agent_section_replaces_the_global_one_and_omitted_keys_use_defaults() {
    let config = load(
        Layout::Both,
        GLOBAL_NON_DEFAULT,
        "[agent]\ndeploy_dir = \"/srv/agent\"\n",
    );
    assert!(config.agent.enabled);
    assert_eq!(config.agent.deploy_dir, "/srv/agent");
    assert_eq!(config.agent.timeout_secs, 30);
    assert_eq!(config.agent.tree_chunk_size, 1000);
    assert_eq!(config.agent.max_file_chunk_bytes, 4_194_304);
}

// @kotowari[REQ-config-011]
#[test]
fn global_sections_are_used_when_the_project_config_lacks_them() {
    for layout in [Layout::Both, Layout::GlobalOnly] {
        let config = load(layout, GLOBAL_NON_DEFAULT, "");

        assert_eq!(config.ssh.timeout_sec, 15, "{layout:?}");
        assert_eq!(
            config.ssh.strict_host_key_checking,
            StrictHostKeyChecking::No,
            "{layout:?}"
        );
        assert!(!config.backup.enabled, "{layout:?}");
        assert_eq!(config.backup.retention_days, 30, "{layout:?}");
        assert!(!config.agent.enabled, "{layout:?}");
        assert_eq!(config.agent.deploy_dir, "/opt/agent", "{layout:?}");
        assert_eq!(config.agent.timeout_secs, 5, "{layout:?}");
        assert_eq!(config.agent.tree_chunk_size, 10, "{layout:?}");
        assert_eq!(config.agent.max_file_chunk_bytes, 1024, "{layout:?}");
    }
}

// ─── [defaults] のキーごとの合成（REQ-config-012） ────────────

fn defaults_of(global: &str, project: &str) -> (u32, u32) {
    let config = load(
        Layout::Both,
        &format!("[defaults]\n{global}"),
        &format!("[defaults]\n{project}"),
    );
    (
        config.defaults.file_permissions,
        config.defaults.dir_permissions,
    )
}

// @kotowari[REQ-config-012]
#[test]
fn defaults_key_in_the_project_config_wins_over_the_global_one() {
    let both = "file_permissions = \"0o644\"\ndir_permissions = \"0o755\"\n";
    let project = "file_permissions = \"0o600\"\ndir_permissions = \"0o700\"\n";

    assert_eq!(defaults_of(both, project), (0o600, 0o700));
}

// @kotowari[REQ-config-012]
#[test]
fn defaults_key_missing_from_the_project_config_comes_from_the_global_one() {
    let both = "file_permissions = \"0o644\"\ndir_permissions = \"0o755\"\n";

    assert_eq!(
        defaults_of(both, "file_permissions = \"0o600\"\n"),
        (0o600, 0o755)
    );
    assert_eq!(
        defaults_of(both, "dir_permissions = \"0o700\"\n"),
        (0o644, 0o700)
    );
}

// @kotowari[REQ-config-012]
#[test]
fn global_defaults_keys_are_used_when_the_project_config_lacks_the_section() {
    let global = "[defaults]\nfile_permissions = \"0o640\"\ndir_permissions = \"0o750\"\n";

    for layout in [Layout::Both, Layout::GlobalOnly] {
        let config = load(layout, global, "");
        assert_eq!(config.defaults.file_permissions, 0o640, "{layout:?}");
        assert_eq!(config.defaults.dir_permissions, 0o750, "{layout:?}");
    }
}

// @kotowari[REQ-config-012]
#[test]
fn defaults_key_in_neither_config_uses_the_default_value() {
    assert_eq!(
        defaults_of(
            "file_permissions = \"0o644\"\n",
            "file_permissions = \"0o600\"\n"
        ),
        (0o600, 0o775)
    );
    assert_eq!(
        defaults_of(
            "dir_permissions = \"0o755\"\n",
            "dir_permissions = \"0o700\"\n"
        ),
        (0o664, 0o700)
    );
}

// ─── 省いたキーの既定値（REQ-config-013、TBL-config-001） ─────

/// TBL-config-001 の [ssh]・[backup]・[agent]・[defaults] の 11 行
fn assert_section_defaults(config: &AppConfig, case: &str) {
    assert_eq!(config.ssh.timeout_sec, 300, "{case}");
    assert_eq!(
        config.ssh.strict_host_key_checking,
        StrictHostKeyChecking::Ask,
        "{case}"
    );
    assert!(config.backup.enabled, "{case}");
    assert_eq!(config.backup.retention_days, 7, "{case}");
    assert!(config.agent.enabled, "{case}");
    assert_eq!(config.agent.deploy_dir, "/var/tmp", "{case}");
    assert_eq!(config.agent.timeout_secs, 30, "{case}");
    assert_eq!(config.agent.tree_chunk_size, 1000, "{case}");
    assert_eq!(config.agent.max_file_chunk_bytes, 4_194_304, "{case}");
    assert_eq!(config.defaults.file_permissions, 0o664, "{case}");
    assert_eq!(config.defaults.dir_permissions, 0o775, "{case}");
}

/// キーを一つも書かない [ssh]・[backup]・[agent]・[defaults]
const EMPTY_SECTIONS: &str = "[ssh]\n[backup]\n[agent]\n[defaults]\n";

// @kotowari[REQ-config-013]
#[test]
fn omitted_server_keys_use_the_default_port_auth_and_sudo() {
    let server = "[servers.develop]\nhost = \"dev.example.invalid\"\nuser = \"deploy\"\nroot_dir = \"/srv/app\"\n";
    for (layout, global, project) in [
        (Layout::GlobalOnly, server, ""),
        (Layout::ProjectOnly, "", server),
        (Layout::Both, "", server),
    ] {
        let config = load(layout, global, project);
        let develop = &config.servers["develop"];
        assert_eq!(develop.port, 22, "{layout:?}");
        assert_eq!(develop.auth, AuthMethod::Key, "{layout:?}");
        assert!(!develop.sudo, "{layout:?}");
    }
}

// @kotowari[REQ-config-013]
#[test]
fn absent_sections_use_the_default_values() {
    for layout in [Layout::GlobalOnly, Layout::ProjectOnly, Layout::Both] {
        let config = load(layout, "", "");
        assert_section_defaults(&config, &format!("{layout:?}"));
    }
}

// @kotowari[REQ-config-013]
#[test]
fn sections_with_every_key_omitted_use_the_default_values() {
    for (case, layout, global, project) in [
        ("global config only", Layout::GlobalOnly, EMPTY_SECTIONS, ""),
        (
            "project config only",
            Layout::ProjectOnly,
            "",
            EMPTY_SECTIONS,
        ),
        (
            "both configs, sections in the project",
            Layout::Both,
            "",
            EMPTY_SECTIONS,
        ),
        (
            "both configs, sections in the global",
            Layout::Both,
            EMPTY_SECTIONS,
            "",
        ),
        (
            "both configs, sections in both",
            Layout::Both,
            EMPTY_SECTIONS,
            EMPTY_SECTIONS,
        ),
    ] {
        let config = load(layout, global, project);
        assert_section_defaults(&config, case);
    }
}

// ─── 走査の上限のキーごとの選び方と既定値（REQ-config-027、TBL-config-001） ─────

/// トップレベルのキーを [local] より前に書いて読み込む（[local] の後に書くと [local] の中のキーになる）
fn load_top_level(global: Option<&str>, project: Option<&str>) -> AppConfig {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, top: &str| {
        let path = dir.path().join(name);
        fs::write(&path, format!("{top}{LOCAL}")).unwrap();
        path
    };
    let global_path = global.map(|top| write("global.toml", top));
    let project_path = project.map(|top| write("project.toml", top));
    load_config_from_paths(global_path.as_deref(), project_path.as_deref()).unwrap()
}

fn scan_limits(config: &AppConfig) -> (usize, usize) {
    (config.max_scan_entries, config.badge_scan_max_files)
}

// @kotowari[REQ-config-027]
#[test]
fn scan_limit_keys_are_chosen_per_key_from_the_project_then_the_global_config() {
    // それぞれのキーを片方にだけ書く: どちらも反映される
    let config = load_top_level(
        Some("max_scan_entries = 1234\n"),
        Some("badge_scan_max_files = 77\n"),
    );
    assert_eq!(scan_limits(&config), (1234, 77));

    // 両方に書く: プロジェクト側を使う
    let both = "max_scan_entries = 1234\nbadge_scan_max_files = 77\n";
    let project = "max_scan_entries = 4321\nbadge_scan_max_files = 88\n";
    let config = load_top_level(Some(both), Some(project));
    assert_eq!(scan_limits(&config), (4321, 88));

    // どちらにも書かない: 既定値を使う
    let config = load_top_level(Some(""), Some(""));
    assert_eq!(scan_limits(&config), (50_000, 500));
}

// @kotowari[REQ-config-013]
#[test]
fn omitted_scan_limit_keys_use_the_default_values() {
    for (case, global, project) in [
        ("global config only", Some(""), None),
        ("project config only", None, Some("")),
        ("both configs", Some(""), Some("")),
    ] {
        let config = load_top_level(global, project);
        assert_eq!(scan_limits(&config), (50_000, 500), "{case}");
    }
}
