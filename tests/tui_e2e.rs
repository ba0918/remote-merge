//! TUI E2E テスト（PTY ベース）
//!
//! 実際にバイナリを PTY 上で起動し、キー入力を送って画面状態を検証する。
//! バグの再現テストとリグレッション防止が目的。
//!
//! テストごとに一時ディレクトリを作成し、localhost SSH 経由で接続する。
//! 前提: localhost に SSH 接続可能であること（公開鍵認証）。

use expectrl::process::unix::UnixProcess;
use expectrl::stream::log::LogStream;
use expectrl::Expect;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// E2E テスト用の Session 型エイリアス
type TuiSession =
    expectrl::Session<UnixProcess, LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>>;

// ─── ヘルパー ───────────────────────────────────────────

/// E2E テスト用の環境を構築する。
/// local/remote/staging ディレクトリにファイルを配置し、テスト用 config を生成する。
struct E2eEnv {
    _temp: TempDir,
    config_path: String,
}

/// ファイル配置ヘルパー
fn place_files(dir: &std::path::Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
}

impl E2eEnv {
    /// 2サーバー構成: local <-> develop(remote)
    fn new(local_files: &[(&str, &str)], remote_files: &[(&str, &str)]) -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let base = temp.path();

        let local_dir = base.join("local");
        let remote_dir = base.join("remote");
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();

        place_files(&local_dir, local_files);
        place_files(&remote_dir, remote_files);

        let config_path = base.join("test-config.toml");
        let config_content = Self::gen_config(&local_dir, &remote_dir, None);
        fs::write(&config_path, &config_content).unwrap();

        Self {
            _temp: temp,
            config_path: config_path.to_string_lossy().to_string(),
        }
    }

    /// 3サーバー構成: develop(left) <-> staging(right) + local(ref)
    ///
    /// スクショの再現: `develop <-> staging` で local が reference。
    fn new_3way(
        local_files: &[(&str, &str)],
        develop_files: &[(&str, &str)],
        staging_files: &[(&str, &str)],
    ) -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let base = temp.path();

        let local_dir = base.join("local");
        let develop_dir = base.join("develop");
        let staging_dir = base.join("staging");
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&develop_dir).unwrap();
        fs::create_dir_all(&staging_dir).unwrap();

        place_files(&local_dir, local_files);
        place_files(&develop_dir, develop_files);
        place_files(&staging_dir, staging_files);

        let config_path = base.join("test-config.toml");
        let config_content = Self::gen_config(&local_dir, &develop_dir, Some(&staging_dir));
        fs::write(&config_path, &config_content).unwrap();

        Self {
            _temp: temp,
            config_path: config_path.to_string_lossy().to_string(),
        }
    }

    fn gen_config(
        local_dir: &std::path::Path,
        develop_dir: &std::path::Path,
        staging_dir: Option<&std::path::Path>,
    ) -> String {
        let home = std::env::var("HOME").expect("HOME not set");
        let key_path = format!("{}/.ssh/id_ed25519", home);
        let user = std::env::var("USER").expect("USER not set");

        let mut config = format!(
            r#"[local]
root_dir = "{local}"

[servers.develop]
host = "localhost"
port = 22
user = "{user}"
auth = "key"
key = "{key}"
root_dir = "{develop}"
"#,
            local = local_dir.display(),
            develop = develop_dir.display(),
            user = user,
            key = key_path,
        );

        if let Some(staging) = staging_dir {
            config.push_str(&format!(
                r#"
[servers.staging]
host = "localhost"
port = 22
user = "{user}"
auth = "key"
key = "{key}"
root_dir = "{staging}"
"#,
                user = user,
                key = key_path,
                staging = staging.display(),
            ));
        }

        config.push_str(
            r#"
[filter]
exclude = [".git", "target"]

[ssh]
timeout_sec = 10
"#,
        );

        config
    }

    /// TUI を起動して expectrl Session を返す。
    /// 追加の CLI 引数を指定可能。
    /// PTY サイズを 200x50 にリサイズして 3way バッジが描画されるようにする。
    fn spawn_tui_with_args(&self, extra_args: &[&str]) -> TuiSession {
        let binary = env!("CARGO_BIN_EXE_remote-merge");
        let mut cmd = Command::new(binary);
        cmd.arg("--config").arg(&self.config_path);
        cmd.arg("--log-level").arg("debug");

        for arg in extra_args {
            cmd.arg(arg);
        }

        let mut session = expectrl::Session::spawn(cmd).expect("Failed to spawn TUI process");

        // PTY のウィンドウサイズを ioctl (TIOCSWINSZ) で設定
        // 環境変数 COLUMNS/LINES は PTY サイズに影響しないため ioctl が必要
        session
            .get_process_mut()
            .set_window_size(200, 50)
            .expect("Failed to set PTY window size");

        expectrl::session::log(session, std::io::stderr()).expect("Failed to set up logging")
    }

    /// TUI をデフォルト引数で起動する。
    fn spawn_tui(&self) -> TuiSession {
        self.spawn_tui_with_args(&[])
    }
}

/// ANSI エスケープシーケンスを除去してプレーンテキストにする
fn strip_ansi(input: &[u8]) -> String {
    let s = String::from_utf8_lossy(input);
    regex_lite_strip(&s)
}

/// 簡易 ANSI ストリッパー（正規表現クレート不要版）
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

// ─── テスト ─────────────────────────────────────────────

/// 環境確認: expectrl の PTY が WSL2 で正しく動作するかの最小テスト
#[test]
fn test_expectrl_smoke() {
    let binary = env!("CARGO_BIN_EXE_remote-merge");

    // --help を使って PTY + 出力キャプチャが動くことを確認
    let mut cmd = Command::new(binary);
    cmd.arg("--help");

    let mut session = expectrl::Session::spawn(cmd).expect("Failed to spawn process");

    session.set_expect_timeout(Some(Duration::from_secs(5)));

    // "Usage" が表示されるのを待つ
    let result = session.expect("Usage");
    assert!(
        result.is_ok(),
        "Expected to find 'Usage' in output, but timed out or failed: {:?}",
        result.err()
    );

    eprintln!("SUCCESS: expectrl PTY works on this environment");
}

/// TUI が localhost SSH 経由で起動し、ファイルツリーが表示されることを確認
#[test]
fn test_tui_startup_with_localhost() {
    let env = E2eEnv::new(
        &[("hello.txt", "Hello from local\n")],
        &[("hello.txt", "Hello from remote\n")],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // TUI が起動し、ファイル名がツリーに表示されるのを待つ
    let result = session.expect("hello.txt");
    assert!(
        result.is_ok(),
        "TUI should show 'hello.txt' in file tree: {:?}",
        result.err()
    );

    // q で終了
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: TUI startup with localhost SSH works");
}

/// Bug 1 再現テスト: Enter 連打で diff が消失しないことを確認
///
/// 手順:
/// 1. 差分のあるファイルを1つ用意
/// 2. TUI 起動 → ファイルが表示されるのを待つ
/// 3. Enter でファイル選択 → diff が表示される（差分行が見える）
/// 4. Tab でツリーに戻る → 再度 Enter → diff がまだ表示される（差分行が見える）
/// 5. 繰り返す
///
/// 各 Enter 後に diff 内容のマーカー文字列が画面に出力されることを確認する。
/// 「Select a file to view diff」が出たら diff が消えたことを意味する。
#[test]
fn test_enter_spam_does_not_lose_diff() {
    let local_content = "line1\nline2\nline3\nLOCALMARKER\n";
    let remote_content = "line1\nline2\nline3\nREMOTEMARKER\n";

    let env = E2eEnv::new(
        &[("test.txt", local_content)],
        &[("test.txt", remote_content)],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーにファイルが表示されるのを待つ
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "Should see 'test.txt' in tree: {:?}",
        result.err()
    );

    // Enter 連打を複数回繰り返す
    for i in 1..=5 {
        // Enter でファイル選択（初回以外は Tab → Enter）
        if i > 1 {
            session
                .send("\t")
                .unwrap_or_else(|_| panic!("Failed to send Tab (round {})", i));
            thread::sleep(Duration::from_millis(300));
        }

        session
            .send("\r")
            .unwrap_or_else(|_| panic!("Failed to send Enter (round {})", i));

        // diff 内容のマーカーが表示されるのを待つ
        // LOCALMARKER は左側の差分行として表示されるはず
        let result = session.expect("LOCALMARKER");
        assert!(
            result.is_ok(),
            "Round {}: diff content 'LOCALMARKER' should be visible after Enter. \
             Diff may have disappeared! Error: {:?}",
            i,
            result.err()
        );

        eprintln!("Round {}: diff content confirmed visible", i);
    }

    // 正常終了
    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: Enter spam (5 rounds) did not cause diff to disappear");
}

/// 複数ファイル + ディレクトリ構造で Enter 連打テスト。
///
/// ディレクトリ展開 → ファイル選択 → diff 表示 → Tab → Enter で diff が消失しないか確認。
/// "/" 検索を使って確実にファイルにカーソルを合わせる。
#[test]
fn test_enter_spam_with_directory_structure() {
    let env = E2eEnv::new(
        &[
            (
                "src/main.rs",
                "fn main() {\n    println!(\"LOCALMARK\");\n}\n",
            ),
            (
                "src/lib.rs",
                "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            ),
        ],
        &[
            (
                "src/main.rs",
                "fn main() {\n    println!(\"REMOTEMARK\");\n}\n",
            ),
            (
                "src/lib.rs",
                "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            ),
        ],
    );

    let mut session = env.spawn_tui();
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーの表示を待つ
    let result = session.expect("src");
    assert!(
        result.is_ok(),
        "Should see 'src' dir in tree: {:?}",
        result.err()
    );

    // src ディレクトリを展開 (Enter)  — 初期カーソルは src/ にある（ディレクトリが先頭）
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // "/" 検索で main.rs にジャンプ
    session.send("/").expect("Failed to send /");
    thread::sleep(Duration::from_millis(200));
    session.send("main").expect("Failed to send search text");
    thread::sleep(Duration::from_millis(300));
    session
        .send("\r")
        .expect("Failed to send Enter for search confirm");
    thread::sleep(Duration::from_millis(500));

    // Enter で main.rs を選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff に "LOCALMARK" が表示されるはず
    let result = session.expect("LOCALMARK");
    assert!(
        result.is_ok(),
        "Should see diff content 'LOCALMARK' after selecting main.rs: {:?}",
        result.err()
    );
    eprintln!("main.rs diff confirmed visible");

    // Tab → Enter × 3 で diff が消えないことを確認
    for i in 2..=4 {
        session.send("\t").unwrap();
        thread::sleep(Duration::from_millis(300));
        session.send("\r").unwrap();
        let result = session.expect("LOCALMARK");
        assert!(
            result.is_ok(),
            "Round {}: diff content 'LOCALMARK' should still be visible: {:?}",
            i,
            result.err()
        );
        eprintln!("Round {}: main.rs diff confirmed visible", i);
    }

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: Enter spam with directory structure did not lose diff");
}

/// Bug 1 再現: 3way モードで Enter 連打すると diff 行の [C!] バッジが [3≠] に劣化する
///
/// 再現手順（スクショ通り）:
/// 1. develop <-> staging + ref=local の 3way 構成
/// 2. 3サーバーで全てファイル内容が異なる → [C!] バッジが出る
/// 3. shared/ ディレクトリを展開 → config.json で Enter → diff 行に [C!] 表示
/// 4. もう一度 config.json で Enter → [C!] が [3≠] に劣化する（バグ）
///
/// 根本原因: `invalidate_cache_for_paths()` が `conflict_cache` を消すため、
/// 2回目の `select_file()` で conflict 情報が失われ [C!] → [3≠] に劣化する。
#[test]
fn test_3way_conflict_badge_survives_reenter() {
    // 3サーバーで全て内容が異なるファイル → [C!] が正しいバッジ
    let env = E2eEnv::new_3way(
        // local (ref): 元のバージョン
        &[("shared/config.json", "shared config content\n")],
        // develop (left): develop 版
        &[(
            "shared/config.json",
            "shared config content (remote version)\n",
        )],
        // staging (right): staging 版
        &[(
            "shared/config.json",
            "shared config content (remote version2)\n",
        )],
    );

    // develop <-> staging で起動し、local を reference にする
    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーに shared/ が表示されるのを待つ
    let result = session.expect("shared");
    assert!(
        result.is_ok(),
        "Should see 'shared' dir in tree: {:?}",
        result.err()
    );

    // shared/ を展開 (Enter)
    session.send("\r").expect("Failed to send Enter for expand");
    thread::sleep(Duration::from_secs(1));

    // 展開後の画面をダンプして config.json が見えるか確認
    {
        let mut buf = vec![0u8; 64 * 1024];
        let n = session.try_read(&mut buf).unwrap_or(0);
        let plain = strip_ansi(&buf[..n]);
        eprintln!("=== AFTER EXPAND ({} bytes) ===", n);
        eprintln!("{}", plain);
        eprintln!("=== END ===");
    }

    // j で config.json に移動
    session.send("j").expect("Failed to send j");
    thread::sleep(Duration::from_millis(500));

    // 移動後の画面ダンプ
    {
        let mut buf = vec![0u8; 64 * 1024];
        let n = session.try_read(&mut buf).unwrap_or(0);
        let plain = strip_ansi(&buf[..n]);
        eprintln!("=== AFTER J ({} bytes) ===", n);
        eprintln!("{}", plain);
        eprintln!("=== END ===");
    }

    // 1回目の Enter: config.json を選択 → diff 表示 → [C!] バッジが出るはず
    session.send("\r").expect("Failed to send Enter (1st)");

    // diff 表示 + conflict 計算を待つ
    thread::sleep(Duration::from_secs(3));

    // 画面の内容を try_read で取得して C! が含まれるか確認
    let mut buf = vec![0u8; 64 * 1024];
    let n = session.try_read(&mut buf).unwrap_or(0);
    let screen_content = String::from_utf8_lossy(&buf[..n]);
    let plain = strip_ansi(screen_content.as_bytes());
    eprintln!("=== AFTER 1ST ENTER ({} bytes) ===", n);
    eprintln!("{}", plain);
    eprintln!("=== END ===");

    assert!(
        plain.contains("C!") || screen_content.contains("C!"),
        "1st Enter: Expected [C!] conflict badge in diff view. \
         Screen content (stripped): {}",
        &plain[..plain.len().min(500)]
    );
    eprintln!("1st Enter: [C!] badge confirmed");

    // 2回目の Enter: Tab でツリーに戻って再度 Enter
    // バグ: ここで [C!] が [3≠] に劣化する
    session.send("\t").expect("Failed to send Tab");
    thread::sleep(Duration::from_millis(300));
    session.send("\r").expect("Failed to send Enter (2nd)");
    thread::sleep(Duration::from_secs(2));

    // [C!] がまだ表示されているか確認
    let result = session.expect("C!]");
    if result.is_err() {
        // [C!] が見つからない → バグ再現！
        session.send("q").ok();
        panic!(
            "BUG REPRODUCED: 2nd Enter caused [C!] conflict badge to disappear. \
             This is likely because invalidate_cache_for_paths() clears conflict_cache, \
             causing the badge to degrade from [C!] to [3≠]."
        );
    }

    eprintln!("2nd Enter: [C!] badge still present (bug is fixed!)");

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way [C!] badge survives re-Enter");
}

/// 3way 構成で right 側のファイルが正しく読み込めることを確認。
///
/// バグ: develop <-> staging + ref=local で起動すると、
/// right (staging) 側が「R: not found」になり diff が片側しか表示されない。
/// 正常時は diff に left と right 両方のコンテンツが表示されるべき。
#[test]
fn test_3way_right_side_content_loads() {
    let env = E2eEnv::new_3way(
        // local (ref)
        &[("test.txt", "original content\n")],
        // develop (left)
        &[("test.txt", "LEFTCONTENT develop version\n")],
        // staging (right)
        &[("test.txt", "RIGHTCONTENT staging version\n")],
    );

    let mut session =
        env.spawn_tui_with_args(&["--left", "develop", "--right", "staging", "--ref", "local"]);
    session.set_expect_timeout(Some(Duration::from_secs(10)));

    // ファイルツリーにバッジ付きファイルが表示されるのを待つ
    // [+] or [M] バッジが付くまで待つことでツリーロード完了を確認
    let result = session.expect("test.txt");
    assert!(
        result.is_ok(),
        "Should see 'test.txt' in tree: {:?}",
        result.err()
    );

    // SSH 接続確立 + ツリー完成を待つ
    thread::sleep(Duration::from_secs(3));

    // Enter でファイル選択 → diff 表示
    session.send("\r").expect("Failed to send Enter");

    // diff 内容が表示されるのを expect で待つ
    let result = session.expect("LEFTCONTENT");
    assert!(
        result.is_ok(),
        "Left side content 'LEFTCONTENT' should be visible in diff: {:?}",
        result.err()
    );

    // right 側コンテンツを expect で待つ — R: not found バグならタイムアウトする
    let result = session.expect("RIGHTCONTENT");
    assert!(
        result.is_ok(),
        "Right side content 'RIGHTCONTENT' should be visible in diff. \
         If 'R: not found' appears instead, right side file loading is broken: {:?}",
        result.err()
    );

    session.send("q").expect("Failed to send quit");
    thread::sleep(Duration::from_millis(500));

    eprintln!("SUCCESS: 3way right side content loads correctly");
}

/// 存在しないサーバー名を --right に渡すとエラーで起動を拒否すること。
///
/// 以前は offline mode で起動してしまい「R: not found」になっていた。
/// config にないサーバー名はバリデーションで弾くべき。
#[test]
fn test_invalid_server_name_rejected() {
    let env = E2eEnv::new(&[("test.txt", "local\n")], &[("test.txt", "remote\n")]);

    let binary = env!("CARGO_BIN_EXE_remote-merge");
    let output = Command::new(binary)
        .arg("--config")
        .arg(&env.config_path)
        .arg("--left")
        .arg("develop")
        .arg("--right")
        .arg("nonexistent_server")
        .output()
        .expect("Failed to execute binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);

    assert!(
        !output.status.success(),
        "Should fail when server name is not in config. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(
        combined.contains("not found in config"),
        "Error message should mention 'not found in config'. Output: {}",
        combined
    );

    eprintln!("SUCCESS: Invalid server name correctly rejected at startup");
}
