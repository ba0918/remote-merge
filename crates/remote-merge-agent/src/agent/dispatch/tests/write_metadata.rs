//! WriteFile の所有者と権限の扱い（MetadataConfig）

use super::support::*;

#[test]
fn write_file_propagates_metadata_config() {
    let (tmp, mut d) = setup_with_config(MetadataConfig {
        default_uid: Some(1000),
        default_gid: Some(1000),
        file_permissions: Some(0o644),
        dir_permissions: Some(0o755),
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
        file_permissions: Some(0o600),
        dir_permissions: None,
    });

    // 既存ファイルを作成
    fs::write(tmp.path().join("existing.txt"), "old content").unwrap();
    // パーミッションを設定
    fs::set_permissions(
        tmp.path().join("existing.txt"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    // 1st chunk: is_first_chunk=true → SavedMetadata を保存する
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

    // SavedMetadata がディスパッチャーに保存されていることを確認
    assert!(d.saved_metadata.contains_key("existing.txt"));
    assert!(d.saved_metadata["existing.txt"].is_some());

    // 2nd chunk: is_last_chunk=true → SavedMetadata を使ってメタデータを復元
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

    // 転送完了後、saved_metadata がクリーンアップされていること
    assert!(!d.saved_metadata.contains_key("existing.txt"));
    assert!(!d.written_paths.contains("existing.txt"));
}

#[test]
fn write_file_new_file_uses_config_defaults() {
    let (tmp, mut d) = setup_with_config(MetadataConfig {
        default_uid: None,
        default_gid: None,
        file_permissions: Some(0o600),
        dir_permissions: None,
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
    // saved_metadata はクリーンアップ済み
    assert!(!d.saved_metadata.contains_key("brand_new.txt"));
}
