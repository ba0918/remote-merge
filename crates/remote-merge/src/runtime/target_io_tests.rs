//! 比較先（ローカルの経路とエージェントの経路）の操作の単体テスト
use remote_merge_core::op_failure::{OpFailure, PathResult, SizeLimit};
use remote_merge_core::relative_path::EntryPath;
use tempfile::TempDir;

use super::*;
use crate::runtime::local_target::LocalTargetIo;
use remote_merge_core::op_failure::MAX_FILE_SIZE;

fn entry(path: &str) -> EntryPath {
    EntryPath::parse(path).unwrap()
}

/// root_dir を `tmp` にした runtime
fn local_runtime(tmp: &TempDir) -> CoreRuntime {
    let mut rt = CoreRuntime::new_for_test();
    rt.config.local.root_dir = tmp.path().to_path_buf();
    rt
}

#[test]
fn local_target_reads_writes_and_removes_files_below_its_root() {
    let tmp = TempDir::new().unwrap();
    let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![]);
    let mut runtime = CoreRuntime::new_for_test();

    io.write_file_bytes(&mut runtime, &entry("nested/file.txt"), b"content")
        .unwrap();
    let read = io
        .read_entries(
            &mut runtime,
            &[entry("nested/file.txt")],
            SizeLimit::Unlimited,
        )
        .unwrap();
    assert_eq!(read[0].1.as_deref(), Ok(&b"content"[..]));
    io.remove_file(&mut runtime, &entry("nested/file.txt"))
        .unwrap();
    assert!(!tmp.path().join("nested/file.txt").exists());
}

#[cfg(unix)]
#[test]
fn local_target_creates_symlinks() {
    let tmp = TempDir::new().unwrap();
    let mut io = LocalTargetIo::new(tmp.path().to_path_buf(), vec![]);
    let mut runtime = CoreRuntime::new_for_test();

    io.create_symlink(&mut runtime, &entry("link.txt"), "target.txt")
        .unwrap();

    assert_eq!(
        std::fs::read_link(tmp.path().join("link.txt")).unwrap(),
        std::path::PathBuf::from("target.txt")
    );
}

// ── 大きさの上限（ローカルの経路） ──

/// root の下に大きさ `len` の疎なファイルを置いて読めなくし、読めなくなったか（root で動くと
/// 読めてしまう）を返す
#[cfg(unix)]
fn place_unreadable_sparse(root: &std::path::Path, name: &str, len: u64) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join(name);
    std::fs::File::create(&path).unwrap().set_len(len).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::read(&path).is_err()
}

/// 上限を超えるファイルと上限ちょうどのファイルを読めなくして置いた runtime。前提が崩れたら
/// None（root で動くと読めてしまう）
#[cfg(unix)]
fn runtime_with_unreadable_files_around_the_limit(tmp: &TempDir) -> Option<CoreRuntime> {
    let limit = MAX_FILE_SIZE;
    let placed = place_unreadable_sparse(tmp.path(), "over", limit + 1)
        && place_unreadable_sparse(tmp.path(), "at", limit);
    if !placed {
        eprintln!("note: running as root; unreadable files cannot be made");
        return None;
    }
    Some(local_runtime(tmp))
}

/// ローカルの経路で `paths` を `limit` を当てて読む
fn local_reads(rt: &mut CoreRuntime, paths: &[&str], limit: SizeLimit) -> Vec<PathResult<Vec<u8>>> {
    let paths: Vec<EntryPath> = paths.iter().map(|path| entry(path)).collect();
    let mut io = for_side(&Side::Local, rt);
    io.read_entries(rt, &paths, limit).unwrap()
}

fn too_large(read: &PathResult<Vec<u8>>) -> bool {
    matches!(read.1, Err(OpFailure::TooLarge { .. }))
}

#[cfg(unix)]
#[test]
fn a_file_over_the_limit_is_refused_by_its_size_before_it_is_read() {
    let tmp = TempDir::new().unwrap();
    let Some(mut rt) = runtime_with_unreadable_files_around_the_limit(&tmp) else {
        return;
    };

    let reads = local_reads(&mut rt, &["over", "at"], SizeLimit::from_force(false));

    assert!(too_large(&reads[0]), "{reads:?}");
    assert!(!too_large(&reads[1]), "{reads:?}");
    assert!(reads[1].1.is_err(), "{reads:?}");
}

#[cfg(unix)]
#[test]
fn a_lifted_limit_reads_without_asking_the_size() {
    let tmp = TempDir::new().unwrap();
    let Some(mut rt) = runtime_with_unreadable_files_around_the_limit(&tmp) else {
        return;
    };

    let reads = local_reads(&mut rt, &["over"], SizeLimit::Unlimited);

    assert!(!too_large(&reads[0]), "{reads:?}");
}

#[cfg(unix)]
#[test]
fn a_batch_answers_a_file_over_the_limit_as_too_large_and_reads_the_rest() {
    let tmp = TempDir::new().unwrap();
    let Some(mut rt) = runtime_with_unreadable_files_around_the_limit(&tmp) else {
        return;
    };
    std::fs::write(tmp.path().join("small"), "s").unwrap();

    let reads = local_reads(&mut rt, &["over", "small"], SizeLimit::from_force(false));

    assert!(too_large(&reads[0]), "{reads:?}");
    assert_eq!(reads[1].1.as_deref(), Ok(&b"s"[..]));
}

// ── エージェントの経路 ──

/// サーバ "develop" に、最初の問い合わせに `response` を返す（None なら答えずに接続を切る）
/// 試験用のエージェントをつないだ runtime と、エージェントが受けた問い合わせの数
#[cfg(unix)]
fn runtime_with_scripted_agent(
    response: Option<crate::agent::protocol::AgentResponse>,
) -> (CoreRuntime, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let mut rt = crate::runtime::test_support::runtime_with_server("develop", "/srv/app", false);
    let received = crate::runtime::scripted_agent::connect(&mut rt, "develop", response);
    (rt, received)
}

#[cfg(unix)]
fn requests(received: &std::sync::atomic::AtomicUsize) -> usize {
    received.load(std::sync::atomic::Ordering::SeqCst)
}

/// サーバ "develop" の経路で "a.bin" を上限を当てて読む
fn remote_reads(rt: &mut CoreRuntime) -> anyhow::Result<Vec<PathResult<Vec<u8>>>> {
    let mut io = for_side(&Side::Remote("develop".into()), rt);
    io.read_entries(rt, &[entry("a.bin")], SizeLimit::from_force(false))
}

/// エージェントの走査の失敗は走査の誤りにし、エージェントも外さない
#[cfg(unix)]
#[test]
fn a_scan_failure_from_the_agent_is_a_scan_error_and_keeps_the_agent() {
    use remote_merge_core::scan::outcome::{ScanFailure, ScanRequest, ScanScope};
    let failure = ScanFailure::CollectFailed {
        message: "disk went away".into(),
    };
    let (mut rt, received) =
        runtime_with_scripted_agent(Some(crate::agent::protocol::AgentResponse::TreeFailed {
            failure: failure.clone(),
        }));
    let request = ScanRequest {
        scope: ScanScope::Whole,
        max_entries: 100,
    };
    let mut io = for_side(&Side::Remote("develop".into()), &rt);

    let error = io.scan_listing(&mut rt, &request).unwrap_err();

    assert_eq!(
        error.downcast_ref::<ScanFailure>(),
        Some(&failure),
        "{error:#}"
    );
    assert_eq!(requests(&received), 1);
    assert!(rt.has_agent("develop"));
}

// エージェントのエラー応答は、尋ねたパスの失敗として返り、エージェントに一度だけ尋ね、
// エージェントを使い続ける
#[cfg(unix)]
#[test]
fn an_error_returned_by_the_agent_fails_the_question_and_keeps_the_agent() {
    let (mut rt, received) =
        runtime_with_scripted_agent(Some(crate::agent::protocol::AgentResponse::Error {
            message: "read failed".into(),
        }));

    let error = remote_reads(&mut rt).unwrap_err();

    assert!(format!("{error:#}").contains("read failed"), "{error:#}");
    assert_eq!(requests(&received), 1);
    assert!(rt.agent_clients.contains_key("develop"));
}

// 接続断は、エージェントを起動し直して一度だけ尋ね直す。起動し直せなければ接続断のまま返す（この
// 試験では SSH につながっていないので起動し直せない）
#[cfg(unix)]
#[test]
fn a_lost_agent_connection_stays_a_connection_loss_when_the_agent_cannot_be_restarted() {
    let (mut rt, received) = runtime_with_scripted_agent(None);

    let error = remote_reads(&mut rt).unwrap_err();

    assert_eq!(
        crate::runtime::connection::classify(&error),
        crate::runtime::connection::Failure::Lost,
        "{error:#}"
    );
    assert!(
        format!("{error:#}").contains("could not be restarted"),
        "{error:#}"
    );
    assert_eq!(requests(&received), 1);
}

// エージェントが尋ねたパスに答えなかったら、要求と合わない応答（接続断）にする（REQ-ssh-010・D20）
#[cfg(unix)]
#[test]
fn a_path_the_agent_leaves_out_is_a_disconnection() {
    for sudo in [false, true] {
        let (mut rt, received) = runtime_with_scripted_agent(Some(
            crate::agent::protocol::AgentResponse::FileContents {
                results: vec![],
                is_last: true,
                refused: vec![],
            },
        ));
        rt.config.servers.get_mut("develop").unwrap().sudo = sudo;

        let error = remote_reads(&mut rt).unwrap_err();

        assert!(
            crate::runtime::connection::classify(&error).is_disconnection(),
            "{error:#}"
        );
        assert_eq!(requests(&received), 1);
        assert!(!rt.has_client("develop"));
    }
}
