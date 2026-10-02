//! exclude・include・sensitive のパターンがどのパスに当たるか（docs/ir/config/filters.md）の契約テスト。
//!
//! 設定を `load_config_from_paths` で読み（合成と include の整え方を通す）、サーバ develop を
//! ローカルのディレクトリに差し替えて status を全件表示で実行し、JSON の "files" の "path" と
//! "sensitive" で観測する。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use remote_merge::cli::status::{execute_status, StatusArgs};
use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::format_json;
use tempfile::TempDir;

/// 左（local）と右（develop）に同じ名前で中身の違うファイルを置いた構成
struct Workspace {
    local: TempDir,
    develop: TempDir,
    config_dir: TempDir,
}

impl Workspace {
    fn new(files: &[&str]) -> Self {
        let workspace = Self {
            local: TempDir::new().unwrap(),
            develop: TempDir::new().unwrap(),
            config_dir: TempDir::new().unwrap(),
        };
        for name in files {
            for (root, content) in [
                (&workspace.local, "left\n"),
                (&workspace.develop, "right\n"),
            ] {
                let path = root.path().join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
        }
        workspace
    }

    fn local_root(&self) -> &Path {
        self.local.path()
    }

    /// どちらの設定にも書く [local]（[local] がないときの挙動は FLAG-config-002 の範囲のため避ける）
    fn local_section(&self) -> String {
        format!(
            "[local]\nroot_dir = {:?}\n",
            self.local.path().display().to_string()
        )
    }

    /// グローバル設定に `global_filter` を、プロジェクト設定があれば `project_filter` を
    /// [filter] の中身として書いて読み、status を全件表示で実行した JSON の "files" を返す
    fn files(&self, global_filter: &str, project_filter: Option<&str>) -> Vec<serde_json::Value> {
        let global_path = self.config_dir.path().join("global.toml");
        fs::write(
            &global_path,
            format!(
                "{}[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = false\n[filter]\n{global_filter}\n",
                self.local_section(),
                self.develop.path().display().to_string(),
            ),
        )
        .unwrap();
        let project_path = self.config_dir.path().join("project.toml");
        if let Some(filter) = project_filter {
            fs::write(
                &project_path,
                format!("{}[filter]\n{filter}\n", self.local_section()),
            )
            .unwrap();
        }
        let config = load_config_from_paths(
            Some(&global_path),
            project_filter.map(|_| project_path.as_path()),
        )
        .unwrap();
        let targets = RuntimeTargets::production()
            .with_local("develop", self.develop.path())
            .with_startup_directory(self.config_dir.path().to_path_buf());
        let result = execute_status(status_args(), config, targets).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&format_json(&result.output).unwrap()).unwrap();
        json["files"]
            .as_array()
            .unwrap_or_else(|| panic!("files missing: {json}"))
            .clone()
    }

    /// 一覧に出たパスの集合
    fn listed(&self, filter: &str) -> BTreeSet<String> {
        self.files(filter, None)
            .iter()
            .map(|file| file["path"].as_str().unwrap().to_string())
            .collect()
    }

    /// 一覧に出たパスごとの "sensitive"
    fn sensitivity(
        &self,
        global_filter: &str,
        project_filter: Option<&str>,
    ) -> BTreeMap<String, bool> {
        self.files(global_filter, project_filter)
            .iter()
            .map(|file| {
                (
                    file["path"].as_str().unwrap().to_string(),
                    file["sensitive"].as_bool().unwrap(),
                )
            })
            .collect()
    }
}

fn status_args() -> StatusArgs {
    StatusArgs {
        left: Some("local".into()),
        right: Some("develop".into()),
        ref_server: None,
        format: "json".into(),
        summary: false,
        all: true,
        checksum: false,
        verbose: 0,
        max_entries: None,
    }
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

// ─── "/" を含まない exclude のパターン（REQ-config-021） ─────────────────

// @kotowari[REQ-config-021]
#[test]
fn name_pattern_excludes_files_whose_name_matches_at_any_depth() {
    let workspace = Workspace::new(&[
        "app.log",
        "app.txt",
        "logs/deep/trace.log",
        "logs/deep/readme.txt",
    ]);
    assert_eq!(
        workspace.listed("exclude = [\"*.log\"]"),
        set(&["app.txt", "logs/deep/readme.txt"])
    );
}

// @kotowari[REQ-config-021]
#[test]
fn name_pattern_matching_a_directory_name_excludes_everything_below_it() {
    let workspace = Workspace::new(&[
        "a/node_modules/b/c.txt",
        "node_modules/top.txt",
        "a/keep.txt",
        "a/node_modules_x/c.txt",
    ]);
    assert_eq!(
        workspace.listed("exclude = [\"node_modules\"]"),
        set(&["a/keep.txt", "a/node_modules_x/c.txt"])
    );
}

// ─── "/" を含む exclude のパターン（REQ-config-022） ─────────────────

// @kotowari[REQ-config-022]
#[test]
fn path_pattern_excludes_only_paths_whose_whole_relative_path_matches() {
    let workspace = Workspace::new(&["config/a.toml", "config/a.txt", "other/a.toml", "a.toml"]);
    assert_eq!(
        workspace.listed("exclude = [\"config/*.toml\"]"),
        set(&["config/a.txt", "other/a.toml", "a.toml"])
    );
}

// @kotowari[REQ-config-022]
#[test]
fn path_pattern_ending_in_double_star_excludes_everything_below_the_directory() {
    let workspace = Workspace::new(&[
        "vendor/legacy/a.txt",
        "vendor/legacy/deep/b.txt",
        "vendor/current/a.txt",
        "legacy/a.txt",
    ]);
    assert_eq!(
        workspace.listed("exclude = [\"vendor/legacy/**\"]"),
        set(&["vendor/current/a.txt", "legacy/a.txt"])
    );
}

// ─── include の前方一致（REQ-config-023） ─────────────────

// @kotowari[REQ-config-023]
#[test]
fn include_selects_the_directory_and_below_but_not_a_name_that_only_starts_with_it() {
    let workspace = Workspace::new(&["src/a.txt", "src/deep/b.txt", "srcx/a.txt", "top.txt"]);
    assert_eq!(
        workspace.listed("include = [\"src\"]"),
        set(&["src/a.txt", "src/deep/b.txt"])
    );
}

// @kotowari[REQ-config-023]
#[test]
fn nested_include_selects_only_below_the_nested_directory() {
    let workspace = Workspace::new(&[
        "a/b/c.txt",
        "a/b/d/e.txt",
        "a/other.txt",
        "a/bx/c.txt",
        "top.txt",
    ]);
    assert_eq!(
        workspace.listed("include = [\"a/b\"]"),
        set(&["a/b/c.txt", "a/b/d/e.txt"])
    );
}

// ─── include の書き方の整え方と無効な値（REQ-config-024） ─────────────────

/// "src" の外にもファイルがある構成。include が "src" と同じ対象になれば src の下だけが出る
const SRC_AND_OUTSIDE: &[&str] = &["src/a.txt", "src/deep/b.txt", "top.txt", "other/c.txt"];

// @kotowari[REQ-config-024]
#[test]
fn empty_include_value_is_ignored() {
    // 空の値は整えられなければ root_dir 全体を指すため、"src" の外のファイルが出ないことで無視を見分ける
    let workspace = Workspace::new(SRC_AND_OUTSIDE);
    assert_eq!(
        workspace.listed("include = [\"\", \"src\"]"),
        set(&["src/a.txt", "src/deep/b.txt"])
    );
}

// @kotowari[REQ-config-024]
#[test]
fn absolute_traversal_and_glob_include_values_are_ignored() {
    // どの値も整えられなければ root_dir の中の実在するディレクトリとして走査されるため、
    // その下のファイルが出ないことで無視を見分ける
    let workspace = Workspace::new(&[
        "src/a.txt",
        "absolute/a.txt",
        "traversal/a.txt",
        "lib[1]/a.txt",
        "lib*/a.txt",
        "lib?/a.txt",
        "top.txt",
    ]);
    let absolute = workspace.local_root().join("absolute");
    assert!(absolute.is_dir());
    for glob in ["lib[1]", "lib*", "lib?"] {
        assert!(workspace.local_root().join(glob).is_dir());
    }
    let filter = format!(
        "include = [{:?}, \"src/../traversal\", \"lib[1]\", \"lib*\", \"lib?\", \"src\"]",
        absolute.display().to_string()
    );
    assert_eq!(workspace.listed(&filter), set(&["src/a.txt"]));
}

// ─── include と exclude の併用（REQ-config-025） ─────────────────

// @kotowari[REQ-config-025]
#[test]
fn exclude_removes_matching_paths_from_the_include_target() {
    let workspace = Workspace::new(&[
        "src/a.txt",
        "src/a.log",
        "src/deep/b.log",
        "top.txt",
        "top.log",
    ]);
    assert_eq!(
        workspace.listed("include = [\"src\"]\nexclude = [\"*.log\"]"),
        set(&["src/a.txt"])
    );
}

// ─── 既定の sensitive のパターン（REQ-config-026） ─────────────────

/// 既定の六つのパターンのそれぞれに名前が当たるファイル（サブディレクトリに置いたものを含む）
const DEFAULT_SENSITIVE: &[&str] = &[
    ".env",
    "app/.env",
    ".env.production",
    "certs/server.pem",
    "server.key",
    "credentials.json",
    "config/credentials.yml",
    "my-secret-notes.txt",
];

/// 既定のどのパターンにも名前が当たらないファイル
const NOT_SENSITIVE: &[&str] = &["README.md", "env.txt", "certs/server.crt", "keys.txt"];

/// 既定のパターンにも "*secret*" にも当たらず、設定に書く "*.confidential" にだけ当たるファイル
const CONFIGURED_SENSITIVE: &[&str] = &["token.confidential", "data/token.confidential"];

fn expected_sensitivity(
    sensitive: &[&[&str]],
    not_sensitive: &[&[&str]],
) -> BTreeMap<String, bool> {
    let marked = sensitive
        .iter()
        .flat_map(|paths| paths.iter())
        .map(|path| (path.to_string(), true));
    let unmarked = not_sensitive
        .iter()
        .flat_map(|paths| paths.iter())
        .map(|path| (path.to_string(), false));
    marked.chain(unmarked).collect()
}

fn all_files() -> Vec<&'static str> {
    [DEFAULT_SENSITIVE, NOT_SENSITIVE, CONFIGURED_SENSITIVE].concat()
}

// @kotowari[REQ-config-026]
#[test]
fn default_sensitive_patterns_apply_without_a_sensitive_setting() {
    let workspace = Workspace::new(&all_files());
    assert_eq!(
        workspace.sensitivity("", None),
        expected_sensitivity(&[DEFAULT_SENSITIVE], &[NOT_SENSITIVE, CONFIGURED_SENSITIVE])
    );
}

// @kotowari[REQ-config-026]
#[test]
fn sensitive_pattern_in_the_global_config_is_added_to_the_defaults() {
    let workspace = Workspace::new(&all_files());
    assert_eq!(
        workspace.sensitivity("sensitive = [\"*.confidential\"]", None),
        expected_sensitivity(&[DEFAULT_SENSITIVE, CONFIGURED_SENSITIVE], &[NOT_SENSITIVE])
    );
}

// @kotowari[REQ-config-026]
#[test]
fn sensitive_pattern_in_the_project_config_is_added_to_the_defaults() {
    let workspace = Workspace::new(&all_files());
    assert_eq!(
        workspace.sensitivity("", Some("sensitive = [\"*.confidential\"]")),
        expected_sensitivity(&[DEFAULT_SENSITIVE, CONFIGURED_SENSITIVE], &[NOT_SENSITIVE])
    );
}
