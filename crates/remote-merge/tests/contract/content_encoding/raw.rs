use super::super::args_support::{merge_args, rollback_args};
use super::super::backup_support::{config, merge_files, restore, targets};
use tempfile::TempDir;

// @kotowari[REQ-content-006, REQ-merge-045]
#[test]
fn invalid_hunk_interpretation_never_reserves_or_cleans_backups_even_with_force() {
    use remote_merge::cli::merge::execute_merge;
    use remote_merge_core::content::{Encoding, Interpretation};
    // 読み込み元の中身を指定の文字コードで読めないものと、左右の文字コードが違うもの。どちらの誤りも
    // パス "a" と、読めなかった文字コードか文字コードの違いを示す
    for (source, left_codec, right_codec, named) in [
        (
            b"\xa4".as_slice(),
            Encoding::EucJp,
            Encoding::EucJp,
            "euc-jp",
        ),
        (
            b"valid\n".as_slice(),
            Encoding::Utf8,
            Encoding::EucJp,
            "encoding",
        ),
    ] {
        for dry_run in [false, true] {
            let local = TempDir::new().unwrap();
            let remote = TempDir::new().unwrap();
            let backup = TempDir::new().unwrap();
            let config = config(&local, &remote, true);
            std::fs::write(local.path().join("a"), b"seed\n").unwrap();
            std::fs::write(remote.path().join("a"), b"\xa4\xa2\n").unwrap();
            let old_targets = super::super::backup_support::targets_at(
                &remote,
                &backup,
                super::super::backup_support::fixed_now() - chrono::Duration::days(8),
            );
            assert_eq!(
                merge_files(merge_args(&["a"]), config.clone(), old_targets)
                    .merged
                    .len(),
                1
            );
            std::fs::write(local.path().join("a"), source).unwrap();
            std::fs::write(remote.path().join("a"), b"\xa4\xa2\n").unwrap();
            let before = super::super::backup_support::store_entries(backup.path());
            let mut args = merge_args(&["a"]);
            args.hunks = Some(vec![0]);
            args.force = true;
            args.dry_run = dry_run;
            args.encodings.left = Some(Interpretation::Text(left_codec));
            args.encodings.right = Some(Interpretation::Text(right_codec));
            let error = execute_merge(args, config, targets(&remote, &backup))
                .map(|result| result.exit_code)
                .expect_err("the hunk merge went on with an invalid interpretation");
            let error = format!("{error:#}");
            assert!(error.starts_with("a: ") && error.contains(named), "{error}");
            assert_eq!(
                super::super::backup_support::store_entries(backup.path()),
                before
            );
            assert_eq!(
                std::fs::read(remote.path().join("a")).unwrap(),
                b"\xa4\xa2\n"
            );
        }
    }
}

// @kotowari[REQ-content-004, EX-content-007]
#[test]
fn ex_content_007_rollback_restores_raw_bytes_after_encoding_configuration_changes() {
    let local = TempDir::new().unwrap();
    let remote = TempDir::new().unwrap();
    let backup = TempDir::new().unwrap();
    let mut config = config(&local, &remote, true);
    std::fs::write(local.path().join("a"), b"\xa4\xa2\n").unwrap();
    std::fs::write(remote.path().join("a"), b"\xa4\xa4\r\n").unwrap();
    let content_file = local.path().join("content.toml");
    std::fs::write(
        &content_file,
        "[local]\nroot_dir='.'\n[servers.develop]\nhost='example.invalid'\nuser='unused'\nroot_dir='.'\n[content.defaults]\nlocal='euc-jp'\ndevelop='euc-jp'\n",
    )
    .unwrap();
    config.content = remote_merge_config::load_config_from_paths(Some(&content_file), None)
        .unwrap()
        .content;
    let output = merge_files(
        merge_args(&["a"]),
        config.clone(),
        targets(&remote, &backup),
    );
    assert_eq!(output.merged.len(), 1);
    assert_eq!(
        std::fs::read(remote.path().join("a")).unwrap(),
        b"\xa4\xa2\n"
    );
    std::fs::write(
        &content_file,
        "[local]\nroot_dir='.'\n[servers.develop]\nhost='example.invalid'\nuser='unused'\nroot_dir='.'\n[content.defaults]\nlocal='utf-8'\ndevelop='utf-8'\n",
    )
    .unwrap();
    config.content = remote_merge_config::load_config_from_paths(Some(&content_file), None)
        .unwrap()
        .content;
    let (output, code) = restore(rollback_args("develop"), config, targets(&remote, &backup));
    assert_eq!(code, 0);
    assert_eq!(output.restored.len(), 1);
    assert_eq!(
        std::fs::read(remote.path().join("a")).unwrap(),
        b"\xa4\xa4\r\n"
    );
}

// @kotowari[REQ-content-004, EX-content-006]
#[tokio::test(flavor = "multi_thread")]
async fn ex_content_006_alias_byte_change_is_detected_by_the_final_raw_recheck() {
    use super::super::agent_support::{AgentFixture, Setup, json};
    use super::super::server_faults::Faults;
    use super::super::server_removal::{Rewrite, RewriteKind};
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("a", Some("new\n"), Some("old\n"));
    std::fs::write(fixture.remote().join("a"), b"\x87\x54\n").unwrap();
    fixture.server.faults().rewrite.arm(Rewrite {
        agent_request: "HashContents",
        occurrence: 2,
        path: fixture.remote().join("a"),
        change: RewriteKind::SameSizeContent(b"\xfa\x4a\n".to_vec()),
    });
    let output = fixture.run(&[
        "merge",
        "a",
        "--hunks",
        "0",
        "--left-encoding",
        "cp932",
        "--right-encoding",
        "cp932",
    ]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(json(&output)["failed"][0]["path"], "a");
    assert_eq!(
        std::fs::read(fixture.remote().join("a")).unwrap(),
        b"\xfa\x4a\n"
    );
}
