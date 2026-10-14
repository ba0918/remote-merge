#![cfg(unix)]
//! 比べる流れと経路の操作をローカルとエージェントの二つの経路で直接呼ぶ契約テストの道具（親のプロセスの側）。経路ごとの
//! root_dir と設定と偽サーバを用意し、子のプロセス（flow_support の [`child`](super::flow_support::child)）
//! を動かして答えを受ける

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::flow_support::{CONFIG, HOME, REQUEST, RESULT, ROUTE, ROUTES, Roots, Route};
use super::server_faults::Faults;
use super::ssh_server::TestServer;

/// 二つの経路の root_dir と、エージェントの経路の偽サーバ
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
        Self::with_roots(|roots| state(&roots.left, &roots.right))
    }

    /// 経路ごとに左右と参照先の root_dir を作り、`state` で同じ状態にする
    pub(super) fn with_roots(state: impl Fn(&Roots)) -> Self {
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
            let roots = routes.roots(route);
            for root in [&roots.left, &roots.right, &roots.reference] {
                fs::create_dir_all(root).unwrap();
            }
            state(&roots);
            fs::write(routes.config(route), routes.config_text(route)).unwrap();
        }
        routes
    }

    pub(super) fn roots(&self, route: Route) -> Roots {
        Roots::of(self.home.path(), route)
    }

    pub(super) fn right(&self, route: Route) -> PathBuf {
        self.roots(route).right
    }

    /// エージェントが受けた要求の種類（二つの経路で続けて並ぶ）
    pub(super) fn agent_requests(&self) -> Vec<String> {
        self.server.faults().agent_requests()
    }

    fn config(&self, route: Route) -> PathBuf {
        self.home.path().join(format!("{}.toml", route.name()))
    }

    fn config_text(&self, route: Route) -> String {
        let roots = self.roots(route);
        let server = |name: &str, root: &Path| {
            format!(
                "[servers.{name}]\nhost = \"127.0.0.1\"\nport = {}\nuser = \"testuser\"\nauth = \"password\"\npassword = \"fixture-password\"\nroot_dir = {:?}\n",
                self.server.port(),
                root.to_str().unwrap()
            )
        };
        format!(
            "[local]\nroot_dir = {:?}\n{}{}[agent]\nenabled = true\ndeploy_dir = {:?}\n[ssh]\ntimeout_sec = 3\nstrict_host_key_checking = \"no\"\n",
            roots.left.to_str().unwrap(),
            server("fixture", &roots.right),
            server("reference", &roots.reference),
            self.home.path().join("agent").to_str().unwrap(),
        )
    }

    /// テスト `test`（初めに子のプロセスかを見て、そうなら求めに答えて終わるもの）を子のプロセスで
    /// 動かし、`route` の経路で `request` を行った答えを返す
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
            .env(HOME, self.home.path())
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
