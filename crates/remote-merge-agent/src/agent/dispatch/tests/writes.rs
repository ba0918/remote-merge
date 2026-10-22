//! WriteFile・Symlink

use super::support::*;

// ── WriteFile ──

#[test]
fn write_file_single() {
    let (tmp, mut d) = setup();

    let resp = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "out.txt".into(),
            content: b"data".to_vec(),
            is_binary: false,
            more_to_follow: false,
        })
        .unwrap(),
    );

    assert_eq!(resp, AgentResponse::Changed { failure: None });
    assert_eq!(
        fs::read_to_string(tmp.path().join("out.txt")).unwrap(),
        "data"
    );
}

#[test]
fn write_file_chunked_appends() {
    let (tmp, mut d) = setup();

    // 最初のチャンク（more_to_follow=true）
    let resp1 = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "chunked.bin".into(),
            content: b"chunk1".to_vec(),
            is_binary: true,
            more_to_follow: true,
        })
        .unwrap(),
    );
    assert_eq!(resp1, AgentResponse::Changed { failure: None });

    // 2番目のチャンク（more_to_follow=false で完了）
    let resp2 = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "chunked.bin".into(),
            content: b"chunk2".to_vec(),
            is_binary: true,
            more_to_follow: false,
        })
        .unwrap(),
    );
    assert_eq!(resp2, AgentResponse::Changed { failure: None });

    let content = fs::read(tmp.path().join("chunked.bin")).unwrap();
    assert_eq!(content, b"chunk1chunk2");
}

#[test]
fn write_file_second_transfer_starts_fresh() {
    let (tmp, mut d) = setup();

    // 1回目の転送
    d.dispatch(AgentRequest::WriteFile {
        path: "file.txt".into(),
        content: b"first".to_vec(),
        is_binary: false,
        more_to_follow: false,
    });

    // 2回目の転送（同じパス）— is_first_chunk=true になるはず
    d.dispatch(AgentRequest::WriteFile {
        path: "file.txt".into(),
        content: b"second".to_vec(),
        is_binary: false,
        more_to_follow: false,
    });

    let content = fs::read_to_string(tmp.path().join("file.txt")).unwrap();
    assert_eq!(content, "second");
}

// ── Symlink ──

#[test]
fn symlink_success() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("target.txt"), "link target").unwrap();

    let resp = single(
        d.dispatch(AgentRequest::Symlink {
            path: "link.txt".into(),
            target: "target.txt".into(),
        })
        .unwrap(),
    );

    assert_eq!(resp, AgentResponse::Changed { failure: None });

    let link = tmp.path().join("link.txt");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&link).unwrap(), "link target");
}

#[test]
// リンク先が ".." で始まる symlink も、ほかの経路と同じくそのまま作る（REQ-merge-023）
fn a_symlink_to_a_parent_target_is_created() {
    let (tmp, mut d) = setup();

    let resp = single(
        d.dispatch(AgentRequest::Symlink {
            path: "link".into(),
            target: "../etc/passwd".into(),
        })
        .unwrap(),
    );

    assert_eq!(resp, AgentResponse::Changed { failure: None });
    assert_eq!(
        fs::read_link(tmp.path().join("link")).unwrap(),
        std::path::Path::new("../etc/passwd")
    );
}
