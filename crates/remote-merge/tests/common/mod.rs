//! 共通テストヘルパーモジュール
//!
//! CLI E2E テストで共有するヘルパー関数・構造体を集約する。

#![allow(
    dead_code,
    reason = "テストの共通部品は、使うテストバイナリと使わないものがある"
)]

use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

#[path = "../contract/ssh_server.rs"]
pub(crate) mod ssh_server;

#[path = "../contract/server_faults.rs"]
pub(crate) mod server_faults;

#[path = "../contract/server_removal.rs"]
pub(crate) mod server_removal;

pub(crate) mod login_shell;

// ─── ファイル配置ヘルパー ────────────────────────────────

/// テキストファイルを配置する。親ディレクトリがなければ再帰的に作成する。
pub fn place_files(dir: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
}

/// バイナリファイルを配置する（NUL バイト等を含むファイル用）。
pub fn place_binary_file(dir: &Path, path: &str, content: &[u8]) {
    let full = dir.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
}

/// シンボリックリンクを作成する。
///
/// `link_path` は `dir` からの相対パス。
/// `target_path` はリンク先のパス（相対でも絶対でも可）。
pub fn place_symlink(dir: &Path, link_path: &str, target_path: &str) {
    let full_link = dir.join(link_path);
    if let Some(parent) = full_link.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    unix_fs::symlink(target_path, &full_link).unwrap();
}

// ─── ANSI エスケープ除去 ─────────────────────────────────

/// ANSI エスケープシーケンスを除去してプレーンテキストにする
pub fn strip_ansi(input: &[u8]) -> String {
    let s = String::from_utf8_lossy(input);
    regex_lite_strip(&s)
}

/// 簡易 ANSI ストリッパー（正規表現クレート不要版）
///
/// CSI シーケンス (ESC [ ... letter) と OSC シーケンス (ESC ] ... BEL) を除去する。
fn regex_lite_strip(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ ... (letter) をスキップ
            if chars.peek() == Some(&'[') {
                chars.next(); // '['
                // パラメータバイトと中間バイトを読み飛ばし
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() || next == '~' {
                        chars.next(); // 終端文字
                        break;
                    }
                    chars.next();
                }
            }
            // ESC ] (OSC) もスキップ
            else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if next == '\x07' || next == '\\' {
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ─── Config 生成 ─────────────────────────────────────────

/// テスト用の config TOML 文字列を生成する。
///
/// テスト自身が起動した SSH サーバーへの接続設定を返す。
/// `staging_dir` が `Some` なら 3way 構成（develop + staging）の config を生成する。
pub fn gen_config(
    local_dir: &Path,
    develop_dir: &Path,
    staging_dir: Option<&Path>,
    port: u16,
) -> String {
    assert_ne!(port, 22, "refusing to use the system SSH port");
    assert_ne!(port, 0, "SSH fixture has no listening port");

    let mut config = format!(
        r#"[local]
root_dir = "{local}"

[servers.develop]
        host = "127.0.0.1"
        port = {port}
        user = "fixture-user"
        auth = "password"
        password = "fixture-password"
root_dir = "{develop}"
"#,
        local = local_dir.display(),
        develop = develop_dir.display(),
        port = port,
    );

    if let Some(staging) = staging_dir {
        config.push_str(&format!(
            r#"
[servers.staging]
        host = "127.0.0.1"
        port = {port}
        user = "fixture-user"
        auth = "password"
        password = "fixture-password"
root_dir = "{staging}"
"#,
            port = port,
            staging = staging.display(),
        ));
    }

    config.push_str(&format!(
        r#"
[agent]
enabled = true
deploy_dir = "{agent}"

[filter]
{DEFAULT_FILTER}

[ssh]
timeout_sec = 10
strict_host_key_checking = "no"
"#,
        agent = agent_dir(develop_dir).display(),
    ));

    config
}

/// [`gen_config`] の設定の [filter] の中身
pub const DEFAULT_FILTER: &str = "exclude = [\".git\", \"target\"]";

/// [`gen_config`] の設定の [filter] の中身を `filter` に置き換えた設定
pub fn with_filter(config: &str, filter: &str) -> String {
    let replaced = config.replace(DEFAULT_FILTER, filter);
    assert_ne!(replaced, config, "fixture filter line missing");
    replaced
}

/// [`deploy_agent_for`] で `base` の下に置いたエージェントを使う設定の [agent] の節
pub fn agent_section(base: &Path) -> String {
    format!(
        "[agent]\nenabled = true\ndeploy_dir = {:?}\n",
        base.join("agent").to_str().unwrap()
    )
}

/// エージェントの配置先。develop の root_dir を持つ試験の一時ディレクトリ（`deploy_agent` で置いたもの）
fn agent_dir(develop_dir: &Path) -> PathBuf {
    develop_dir
        .ancestors()
        .map(|base| base.join("agent"))
        .find(|agent| {
            agent
                .join("remote-merge-fixture-user/remote-merge")
                .exists()
        })
        .unwrap_or_else(|| panic!("no deployed agent above {}", develop_dir.display()))
}

/// 配置先に、試験のつなぐ利用者（"fixture-user"）のエージェントとしてこのテストの実行ファイルを置く
pub fn deploy_agent(base: &Path) {
    deploy_agent_for(base, "fixture-user");
}

/// `base` の下の配置先 "agent" に、つなぐ利用者 `user` のエージェントとしてこのテストの実行ファイルを
/// 置く。配置先の祖先はグループやその他が書き込めるとエージェントが実行されないため、`base` の権限を
/// umask によらず決める
pub fn deploy_agent_for(base: &Path, user: &str) {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::set_permissions(base, fs::Permissions::from_mode(0o700)).unwrap();
    let deployed = base.join(format!("agent/remote-merge-{user}/remote-merge"));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(deployed.parent().unwrap())
        .unwrap();
    unix_fs::symlink(env!("CARGO_BIN_EXE_remote-merge"), &deployed).unwrap();
}

// ─── CliEnv（試験 SSH サーバと一時ディレクトリ） ─────────────────────

/// エージェントを置いた試験 SSH サーバと、local と書き込み先の root_dir を持つ一時ディレクトリ。
/// 設定ファイル `config_path` はこのサーバだけにつなぎ、CLI のサブコマンドをその設定で起動する
pub struct CliEnv {
    server: ssh_server::TestServer,
    _runtime: tokio::runtime::Runtime,
    pub temp: TempDir,
    pub config_path: String,
    pub local_dir: PathBuf,
    /// develop の root_dir
    pub remote_dir: PathBuf,
}

/// 設定のサーバの一つが、このテスト自身の試験サーバ（ポート `port`）を指すことを確かめる
pub fn assert_owned_server(server: &toml::Value, port: u16) {
    assert_eq!(server["host"].as_str(), Some("127.0.0.1"));
    let configured = server["port"].as_integer().expect("fixture port missing");
    assert!(
        configured > 0 && configured != 22,
        "refusing to connect to an unowned SSH port"
    );
    assert_eq!(configured, i64::from(port), "fixture port is not owned");
}

impl CliEnv {
    /// 2サーバー構成: local <-> develop(remote)
    pub fn new(local_files: &[(&str, &str)], remote_files: &[(&str, &str)]) -> Self {
        Self::start(local_files, ("remote", remote_files), None)
    }

    /// 3サーバー構成: local(ref) + develop(left) + staging(right)
    ///
    /// `remote_dir` は develop の root_dir を指す。
    pub fn new_3way(
        local_files: &[(&str, &str)],
        develop_files: &[(&str, &str)],
        staging_files: &[(&str, &str)],
    ) -> Self {
        Self::start(local_files, ("develop", develop_files), Some(staging_files))
    }

    /// 一時ディレクトリの下に local、`develop`（ディレクトリの名前と置くファイル）、`staging_files` が
    /// あれば staging の root_dir を作ってファイルを置き、エージェントを置いた試験サーバを起動して
    /// 設定を書く
    fn start(
        local_files: &[(&str, &str)],
        develop: (&str, &[(&str, &str)]),
        staging_files: Option<&[(&str, &str)]>,
    ) -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let base = temp.path();
        let local_dir = base.join("local");
        let develop_dir = base.join(develop.0);
        let staging_dir = staging_files.map(|_| base.join("staging"));
        let roots = [(&local_dir, local_files), (&develop_dir, develop.1)]
            .into_iter()
            .chain(staging_dir.iter().zip(staging_files));
        for (dir, files) in roots {
            fs::create_dir_all(dir).unwrap();
            place_files(dir, files);
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        deploy_agent(base);
        let server = runtime.block_on(ssh_server::TestServer::filesystem_with_agent(base));
        let config_path = base.join("test-config.toml");
        let config = gen_config(
            &local_dir,
            &develop_dir,
            staging_dir.as_deref(),
            server.port(),
        );
        fs::write(&config_path, &config).unwrap();

        Self {
            temp,
            config_path: config_path.to_string_lossy().to_string(),
            local_dir,
            remote_dir: develop_dir,
            server,
            _runtime: runtime,
        }
    }

    fn assert_isolated_config(&self) {
        self.assert_isolated_config_at(Path::new(&self.config_path), &self.local_dir);
    }

    /// 自分の設定ファイルと同じ隔離の確認を、テストが別に書いた設定ファイルにかける。
    ///
    /// `local_root` はその設定の [local] の root_dir として書いた値。
    pub fn assert_isolated_config_at(&self, config_path: &Path, local_root: &Path) {
        let config: toml::Table = fs::read_to_string(config_path)
            .expect("fixture configuration missing")
            .parse()
            .expect("fixture configuration invalid");
        assert_eq!(config["local"]["root_dir"].as_str(), local_root.to_str());
        assert_eq!(config["agent"]["enabled"].as_bool(), Some(true));
        let deploy_dir = Path::new(
            config["agent"]["deploy_dir"]
                .as_str()
                .expect("fixture deploy_dir missing"),
        );
        assert!(
            deploy_dir.starts_with(self.temp.path()),
            "deploy_dir escapes fixture"
        );
        assert_eq!(
            config["ssh"]["strict_host_key_checking"].as_str(),
            Some("no")
        );
        let servers = config["servers"]
            .as_table()
            .expect("fixture servers missing");
        assert!(!servers.is_empty());
        for server in servers.values() {
            assert_owned_server(server, self.server.port());
            assert_eq!(server["auth"].as_str(), Some("password"));
            assert_eq!(server["user"].as_str(), Some("fixture-user"));
            assert_eq!(server["password"].as_str(), Some("fixture-password"));
            assert!(server.get("key").is_none(), "refusing a HOME key path");
            let root = Path::new(server["root_dir"].as_str().expect("fixture root missing"));
            assert!(
                root.starts_with(self.temp.path()),
                "remote root escapes fixture"
            );
        }
    }

    /// 設定の値のテストのための、`assert_isolated_config` とは別の隔離の確認。
    ///
    /// 知らない strict_host_key_checking・正しくない password・auth = "key" を書いた設定は
    /// `assert_isolated_config` を通らないため、接続先がこのテスト自身の試験サーバであることと、
    /// 読むファイルが一時ディレクトリの下にあることだけを確かめる。password の値は問わない。
    /// `home` は実行ファイルに渡す HOME で、auth が "key" のときの鍵のパスはこれで解決する。
    /// strict_host_key_checking は、`require_no_host_key_checking` が true のとき "no" を求める。
    pub fn assert_isolated_values_config(
        &self,
        config_path: &Path,
        home: &Path,
        require_no_host_key_checking: bool,
    ) {
        let temp = self.temp.path();
        let inside_temp = |path: &Path| {
            path.is_absolute()
                && !path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
                && path.starts_with(temp)
        };
        assert!(inside_temp(home), "HOME escapes fixture");
        let config: toml::Table = fs::read_to_string(config_path)
            .expect("fixture configuration missing")
            .parse()
            .expect("fixture configuration invalid");
        let local = Path::new(
            config["local"]["root_dir"]
                .as_str()
                .expect("fixture local root missing"),
        );
        assert!(inside_temp(local), "local root escapes fixture");
        if let Some(deploy_dir) = config["agent"].get("deploy_dir") {
            let deploy_dir = Path::new(deploy_dir.as_str().expect("deploy_dir is a string"));
            assert!(inside_temp(deploy_dir), "deploy_dir escapes fixture");
        }
        if require_no_host_key_checking {
            assert_eq!(
                config["ssh"]["strict_host_key_checking"].as_str(),
                Some("no")
            );
        }
        let servers = config["servers"]
            .as_table()
            .expect("fixture servers missing");
        assert!(!servers.is_empty());
        for server in servers.values() {
            assert_owned_server(server, self.server.port());
            assert_eq!(server["user"].as_str(), Some("fixture-user"));
            let root = Path::new(server["root_dir"].as_str().expect("fixture root missing"));
            assert!(inside_temp(root), "remote root escapes fixture");
            match server["auth"].as_str() {
                Some("password") => {}
                Some("key") => {
                    let key = match server.get("key").map(|key| key.as_str().unwrap()) {
                        None => home.join(".ssh/id_rsa"),
                        Some(key) => match key.strip_prefix("~/") {
                            Some(rest) => home.join(rest),
                            None => PathBuf::from(key),
                        },
                    };
                    assert!(inside_temp(&key), "key path escapes fixture");
                }
                other => panic!("unexpected fixture auth {other:?}"),
            }
        }
    }

    /// このテスト自身が起動した SSH の試験サーバのポート
    pub fn server_port(&self) -> u16 {
        self.server.port()
    }

    /// 設定の [filter] の中身を `filter` に置き換える
    pub fn replace_filter(&self, filter: &str) {
        let config = fs::read_to_string(&self.config_path).unwrap();
        fs::write(&self.config_path, with_filter(&config, filter)).unwrap();
    }

    /// CLI コマンドを生成（--config 付き）
    ///
    /// [`isolated_command`] の環境で、XDG の場所は一時ディレクトリの直下の "xdg-config"・
    /// "xdg-data"・"xdg-cache" にする（バックアップと診断ログをそこから読むテストがある）。
    pub fn cmd(&self) -> Command {
        self.assert_isolated_config();
        let temp = self.temp.path();
        let mut cmd = isolated_command(&temp.join("home"));
        cmd.env("XDG_CONFIG_HOME", temp.join("xdg-config"));
        cmd.env("XDG_DATA_HOME", temp.join("xdg-data"));
        cmd.env("XDG_CACHE_HOME", temp.join("xdg-cache"));
        cmd.arg("--config").arg(&self.config_path);
        cmd
    }

    pub fn temp_root(&self) -> &Path {
        self.temp.path()
    }

    /// 試験 SSH サーバへのログインを `accepted` 回だけ受け入れ、以降の接続を認証で失敗させる
    pub fn reject_logins_after(&self, accepted: usize) {
        self.server.reject_logins_after(accepted);
    }

    /// 試験 SSH サーバが受け取ったコマンドを、リモートのログインシェル `shell` の `-c` で実行させる
    pub fn use_remote_login_shell(&self, shell: &'static str) {
        self.server.use_login_shell(shell);
    }

    /// CLI コマンドをサブコマンド付きで生成
    pub fn cmd_with(&self, subcommand: &str) -> Command {
        let mut cmd = self.cmd();
        cmd.arg(subcommand);
        cmd
    }
}

// ─── CLI コマンド生成（config なし） ─────────────────────

/// 開発者の環境から隔離して remote-merge の実行ファイルを起動する Command。
///
/// 環境変数を全て消し（RUST_LOG・REMOTE_MERGE_AGENT_BINARY・パスワードの変数などを引き継がない）、
/// HOME を `home`、XDG_CONFIG_HOME・XDG_DATA_HOME・XDG_CACHE_HOME を `home` の下の "config"・
/// "data"・"cache" にする。作業ディレクトリは `home`（なければ作る）、標準入力は空。引き継ぐのは
/// PATH と、カバレッジの計測中の計測データの書き出し先（LLVM_PROFILE_FILE。消すと子の計測が
/// 作業ディレクトリに書かれて数えられない）だけ。場所の違うテストは、返した Command の `env`・
/// `env_remove`・`current_dir`・`stdin` で上書きする
pub fn isolated_command(home: &Path) -> Command {
    fs::create_dir_all(home).expect("Failed to create an isolated home");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_remote-merge"));
    cmd.env_clear();
    for inherited in ["PATH", "LLVM_PROFILE_FILE"] {
        if let Some(value) = std::env::var_os(inherited) {
            cmd.env(inherited, value);
        }
    }
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join("config"));
    cmd.env("XDG_DATA_HOME", home.join("data"));
    cmd.env("XDG_CACHE_HOME", home.join("cache"));
    cmd.current_dir(home);
    cmd.stdin(Stdio::null());
    cmd
}

/// 一時ディレクトリの XDG_DATA_HOME と XDG_CACHE_HOME を持つ Command。一時ディレクトリは Command と
/// 一緒に消え、消せなければ知らせる
pub struct IsolatedCommand {
    command: Command,
    _homes: remote_merge_core::test_tree::TestTree,
}

impl std::ops::Deref for IsolatedCommand {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.command
    }
}

impl std::ops::DerefMut for IsolatedCommand {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.command
    }
}

/// remote-merge バイナリの Command を生成する（config 指定なし）。
///
/// [`isolated_command`] の環境を、Command と一緒に消える一時ディレクトリに作る。XDG の場所は
/// 先に作っておく。exit code テスト等、config 不要なケースで使う。起動したプロセスが終わるまで、
/// 返したものを持っておく
pub fn remote_merge_cmd() -> IsolatedCommand {
    let homes = remote_merge_core::test_tree::TestTree::new();
    let cmd = isolated_command(homes.path());
    for dir in ["config", "data", "cache"] {
        fs::create_dir(homes.path().join(dir)).expect("Failed to create an isolated home");
    }
    IsolatedCommand {
        command: cmd,
        _homes: homes,
    }
}

// ─── 権限で止める前提 ────────────────────────────────────

/// `path` の権限を `mode` にし、この利用者がその権限で止められるかを返す。`mode` に持ち主の読む
/// 権限がなければ読めないこと、読めて書く権限がなければ書けないことを確かめる。root で動くと権限の
/// ビットでは止められないので、理由を標準エラーに示して false を返す。false のとき、呼び出し側は
/// 前提を作れないテストを、関係のない表明で落とさずに確かめずに終える
pub fn restrict_permissions(path: &Path, mode: u32) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("failed to change the mode");
    let is_dir = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir());
    let restricted = match (mode & 0o400 == 0, mode & 0o200 == 0, is_dir) {
        (true, _, true) => fs::read_dir(path).is_err(),
        (true, _, false) => fs::File::open(path).is_err(),
        (false, true, true) => tempfile::tempfile_in(path).is_err(),
        (false, true, false) => fs::OpenOptions::new().write(true).open(path).is_err(),
        (false, false, _) => panic!("mode {mode:o} takes neither reading nor writing away"),
    };
    if !restricted {
        eprintln!(
            "skipped: mode {mode:o} does not restrict {} (running as root)",
            path.display()
        );
    }
    restricted
}

/// 権限のビットがこの利用者を止めるか。止めなければ理由を標準エラーに示して false を返す。
/// 繰り返しの途中で前提を作れずに残りを飛ばさないよう、繰り返す前に一度確かめるときに使う
pub fn permissions_apply() -> bool {
    let probe = TempDir::new().expect("failed to create a probe directory");
    let file = probe.path().join("probe");
    fs::write(&file, "").expect("failed to create a probe file");
    restrict_permissions(&file, 0o000)
}

// ─── 接続を拒否するポート ────────────────────────────────

/// 持っている間、手元で何も待ち受けないと決まっているポート。そこへの接続は拒否される。
///
/// 待ち受けを開いて閉じただけの番号は、並行して走る他のテストが待ち受けに使いうる。つないだままの
/// 接続の手元の番号も使えない。その番号は別の宛先への接続の手元の番号として使い回されうるので、
/// その番号自身へつなぐと、まれに自分自身とつながって拒否されない。ここでは bind だけして listen
/// しないソケットを持つ。持っている間は誰もこの番号に bind できず、接続の手元の番号にも選ばれない
pub struct ClosedPort {
    port: u16,
    _socket: tokio::net::TcpSocket,
}

impl ClosedPort {
    pub fn new() -> Self {
        let socket = tokio::net::TcpSocket::new_v4().expect("failed to create a socket");
        socket
            .bind("127.0.0.1:0".parse().unwrap())
            .expect("failed to bind a port");
        let port = socket.local_addr().unwrap().port();
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
            "port {port} can still be bound"
        );
        let refused = std::net::TcpStream::connect(("127.0.0.1", port)).map(|_| ());
        assert_eq!(
            refused.map_err(|error| error.kind()),
            Err(std::io::ErrorKind::ConnectionRefused),
            "port {port} does not refuse connections"
        );
        Self {
            port,
            _socket: socket,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

// ─── アサーションヘルパー ────────────────────────────────

/// exit code が 0（成功）であることをアサートする。
///
/// 失敗時は stderr の内容を表示する。
pub fn assert_exit_success(output: &Output) {
    assert!(
        output.status.success(),
        "Expected exit code 0, got {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// 指定した exit code であることをアサートする。
///
/// 失敗時は stderr の内容を表示する。
pub fn assert_exit_error(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "Expected exit code {}, got {:?}. stderr: {}",
        code,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// stderr に指定した文字列が含まれることをアサートする。
pub fn assert_stderr_contains(output: &Output, text: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(text),
        "Expected stderr to contain '{}', got: {}",
        text,
        stderr,
    );
}

/// stdout に指定した文字列が含まれることをアサートする。
pub fn assert_stdout_contains(output: &Output, text: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(text),
        "Expected stdout to contain '{}', got: {}",
        text,
        stdout,
    );
}
