//! sync の契約テストが共有する準備と呼び出し。
//!
//! 書き込み先 "develop"・"staging"・"production" はリモートの設定を持つが、実体は一時ディレクトリに差し替える。
//! 書き込む予定があるときに確認のプロンプトがテストのプロセスの標準入力を読まないよう、既定で force を指定する。
//! サーバ "offline" は一時ディレクトリに差し替えず、閉じたローカルのポートを指すため接続に失敗する。

use std::fs;
use std::path::Path;

use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput, SyncCommandResult};
use remote_merge::config::{load_config_from_paths, AppConfig};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::types::SyncOutput;
use tempfile::TempDir;

const TARGETS: [&str; 3] = ["develop", "staging", "production"];

pub struct Fixture {
    local: TempDir,
    targets: Vec<(&'static str, TempDir)>,
    backup: TempDir,
    config_dir: TempDir,
    config: AppConfig,
}

pub fn fixture() -> Fixture {
    let local = TempDir::new().unwrap();
    let targets: Vec<(&'static str, TempDir)> = TARGETS
        .iter()
        .map(|name| (*name, TempDir::new().unwrap()))
        .collect();
    let config_dir = TempDir::new().unwrap();
    let config_path = config_dir.path().join("config.toml");
    let servers: String = targets
        .iter()
        .map(|(name, root)| {
            format!(
                "[servers.{name}]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n",
                root.path().display().to_string()
            )
        })
        .collect();
    let offline = format!(
        "[servers.offline]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"unused\"\nroot_dir = \"/unused\"\n",
        closed_port()
    );
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n{servers}{offline}[backup]\nenabled = false\n",
            local.path().display().to_string(),
        ),
    )
    .unwrap();
    let config = load_config_from_paths(Some(&config_path), None).unwrap();
    Fixture {
        local,
        targets,
        backup: TempDir::new().unwrap(),
        config_dir,
        config,
    }
}

/// 待ち受けを閉じたローカルのポート。接続は拒否される
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

impl Fixture {
    /// "local" か書き込み先の名前のディレクトリ
    pub fn root(&self, side: &str) -> &Path {
        if side == "local" {
            return self.local.path();
        }
        self.targets
            .iter()
            .find(|(name, _)| *name == side)
            .map(|(_, root)| root.path())
            .unwrap_or_else(|| panic!("unknown side {side}"))
    }

    pub fn write(&self, side: &str, path: &str, content: &str) {
        fs::write(self.root(side).join(path), content).unwrap();
    }

    pub fn read(&self, side: &str, path: &str) -> String {
        fs::read_to_string(self.root(side).join(path)).unwrap()
    }

    pub fn exists(&self, side: &str, path: &str) -> bool {
        self.root(side).join(path).exists()
    }

    /// 書き込み先のファイルを読めなくする（書き込みはできる）
    #[cfg(unix)]
    pub fn make_unreadable(&self, side: &str, path: &str) {
        use std::os::unix::fs::PermissionsExt;

        let full = self.root(side).join(path);
        fs::set_permissions(&full, fs::Permissions::from_mode(0o200)).unwrap();
        assert!(
            fs::File::open(&full).is_err(),
            "test needs an unreadable file: {}",
            full.display()
        );
    }

    fn runtime_targets(&self) -> RuntimeTargets {
        self.targets
            .iter()
            .fold(RuntimeTargets::production(), |targets, (name, root)| {
                targets.with_local(*name, root.path())
            })
            .with_backup_store(Some(self.backup.path().to_path_buf()))
            .with_startup_directory(self.config_dir.path().to_path_buf())
    }

    pub fn run(&self, args: SyncArgs) -> anyhow::Result<SyncCommandResult> {
        execute_sync(args, self.config.clone(), self.runtime_targets())
    }

    /// 結果と終了コードを返す。取り消しやエラーで結果がないときは失敗する。
    pub fn sync(&self, args: SyncArgs) -> (SyncOutput, i32) {
        let result = self.run(args).unwrap();
        let SyncCommandOutput::Result(output) = result.output else {
            panic!("expected a sync result")
        };
        (output, result.exit_code)
    }
}

/// local から `rights` へ `paths` を force で書き込む引数
pub fn args(paths: &[&str], rights: &[&str]) -> SyncArgs {
    SyncArgs {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        left: Some("local".into()),
        right: rights.iter().map(|right| right.to_string()).collect(),
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

pub fn labels(output: &SyncOutput) -> Vec<&str> {
    output
        .targets
        .iter()
        .map(|target| target.target.label.as_str())
        .collect()
}
