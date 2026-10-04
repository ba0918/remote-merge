#![cfg(unix)]
//! 設定の値の読み方と検査（docs/ir/config/values.md）のうち、関数呼び出しで観測できるものの契約テスト。
//!
//! 値の読み方とエラーの文言は、公開の読み込み関数に一時ディレクトリの設定ファイルを渡し、
//! 返った値か、原因の連鎖を含めたエラーの表示で確かめる。
//! 新しく作るものの権限と --max-entries は、サーバをローカルのディレクトリに差し替えたコマンドの関数呼び出しで確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;

use remote_merge::cli::diff::{execute_diff, DiffArgs};
use remote_merge::cli::merge::{execute_merge, MergeArgs, MergeCommandOutput};
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::{load_config_from_paths, AppConfig, StrictHostKeyChecking};
use remote_merge::runtime::RuntimeTargets;
use tempfile::TempDir;

use super::{merge_support, status_support, sync_support};

/// どの設定にも書く [local]（[local] がないときの挙動は FLAG-config-002 の範囲のため避ける）。
///
/// トップレベルのキー（max_scan_entries など）を [local] の中に入れないよう、本文の後ろに足す。
const LOCAL: &str = "[local]\nroot_dir = \"/srv/local\"\n";

/// 正しい値だけを持つサーバ develop の、キーを差し替えられる本文
fn server(port: &str, auth: &str, root_dir: &str, extra: &str) -> String {
    format!(
        "[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\n{port}{auth}root_dir = {root_dir:?}\n{extra}"
    )
}

fn plain_server(extra: &str) -> String {
    server("", "", "/srv/remote", extra)
}

/// `body` の後ろに [local] を足した設定を読む
fn load(body: &str) -> remote_merge::error::Result<AppConfig> {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, format!("{body}\n{LOCAL}")).unwrap();
    load_config_from_paths(Some(&path), None)
}

/// 読めるはずの設定を読む
fn load_ok(body: &str) -> AppConfig {
    load(body).unwrap_or_else(|err| panic!("expected {body:?} to load, got {err:#}"))
}

/// 止まるはずの設定のエラーを、原因の連鎖を含めて表示した文字列
fn load_error(body: &str) -> String {
    match load(body) {
        Ok(config) => panic!("expected {body:?} to stop, got {config:?}"),
        Err(err) => format!("{err:#}"),
    }
}

// ─── サーバの値（REQ-config-014、TBL-config-002） ─────────────────

// @kotowari[REQ-config-014]
#[test]
fn server_port_zero_stops_with_the_port_reason() {
    assert_eq!(
        load_error(&server("port = 0\n", "", "/srv/remote", "")),
        "Invalid config value: servers.develop.port - port must be >= 1"
    );
}

// @kotowari[REQ-config-014]
#[test]
fn server_auth_other_than_key_or_password_stops_with_the_auth_reason() {
    for value in ["token", "Password", ""] {
        assert_eq!(
            load_error(&server("", &format!("auth = {value:?}\n"), "/srv/remote", "")),
            format!(
                "Invalid config value: servers.develop.auth - invalid auth value: '{value}' (expected 'key' or 'password')"
            )
        );
    }
}

// @kotowari[REQ-config-014]
#[test]
fn server_empty_root_dir_stops_with_the_root_dir_reason() {
    assert_eq!(
        load_error(&server("", "", "", "")),
        "Invalid config value: servers.develop.root_dir - root_dir must not be empty"
    );
}

// ─── パーミッションの読み方（REQ-config-015） ─────────────────

/// パーミッションを書ける四つのキー。(キー名, そのキーだけを書いた設定の本文を作る関数, 読んだ値を取り出す関数)
type PermissionKey = (
    &'static str,
    fn(&str) -> String,
    fn(&AppConfig) -> Option<u32>,
);

const PERMISSION_KEYS: [PermissionKey; 4] = [
    (
        "servers.develop.file_permissions",
        |value| plain_server(&format!("file_permissions = {value:?}\n")),
        |config| config.servers["develop"].file_permissions,
    ),
    (
        "servers.develop.dir_permissions",
        |value| plain_server(&format!("dir_permissions = {value:?}\n")),
        |config| config.servers["develop"].dir_permissions,
    ),
    (
        "defaults.file_permissions",
        |value| {
            format!(
                "{}[defaults]\nfile_permissions = {value:?}\n",
                plain_server("")
            )
        },
        |config| Some(config.defaults.file_permissions),
    ),
    (
        "defaults.dir_permissions",
        |value| {
            format!(
                "{}[defaults]\ndir_permissions = {value:?}\n",
                plain_server("")
            )
        },
        |config| Some(config.defaults.dir_permissions),
    ),
];

// @kotowari[REQ-config-015]
#[test]
fn permissions_with_or_without_the_0o_prefix_are_read_as_octal() {
    // 0o664 は defaults.file_permissions の既定値でもあるため、既定値と違う 0o751 でも確かめる
    for (key, body, read) in PERMISSION_KEYS {
        for (digits, expected) in [("664", 0o664), ("751", 0o751)] {
            for value in [
                format!("0o{digits}"),
                format!("0{digits}"),
                digits.to_string(),
            ] {
                assert_eq!(
                    read(&load_ok(&body(&value))),
                    Some(expected),
                    "{key} = {value:?}"
                );
            }
        }
    }
}

// @kotowari[REQ-config-015]
#[test]
fn permissions_with_a_non_octal_digit_stop_with_the_key_name() {
    for (key, body, _) in PERMISSION_KEYS {
        for value in ["0o668", "rw-r--r--", "0o-64"] {
            assert_eq!(
                load_error(&body(value)),
                format!(
                    "Invalid config value: {key} - invalid permissions string: '{value}' (must contain only octal digits 0-7)"
                )
            );
        }
    }
}

// @kotowari[REQ-config-015]
#[test]
fn permissions_without_digits_stop_with_the_key_name() {
    for (key, body, _) in PERMISSION_KEYS {
        for value in ["0o", ""] {
            assert_eq!(
                load_error(&body(value)),
                format!(
                    "Invalid config value: {key} - invalid permissions string: '{value}' (empty octal digits)"
                )
            );
        }
    }
}

// @kotowari[REQ-config-015]
#[test]
fn permissions_above_0o777_stop_with_the_key_name() {
    for (key, body, _) in PERMISSION_KEYS {
        for (value, octal) in [("0o1000", "1000"), ("4755", "4755")] {
            assert_eq!(
                load_error(&body(value)),
                format!(
                    "Invalid config value: {key} - invalid permissions value: '{value}' (must be <= 0o777, got 0o{octal})"
                )
            );
        }
    }
}

// ─── 新しく作るものの権限（REQ-config-016） ─────────────────

/// 権限の書き方の三通り。サーバと [defaults] の値は、既定値とも一般的な umask の結果とも違う値にする
#[derive(Debug, Clone, Copy)]
enum PermissionSource {
    /// サーバと [defaults] の両方に書く（サーバの値が選ばれる）
    Server,
    /// [defaults] にだけ書く
    Defaults,
    /// どちらにも書かない（既定値が選ばれる）
    Neither,
}

impl PermissionSource {
    fn server_lines(self) -> &'static str {
        match self {
            Self::Server => "file_permissions = \"0o640\"\ndir_permissions = \"0o750\"\n",
            Self::Defaults | Self::Neither => "",
        }
    }

    fn defaults_section(self) -> &'static str {
        match self {
            Self::Server | Self::Defaults => {
                "[defaults]\nfile_permissions = \"0o604\"\ndir_permissions = \"0o705\"\n"
            }
            Self::Neither => "",
        }
    }

    fn expected_file_mode(self) -> u32 {
        match self {
            Self::Server => 0o640,
            Self::Defaults => 0o604,
            Self::Neither => 0o664,
        }
    }

    fn expected_dir_mode(self) -> u32 {
        match self {
            Self::Server => 0o750,
            Self::Defaults => 0o705,
            Self::Neither => 0o775,
        }
    }
}

const ALL_SOURCES: [PermissionSource; 3] = [
    PermissionSource::Server,
    PermissionSource::Defaults,
    PermissionSource::Neither,
];

/// ローカルと書き込み先 develop の一時ディレクトリ。develop の実体はローカルのディレクトリに差し替える
struct WriteFixture {
    local: TempDir,
    destination: TempDir,
    config_dir: TempDir,
}

impl WriteFixture {
    fn new() -> Self {
        Self {
            local: TempDir::new().unwrap(),
            destination: TempDir::new().unwrap(),
            config_dir: TempDir::new().unwrap(),
        }
    }

    /// 選ぶ順の全体を通すため、`source` の書き方の設定を読み込み関数で読む
    fn config(&self, source: PermissionSource) -> AppConfig {
        let path = self.config_dir.path().join("config.toml");
        fs::write(
            &path,
            format!(
                "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n{}[backup]\nenabled = false\n{}",
                self.local.path().display().to_string(),
                self.destination.path().display().to_string(),
                source.server_lines(),
                source.defaults_section(),
            ),
        )
        .unwrap();
        load_config_from_paths(Some(&path), None).unwrap()
    }

    fn targets(&self) -> RuntimeTargets {
        RuntimeTargets::production()
            .with_local("develop", self.destination.path())
            .with_startup_directory(self.config_dir.path().to_path_buf())
    }

    fn destination_mode(&self, path: &str) -> u32 {
        fs::metadata(self.destination.path().join(path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }
}

// @kotowari[REQ-config-016]
#[test]
fn merge_creates_a_new_file_with_the_server_then_defaults_then_default_mode() {
    for source in ALL_SOURCES {
        let fixture = WriteFixture::new();
        fs::write(fixture.local.path().join("new.txt"), "new\n").unwrap();
        let args = MergeArgs {
            paths: vec!["new.txt".into()],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            dry_run: false,
            force: false,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
            hunks: None,
        };
        let result = execute_merge(args, fixture.config(source), fixture.targets()).unwrap();
        let MergeCommandOutput::Files(output) = result.output else {
            panic!("expected per-file result")
        };
        assert_eq!(output.merged.len(), 1, "{source:?}: {output:?}");
        assert_eq!(
            fs::read_to_string(fixture.destination.path().join("new.txt")).unwrap(),
            "new\n"
        );
        assert_eq!(
            fixture.destination_mode("new.txt"),
            source.expected_file_mode(),
            "{source:?}"
        );
    }
}

// @kotowari[REQ-config-016]
#[test]
fn sync_creates_a_new_directory_with_the_server_then_defaults_then_default_mode() {
    for source in ALL_SOURCES {
        let fixture = WriteFixture::new();
        fs::create_dir(fixture.local.path().join("nested")).unwrap();
        fs::write(fixture.local.path().join("nested/file.txt"), "new\n").unwrap();
        let args = SyncArgs {
            paths: vec!["nested/file.txt".into()],
            left: Some("local".into()),
            right: vec!["develop".into()],
            dry_run: false,
            force: true,
            delete: false,
            with_permissions: false,
            checksum: false,
            format: "json".into(),
            max_entries: None,
        };
        let result = execute_sync(args, fixture.config(source), fixture.targets()).unwrap();
        let SyncCommandOutput::Result(output) = result.output else {
            panic!("expected sync result")
        };
        assert_eq!(output.targets[0].merged.len(), 1, "{source:?}: {output:?}");
        assert_eq!(
            fs::read_to_string(fixture.destination.path().join("nested/file.txt")).unwrap(),
            "new\n"
        );
        assert_eq!(
            fixture.destination_mode("nested"),
            source.expected_dir_mode(),
            "{source:?}"
        );
    }
}

// ─── 走査の上限（REQ-config-017、TBL-config-003） ─────────────────

// @kotowari[REQ-config-017]
#[test]
fn config_max_scan_entries_accepts_its_range_and_stops_outside_it() {
    for value in [1, 1_000_000] {
        let config = load_ok(&format!("max_scan_entries = {value}\n{}", plain_server("")));
        assert_eq!(config.max_scan_entries, value);
    }
    for value in ["0", "1000001"] {
        assert_eq!(
            load_error(&format!("max_scan_entries = {value}\n{}", plain_server(""))),
            format!(
                "Invalid config value: max_scan_entries - max_scan_entries must be between 1 and 1,000,000, got {value}"
            )
        );
    }
}

/// --max-entries の範囲の外の値と、そのときのエラーの全体の文言（値はカンマなしで出る）
const MAX_ENTRIES_OUTSIDE: [(usize, &str); 2] = [
    (0, "max_scan_entries must be between 1 and 1,000,000, got 0"),
    (
        1_000_001,
        "max_scan_entries must be between 1 and 1,000,000, got 1000001",
    ),
];

// @kotowari[REQ-config-017]
#[test]
fn status_max_entries_outside_its_range_stops() {
    let fixture = status_support::fixture();
    for (value, expected) in MAX_ENTRIES_OUTSIDE {
        let mut args = status_support::args(Some("local"), Some("develop"));
        args.max_entries = Some(value);
        let Err(err) = fixture.run(args) else {
            panic!("expected status to stop")
        };
        assert_eq!(format!("{err:#}"), expected);
    }
}

// @kotowari[REQ-config-017]
#[test]
fn diff_max_entries_outside_its_range_stops() {
    let fixture = status_support::fixture();
    for (value, expected) in MAX_ENTRIES_OUTSIDE {
        let args = DiffArgs {
            paths: vec![],
            left: Some("local".into()),
            right: Some("develop".into()),
            ref_server: None,
            format: "json".into(),
            max_lines: None,
            max_files: 100,
            force: false,
            follow_external_links: false,
            max_entries: Some(value),
        };
        let Err(err) = execute_diff(args, fixture.config.clone(), fixture.targets()) else {
            panic!("expected diff to stop")
        };
        assert_eq!(format!("{err:#}"), expected);
    }
}

// @kotowari[REQ-config-017]
#[test]
fn merge_max_entries_outside_its_range_stops() {
    let fixture = merge_support::fixture();
    fixture.write("local", "file.txt", "local\n");
    for (value, expected) in MAX_ENTRIES_OUTSIDE {
        let mut args = merge_support::args(&["file.txt"]);
        args.max_entries = Some(value);
        assert_eq!(format!("{:#}", fixture.merge_error(args)), expected);
        assert!(!fixture.root("develop").join("file.txt").exists());
    }
}

// @kotowari[REQ-config-017]
#[test]
fn sync_max_entries_outside_its_range_stops() {
    let fixture = sync_support::fixture();
    fixture.write("local", "file.txt", "local\n");
    for (value, expected) in MAX_ENTRIES_OUTSIDE {
        let mut args = sync_support::args(&["file.txt"], &["develop"]);
        args.max_entries = Some(value);
        let Err(err) = fixture.run(args) else {
            panic!("expected sync to stop")
        };
        assert_eq!(format!("{err:#}"), expected);
        assert!(!fixture.exists("develop", "file.txt"));
    }
}

// ─── ホスト鍵の確認の値の読み方（REQ-config-018） ─────────────────

fn host_key_checking(value: &str) -> StrictHostKeyChecking {
    load_ok(&format!(
        "{}[ssh]\nstrict_host_key_checking = {value:?}\n",
        plain_server("")
    ))
    .ssh
    .strict_host_key_checking
}

// @kotowari[REQ-config-018]
#[test]
fn strict_host_key_checking_values_are_read_ignoring_case() {
    for (value, expected) in [
        ("ask", StrictHostKeyChecking::Ask),
        ("ASK", StrictHostKeyChecking::Ask),
        ("yes", StrictHostKeyChecking::Yes),
        ("Yes", StrictHostKeyChecking::Yes),
        ("true", StrictHostKeyChecking::Yes),
        ("TRUE", StrictHostKeyChecking::Yes),
        ("no", StrictHostKeyChecking::No),
        ("NO", StrictHostKeyChecking::No),
        ("false", StrictHostKeyChecking::No),
        ("False", StrictHostKeyChecking::No),
    ] {
        assert_eq!(host_key_checking(value), expected, "{value:?}");
    }
}

// @kotowari[REQ-config-018]
#[test]
fn unknown_strict_host_key_checking_value_is_read_as_ask() {
    for value in ["maybe", "on", ""] {
        assert_eq!(
            host_key_checking(value),
            StrictHostKeyChecking::Ask,
            "{value:?}"
        );
    }
}
