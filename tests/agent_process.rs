//! Agent サブプロセス E2E テスト (Level 1)
//!
//! `remote-merge agent --root <path>` を実際のサブプロセスとして起動し、
//! プロセス間通信を含むエンドツーエンドの動作を検証する。
//! インプロセステスト (src/agent/tests.rs) とは異なり、
//! バイナリ起動・stdin/stdout パイプ・プロセス終了などをテストする。

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Result;
use tempfile::TempDir;

use remote_merge::agent::client::AgentClient;

// ---------------------------------------------------------------------------
// AgentProcess ヘルパー
// ---------------------------------------------------------------------------

/// サブプロセスとして起動した Agent を管理する。
struct AgentProcess {
    child: Child,
    client: AgentClient<ChildStdout, ChildStdin>,
}

impl AgentProcess {
    /// Agent サブプロセスを起動し、ハンドシェイクを完了する。
    fn spawn(root: &Path) -> Result<Self> {
        let binary = env!("CARGO_BIN_EXE_remote-merge");
        let mut child = Command::new(binary)
            .arg("agent")
            .arg("--root")
            .arg(root)
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let client = AgentClient::connect(stdout, stdin)?;

        Ok(Self { child, client })
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        // ベストエフォートでグレースフル停止 → 強制終了
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// タイムアウト付き wait ヘルパー
// ---------------------------------------------------------------------------

/// 指定タイムアウト内にプロセスが終了するか確認する。
///
/// タイムアウトを超えた場合は `Err` を返す。
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => {
                if start.elapsed() >= timeout {
                    anyhow::bail!("process did not exit within {:?}", timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// テストケース
// ---------------------------------------------------------------------------

/// ハンドシェイク成功確認: AgentProcess::spawn が成功すること自体が検証。
#[test]
fn agent_process_handshake() {
    let tmp = TempDir::new().unwrap();
    let proc = AgentProcess::spawn(tmp.path()).expect("agent should start and complete handshake");
    // drop で自動終了
    drop(proc);
}

/// --root 引数なしで起動すると非ゼロ終了コードになる。
#[test]
fn agent_missing_root_exits_with_error() {
    let binary = env!("CARGO_BIN_EXE_remote-merge");
    let mut child = Command::new(binary)
        .arg("agent")
        // --root を意図的に省略
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn");

    let status = wait_with_timeout(&mut child, Duration::from_secs(5))
        .expect("process should exit within timeout");

    assert!(
        !status.success(),
        "agent without --root should exit with non-zero code, got: {status}"
    );
}

/// 存在しないパスを --root に渡しても Agent は起動でき、WriteFile はエラーを返す。
///
/// Agent 自体のハンドシェイクは成功するが、ファイル書き込みは root が存在しないため失敗する。
/// これは Agent がルートディレクトリの存在チェックを遅延評価する仕様による。
#[test]
fn agent_nonexistent_root_exits_with_error() {
    let nonexistent = "/nonexistent/path/xyz_does_not_exist_abc";
    let binary = env!("CARGO_BIN_EXE_remote-merge");

    let mut child = Command::new(binary)
        .arg("agent")
        .arg("--root")
        .arg(nonexistent)
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn");

    let stdout = child.stdout.take().unwrap();
    let stdin = child.stdin.take().unwrap();

    // ハンドシェイクは成功する（起動自体は問題ない）
    let mut client = AgentClient::connect(stdout, stdin)
        .expect("handshake should succeed even with nonexistent root");

    // WriteFile を送ると root が存在しないためエラーが返るはず
    let result = client.write_file("test.txt", b"data", false);
    assert!(
        result.is_err(),
        "write_file with nonexistent root should return an error"
    );

    drop(client);
    let _ = wait_with_timeout(&mut child, Duration::from_secs(5));
}

/// stdin を閉じると Agent プロセスが自然終了する。
#[test]
fn agent_stdin_close_terminates() {
    let tmp = TempDir::new().unwrap();
    let binary = env!("CARGO_BIN_EXE_remote-merge");

    let mut child = Command::new(binary)
        .arg("agent")
        .arg("--root")
        .arg(tmp.path())
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn");

    // ハンドシェイク行を読み捨てる
    let mut stdout = child.stdout.take().unwrap();
    {
        let mut reader = BufReader::new(&mut stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("should read handshake");
        assert!(
            line.contains("remote-merge agent"),
            "expected handshake line, got: {line:?}"
        );
    }

    // stdin を閉じる → EOF → Agent ループ終了
    drop(child.stdin.take());

    let status = wait_with_timeout(&mut child, Duration::from_secs(5))
        .expect("agent should exit after stdin EOF");

    // 終了コードは 0 でも非 0 でもよい（実装依存）
    // ただし、ハングせずに終了することが重要
    let _ = status;
}

/// stderr の tracing 出力が stdout に混入しない。
///
/// RUST_LOG=debug で起動してもハンドシェイク直後の stdout が
/// ハンドシェイク行のみで構成されることを確認する。
#[test]
fn agent_stderr_not_in_stdout() {
    let tmp = TempDir::new().unwrap();
    let binary = env!("CARGO_BIN_EXE_remote-merge");

    let mut child = Command::new(binary)
        .arg("agent")
        .arg("--root")
        .arg(tmp.path())
        .env("RUST_LOG", "debug") // デバッグログを有効化
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    // stdout から1行だけ読む（ハンドシェイク行）
    let mut stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(&mut stdout);
    let mut handshake_line = String::new();
    reader
        .read_line(&mut handshake_line)
        .expect("should read handshake line");

    // stdin を閉じてプロセスを終了させる
    drop(child.stdin.take());
    let _ = wait_with_timeout(&mut child, Duration::from_secs(5));

    // ハンドシェイク行にプロトコルプレフィックスが含まれること
    assert!(
        handshake_line.contains("remote-merge agent"),
        "stdout first line should be handshake, got: {handshake_line:?}"
    );
    // tracing の典型的なフォーマット文字列が混入していないこと
    assert!(
        !handshake_line.contains("INFO") && !handshake_line.contains("DEBUG"),
        "stdout should not contain tracing output, got: {handshake_line:?}"
    );
}

/// list_tree がファイルを正しく返す。
#[test]
fn agent_list_tree_roundtrip() {
    let tmp = TempDir::new().unwrap();

    // テストファイルを配置
    std::fs::write(tmp.path().join("hello.txt"), "world").unwrap();
    std::fs::create_dir(tmp.path().join("sub")).unwrap();
    std::fs::write(tmp.path().join("sub").join("inner.txt"), "data").unwrap();

    let mut proc = AgentProcess::spawn(tmp.path()).expect("agent should start");

    let (entries, _truncated) = proc
        .client
        .list_tree("", &[], 10_000)
        .expect("list_tree should succeed");

    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"hello.txt"),
        "expected hello.txt in entries, got: {paths:?}"
    );
    assert!(
        paths.contains(&"sub"),
        "expected sub/ in entries, got: {paths:?}"
    );
    assert!(
        paths.contains(&"sub/inner.txt"),
        "expected sub/inner.txt in entries, got: {paths:?}"
    );
}

/// Shutdown リクエストを送るとプロセスがコード 0 で終了する。
#[test]
fn agent_shutdown_clean_exit() {
    let tmp = TempDir::new().unwrap();
    let mut proc = AgentProcess::spawn(tmp.path()).expect("agent should start");

    // Shutdown 送信（レスポンスなし）
    proc.client
        .shutdown()
        .expect("shutdown send should succeed");

    // プロセス終了を待つ（AgentProcess の Drop より先に wait する）
    let status = wait_with_timeout(&mut proc.child, Duration::from_secs(5))
        .expect("agent should exit after Shutdown");

    assert!(
        status.success(),
        "agent should exit with code 0 after Shutdown, got: {status}"
    );
}

/// プロセスを kill した後に wait() が返る（ゾンビプロセスにならない）。
#[test]
fn agent_kill_no_zombie() {
    let tmp = TempDir::new().unwrap();
    let mut proc = AgentProcess::spawn(tmp.path()).expect("agent should start");

    // 強制終了
    proc.child.kill().expect("kill should succeed");

    // wait() が返ることを確認（ゾンビにならない）
    let status = wait_with_timeout(&mut proc.child, Duration::from_secs(5))
        .expect("wait should return after kill");

    // SIGKILL 後は success() = false
    assert!(
        !status.success(),
        "killed process should not report success"
    );
}
