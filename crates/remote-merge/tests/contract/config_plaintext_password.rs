#![cfg(unix)]
//! 設定の平文の password を使うときの警告（docs/ir/config/FLAGS.md の FLAG-config-011）も、ほかの
//! 利用者に向けた警告と同じく、診断の記録の細かさ（--log-level・RUST_LOG）によらず標準エラーに出す
//! こと（docs/decision/records/2026-10-12-foundation-5.md の F51・F65）の契約テスト

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

const WARNING: &str = "Server 'fixture': using plaintext password from config.";

// @kotowari[FLAG-config-011]
#[tokio::test(flavor = "multi_thread")]
async fn the_plaintext_password_warning_is_shown_at_any_log_level() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a.txt", Some("same\n"), Some("same\n"));

    for level in ["error", "off"] {
        let output = fixture.run(&["--log-level", level, "status"]);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(WARNING), "{level}: {stderr}");
    }
}
