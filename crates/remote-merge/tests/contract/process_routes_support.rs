#![cfg(unix)]
//! ローカルの経路と、エージェントのプロセスを直に起動する経路で、同じ操作を比べる契約テストの道具。
//!
//! 経路ごとに同じ状態の一時ディレクトリを作る。エージェントの経路は SSH を通さず、実行ファイルを
//! `agent --root` で起動して通信の形式で要求する。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use remote_merge::agent::client::AgentClient;
use remote_merge_core::op_failure::{OpFailure, failure_of};
use remote_merge_core::test_tree::PermissionLocks;

pub(super) const MAX_ENTRIES: usize = 10_000;

/// 一つの経路の root_dir と、その外のディレクトリ
pub(super) struct RouteDir {
    pub(super) root: PathBuf,
    pub(super) outside: PathBuf,
}

impl RouteDir {
    fn new(base: &Path, name: &str, state: impl Fn(&Path, &Path)) -> Self {
        let root = base.join(name).join("root");
        let outside = base.join(name).join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        state(&root, &outside);
        Self { root, outside }
    }
}

/// 二つの経路の root_dir
pub(super) struct Routes {
    /// 一時ディレクトリを消す前に権限を戻すため、home より先に落とす
    locks: PermissionLocks,
    home: tempfile::TempDir,
    pub(super) local: RouteDir,
    pub(super) agent: RouteDir,
}

impl Routes {
    pub(super) fn new(state: impl Fn(&Path, &Path)) -> Self {
        let locks = PermissionLocks::new();
        let home = tempfile::TempDir::new().unwrap();
        let local = RouteDir::new(home.path(), "local", &state);
        let agent = RouteDir::new(home.path(), "agent", &state);
        Self {
            locks,
            home,
            local,
            agent,
        }
    }

    /// 二つの経路の root_dir の下の `path` の権限を `mode` にする。fixture を落とすときに元へ戻す
    pub(super) fn lock_on_both(&self, path: &str, mode: u32) {
        for route in [&self.local, &self.agent] {
            self.locks.lock(route.root.join(path), mode);
        }
    }

    /// エージェントのプロセスを起動して `operation` を行う
    pub(super) fn over_agent<T>(&self, operation: impl FnOnce(&mut Agent) -> T) -> T {
        let mut agent = Agent::start(&self.agent.root, self.home.path());
        let result = operation(&mut agent);
        agent.stop();
        result
    }
}

pub(super) struct Agent {
    child: Child,
    pub(super) client: AgentClient<std::process::ChildStdout, std::process::ChildStdin>,
}

impl Agent {
    fn start(root: &Path, cwd: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_remote-merge"))
            .arg("agent")
            .arg("--root")
            .arg(root)
            .current_dir(cwd)
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let client =
            AgentClient::connect(child.stdout.take().unwrap(), child.stdin.take().unwrap())
                .unwrap();
        Self { child, client }
    }

    fn stop(mut self) {
        let _ = self.client.shutdown();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 更新時刻をナノ秒まで決める（symlink はリンク自身）
pub(super) fn touch(path: &Path, stamp: &str) {
    let status = Command::new("touch")
        .args(["-h", "-d", stamp])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// 操作の結果を、成功なら "ok"、失敗なら失敗の種類の文字にする
pub(super) fn kind<T>(result: Result<T, OpFailure>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(failure) => format!("{failure:?}"),
    }
}

/// [`kind`] の、型のない誤りから失敗の種類を取り出すもの
pub(super) fn anyhow_kind<T>(result: anyhow::Result<T>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(error) => match failure_of(&error) {
            Some(failure) => format!("{failure:?}"),
            None => format!("untyped: {error:#}"),
        },
    }
}
