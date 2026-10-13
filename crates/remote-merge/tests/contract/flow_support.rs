#![cfg(unix)]
//! 比べる流れを三つの経路で直接呼ぶ契約テストの道具。
//!
//! 経路ごとに左右の root_dir を作り、左はいつもローカル、右は調べる経路（ローカル・エージェントを
//! 無効にした SSH・エージェント）にする。製品の実行時処理は HOME の known_hosts を読み書きするので、
//! HOME を一時ディレクトリにしたテストのプロセスで動かす（path_operations_agree と同じ形）。

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::side::Side;

use super::server_faults::Faults;
use super::ssh_server::TestServer;

const CONFIG: &str = "REMOTE_MERGE_FLOW_CONFIG";
const ROUTE: &str = "REMOTE_MERGE_FLOW_ROUTE";
const REQUEST: &str = "REMOTE_MERGE_FLOW_REQUEST";
const RESULT: &str = "REMOTE_MERGE_FLOW_RESULT";
const RIGHT: &str = "REMOTE_MERGE_FLOW_RIGHT";

/// 右の側の経路
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    Local,
    Ssh,
    Agent,
}

pub(super) const ROUTES: [Route; 3] = [Route::Local, Route::Ssh, Route::Agent];

impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh => "ssh",
            Self::Agent => "agent",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "local" => Self::Local,
            "ssh" => Self::Ssh,
            "agent" => Self::Agent,
            other => panic!("unknown route {other}"),
        }
    }
}

/// 子のプロセスで受けた求め。右の側は "fixture" のサーバで、経路は `route` で決まる
pub(super) struct Child {
    pub(super) runtime: CoreRuntime,
    pub(super) request: String,
    result: PathBuf,
}

impl Child {
    pub(super) fn right(&self) -> Side {
        Side::Remote("fixture".into())
    }

    /// 答えを親のプロセスに返す
    pub(super) fn answer(self, text: &str) {
        fs::write(&self.result, text).unwrap();
    }
}

/// 子のプロセスなら、設定を読んで右の経路につないだ実行時処理と求めを返す
pub(super) fn child() -> Option<Child> {
    let config = std::env::var(CONFIG).ok()?;
    let route = Route::from_name(&std::env::var(ROUTE).unwrap());
    let right_root = PathBuf::from(std::env::var(RIGHT).unwrap());
    let config =
        remote_merge::config::load_config_from_paths(Some(Path::new(&config)), None).unwrap();
    let mut runtime = match route {
        Route::Local => CoreRuntime::with_targets(
            config,
            RuntimeTargets::production().with_local("fixture", &right_root),
        ),
        Route::Ssh | Route::Agent => CoreRuntime::new(config),
    };
    if route != Route::Local {
        runtime.connect("fixture").unwrap();
    }
    // 調べる経路を通っていることを確かめる
    assert_eq!(runtime.has_agent("fixture"), route == Route::Agent);
    Some(Child {
        runtime,
        request: std::env::var(REQUEST).unwrap(),
        result: PathBuf::from(std::env::var(RESULT).unwrap()),
    })
}

/// 三つの経路の左右の root_dir と、SSH とエージェントの経路の偽サーバ
pub(super) struct FlowRoutes {
    home: tempfile::TempDir,
    server: TestServer,
    /// 偽サーバを動かす非同期の実行環境。テストは子のプロセスで製品の実行時処理（自分の非同期の
    /// 実行環境を持つ）を動かすので、テストの関数を非同期にしない
    _runtime: tokio::runtime::Runtime,
}

impl FlowRoutes {
    /// 経路ごとに左右の root_dir を作り、`state(左, 右)` で同じ状態にする
    pub(super) fn new(state: impl Fn(&Path, &Path)) -> Self {
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let deployed = home.path().join("agent/remote-merge-testuser/remote-merge");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(deployed.parent().unwrap())
            .unwrap();
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_remote-merge"), &deployed).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let server = runtime.block_on(TestServer::filesystem_with_faults(
            home.path(),
            true,
            Faults::default(),
        ));
        let routes = Self {
            home,
            server,
            _runtime: runtime,
        };
        for route in ROUTES {
            let (left, right) = (routes.left(route), routes.right(route));
            fs::create_dir_all(&left).unwrap();
            fs::create_dir_all(&right).unwrap();
            state(&left, &right);
            fs::write(routes.config(route), routes.config_text(route)).unwrap();
        }
        routes
    }

    pub(super) fn left(&self, route: Route) -> PathBuf {
        self.home.path().join(route.name()).join("left")
    }

    pub(super) fn right(&self, route: Route) -> PathBuf {
        self.home.path().join(route.name()).join("right")
    }

    fn config(&self, route: Route) -> PathBuf {
        self.home.path().join(format!("{}.toml", route.name()))
    }

    fn config_text(&self, route: Route) -> String {
        format!(
            "[local]\nroot_dir = {:?}\n[servers.fixture]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n[agent]\nenabled = {}\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
            self.left(route).to_str().unwrap(),
            self.server.port(),
            self.right(route).to_str().unwrap(),
            route == Route::Agent,
            self.home.path().join("agent").to_str().unwrap(),
        )
    }

    /// テスト `test`（初めに [`child`] を見て、子のプロセスなら求めに答えて終わるもの）を子の
    /// プロセスで動かし、`route` の経路で `request` を行った答えを返す
    pub(super) fn run(&self, route: Route, test: &str, request: &str) -> String {
        self.run_within(route, test, request, Duration::from_secs(120), || {})
            .unwrap_or_else(|| panic!("{route:?}: {request} did not finish"))
    }

    /// [`run`](Self::run) を時間の上限 `limit` の内で行う。終わらなければ `release`（待っている
    /// 相手を終わらせるもの）を呼んで子のプロセスを止め、None を返す
    pub(super) fn run_within(
        &self,
        route: Route,
        test: &str,
        request: &str,
        limit: Duration,
        release: impl Fn(),
    ) -> Option<String> {
        let result = self.home.path().join("flow-result");
        let mut process = Command::new(std::env::current_exe().unwrap())
            .env("HOME", self.home.path())
            .env("XDG_CACHE_HOME", self.home.path().join("cache"))
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env("XDG_DATA_HOME", self.home.path().join("data"))
            .env("RUST_LOG", "off")
            .env(CONFIG, self.config(route))
            .env(ROUTE, route.name())
            .env(REQUEST, request)
            .env(RESULT, &result)
            .env(RIGHT, self.right(route))
            .args(["--exact", test, "--nocapture"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + limit;
        while process.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                release();
                process.kill().unwrap();
                process.wait().unwrap();
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = process.wait_with_output().unwrap();
        assert!(output.status.success(), "{route:?}: {output:?}");
        let answer = fs::read_to_string(&result).unwrap();
        fs::remove_file(&result).unwrap();
        Some(answer)
    }
}
