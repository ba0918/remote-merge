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

    // 最初の分: 書き込み先がもとからあったことを覚える
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

    assert_eq!(d.writing.get("existing.txt"), Some(&false));

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
    // 新しく作ったファイルには既定の権限が付く
    assert_eq!(mode_of(&tmp.path().join("brand_new.txt")), 0o600);
    assert!(!d.writing.contains_key("brand_new.txt"));
}
