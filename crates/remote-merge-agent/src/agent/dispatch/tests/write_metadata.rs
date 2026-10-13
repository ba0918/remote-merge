//! WriteFile の所有者と権限の扱い（MetadataConfig）

use super::support::*;

fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn write_file_propagates_metadata_config() {
    let (tmp, mut d) = setup_with_config(MetadataConfig {
        default_uid: Some(1000),
        default_gid: Some(1000),
    });

    let resp = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "new_file.txt".into(),
            content: b"hello".to_vec(),
            is_binary: false,
            more_to_follow: false,
        })
        .unwrap(),
    );

    assert_eq!(resp, AgentResponse::Changed { failure: None });
    // ファイルが書き込まれていること
    assert_eq!(
        fs::read_to_string(tmp.path().join("new_file.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn write_file_chunked_saved_metadata_management() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, mut d) = setup_with_config(MetadataConfig {
        default_uid: None,
        default_gid: None,
    });

    // 既存ファイルを作成
    fs::write(tmp.path().join("existing.txt"), "old content").unwrap();
    // パーミッションを設定
    fs::set_permissions(
        tmp.path().join("existing.txt"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    // 最初の分: 開いたファイルを最後の分まで持つ
    let resp1 = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "existing.txt".into(),
            content: b"chunk1".to_vec(),
            is_binary: false,
            more_to_follow: true,
        })
        .unwrap(),
    );
    assert_eq!(resp1, AgentResponse::Changed { failure: None });

    assert!(d.writing.contains_key("existing.txt"));

    // 最後の分: 既存のファイルはその場で書き換えたので、権限はもとのまま
    let resp2 = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "existing.txt".into(),
            content: b"chunk2".to_vec(),
            is_binary: false,
            more_to_follow: false,
        })
        .unwrap(),
    );
    assert_eq!(resp2, AgentResponse::Changed { failure: None });

    // コンテンツが正しく結合されていること
    assert_eq!(
        fs::read(tmp.path().join("existing.txt")).unwrap(),
        b"chunk1chunk2"
    );

    assert_eq!(mode_of(&tmp.path().join("existing.txt")), 0o755);
    // 書き終えたら覚えたことを消す
    assert!(!d.writing.contains_key("existing.txt"));
}

#[test]
fn a_new_file_keeps_the_mode_it_was_created_with_like_the_local_route() {
    let (tmp, mut d) = setup_with_config(MetadataConfig {
        default_uid: None,
        default_gid: None,
    });

    let resp = single(
        d.dispatch(AgentRequest::WriteFile {
            path: "brand_new.txt".into(),
            content: b"data".to_vec(),
            is_binary: false,
            more_to_follow: false,
        })
        .unwrap(),
    );
    assert_eq!(resp, AgentResponse::Changed { failure: None });
    assert_eq!(
        fs::read_to_string(tmp.path().join("brand_new.txt")).unwrap(),
        "data"
    );
    // 新しいファイルの権限は、どの経路でも書いた後に merge が設定の値で付ける。エージェントは
    // ローカルの経路と同じく、作ったときの権限のままにする
    let local = tempfile::tempdir().unwrap();
    remote_merge_core::machine_ops::write(
        local.path(),
        &remote_merge_core::relative_path::EntryPath::parse("brand_new.txt").unwrap(),
        b"data",
    )
    .unwrap();
    assert_eq!(
        mode_of(&tmp.path().join("brand_new.txt")),
        mode_of(&local.path().join("brand_new.txt"))
    );
    assert!(!d.writing.contains_key("brand_new.txt"));
}

// @kotowari[REQ-merge-042]
#[test]
fn a_write_or_a_link_failing_after_creating_parents_leaves_no_parent() {
    let (tmp, mut d) = setup_with_config(MetadataConfig::default());
    let long = "x".repeat(300);

    let written = single(
        d.dispatch(AgentRequest::WriteFile {
            path: format!("new/sub/{long}"),
            content: b"data".to_vec(),
            is_binary: false,
            more_to_follow: false,
        })
        .unwrap(),
    );
    let linked = single(
        d.dispatch(AgentRequest::Symlink {
            path: format!("other/{long}"),
            target: "t".into(),
        })
        .unwrap(),
    );

    assert!(
        matches!(written, AgentResponse::Changed { failure: Some(_) }),
        "{written:?}"
    );
    assert!(
        matches!(linked, AgentResponse::Changed { failure: Some(_) }),
        "{linked:?}"
    );
    assert!(!tmp.path().join("new").exists());
    assert!(!tmp.path().join("other").exists());
}
