#![cfg(unix)]
//! root_dir 全体を指す値（"/"・"."・""・"//"・"./"）は、ローカル・SSH・エージェントのどの経路でも
//! 一つの項目への操作に届かず、root_dir の権限と中身を変えないことの契約テスト（ローカルの経路）。
//!
//! 項目の操作は root_dir の下の項目の型だけを受け取るので、文字列から型にするところで止まる。
//! SSH とエージェントの経路は、手元の known_hosts に触れないよう、製品の runtime の単体テスト
//! （side_io）で、経路に要求が届かないことを確かめる。エージェントの受け取り口は dispatch の
//! 単体テストが確かめる。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use remote_merge::config::load_config_from_paths;
use remote_merge::runtime::{CoreRuntime, RuntimeTargets};
use remote_merge::side::Side;

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

const ROOT_FORMS: [&str; 5] = ["/", ".", "", "//", "./"];
const REFUSAL: &str = "root_dir itself is not an entry";

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// 一つの項目への操作のそれぞれに root_dir 全体を渡したときの失敗の文言
fn refusals_for_each_operation(core: &mut CoreRuntime, side: &Side, root: &str) -> Vec<String> {
    let before = remote_merge::runtime::change_check::Before::Unknown;
    vec![
        core.chmod_file(side, root, 0o700),
        core.write_file(side, root, "x"),
        core.write_file_bytes(side, root, b"x", &before),
        core.remove_file(side, root),
        core.create_symlink(side, root, "x"),
        core.read_file_bytes(side, root, true).map(|_| ()),
        core.current_link_target(side, root).map(|_| ()),
        core.stat_files(side, &[root.to_string()]).map(|_| ()),
    ]
    .into_iter()
    .map(|result| {
        format!(
            "{:#}",
            result.expect_err("an operation on root_dir itself must fail")
        )
    })
    .collect()
}

/// `side` を `target_root` の経路として、root_dir 全体を指す値をどの項目の操作にも渡せず、
/// root_dir の権限と中身が変わらないことを確かめる。CoreRuntime は自分の非同期の実行環境を持つので、
/// テストの実行環境の外のスレッドで動かす
async fn assert_root_dir_untouched(fixture: &AgentFixture, side: Side, target_root: PathBuf) {
    fs::set_permissions(&target_root, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(target_root.join("kept.txt"), "kept\n").unwrap();
    let config = load_config_from_paths(Some(&fixture.config), None).unwrap();
    let checked_side = side.clone();
    tokio::task::spawn_blocking(move || {
        let mut core = CoreRuntime::with_targets(config, RuntimeTargets::production());
        core.connect_if_remote(&checked_side).unwrap();
        for root in ROOT_FORMS {
            for message in refusals_for_each_operation(&mut core, &checked_side, root) {
                assert!(
                    message.contains(REFUSAL),
                    "{checked_side:?} {root:?}: {message}"
                );
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(mode_of(&target_root), 0o755, "{side:?}");
    assert_eq!(
        fs::read_to_string(target_root.join("kept.txt")).unwrap(),
        "kept\n"
    );
}

// @kotowari[REQ-paths-013]
#[tokio::test(flavor = "multi_thread")]
async fn the_local_route_never_operates_on_the_root_dir_itself() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    assert_root_dir_untouched(&fixture, Side::Local, fixture.local()).await;
}
