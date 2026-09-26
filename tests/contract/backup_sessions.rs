#![cfg(unix)]
//! 書き込み先の区別、セッション ID、期限切れの整理（docs/ir/backup/sessions.md）の契約テスト。

use std::fs;
use std::os::unix::fs::symlink;

use chrono::{DateTime, Duration, TimeZone, Utc};
use proptest::prelude::*;
use remote_merge::backup::{compare_session_ids, next_session_id};
use remote_merge::cli::merge::execute_merge;
use remote_merge::cli::rollback::execute_rollback;
use remote_merge::cli::sync::{execute_sync, SyncArgs, SyncCommandOutput};
use remote_merge::config::AppConfig;
use remote_merge::runtime::bootstrap::{bootstrap_tui_with_targets, TuiBootstrapParams};
use remote_merge::runtime::RuntimeTargets;
use remote_merge::service::output::{format_backup_list_text, format_json};
use tempfile::TempDir;

use super::backup_support::{
    config, config_with_servers, fixed_now, listed_sessions, merge_args, merge_files, restore,
    rollback_args, rollback_list_args, targets, targets_at,
};

// ── 書き込み先の区別 ──

/// 設定上の書き込み先。サーバ名 first と second に使う。
struct Server<'a> {
    host: &'a str,
    user: &'a str,
    port: u16,
    root: &'a str,
}

/// first と second を同じ一時ディレクトリに差し替え、first へ書いたセッションが
/// second の一覧に出るかを返す。区別は設定の値だけで決まる。
fn first_session_is_listed_for_second(first: Server<'_>, second: Server<'_>) -> bool {
    let local = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(destination.path().join("file.txt"), "old\n").unwrap();
    let mut text = format!(
        "[local]\nroot_dir = {:?}\n",
        local.path().display().to_string()
    );
    for (name, server) in [("first", &first), ("second", &second)] {
        text.push_str(&format!(
            "[servers.{name}]\nhost = {:?}\nport = {}\nuser = {:?}\nroot_dir = {:?}\n",
            server.host,
            server.port,
            server.user,
            format!("/srv/{}", server.root)
        ));
    }
    text.push_str("[backup]\nenabled = true\n");
    let path = local.path().join("config.toml");
    fs::write(&path, text).unwrap();
    let config = remote_merge::config::load_config_from_paths(Some(&path), None).unwrap();
    let runtime_targets = RuntimeTargets::production()
        .with_local("first", destination.path())
        .with_local("second", destination.path())
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_now(fixed_now());
    let mut args = merge_args("file.txt");
    args.right = Some("first".into());
    let output = merge_files(args, config.clone(), runtime_targets.clone());
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(
        listed_sessions("first", config.clone(), runtime_targets.clone()).len(),
        1
    );
    !listed_sessions("second", config, runtime_targets).is_empty()
}

fn server(
    host: &'static str,
    user: &'static str,
    port: u16,
    root: &'static str,
) -> Server<'static> {
    Server {
        host,
        user,
        port,
        root,
    }
}

// @kotowari[REQ-backup-021]
#[test]
fn remote_targets_differing_only_in_port_keep_separate_sessions() {
    assert!(!first_session_is_listed_for_second(
        server("example.invalid", "deploy", 22, "app"),
        server("example.invalid", "deploy", 2222, "app"),
    ));
}

// @kotowari[REQ-backup-021]
#[test]
fn remote_targets_differing_only_in_host_keep_separate_sessions() {
    assert!(!first_session_is_listed_for_second(
        server("app1.example.invalid", "deploy", 22, "app"),
        server("app2.example.invalid", "deploy", 22, "app"),
    ));
}

// @kotowari[REQ-backup-021]
#[test]
fn aliases_of_one_host_are_separate_write_targets() {
    assert!(!first_session_is_listed_for_second(
        server("127.0.0.1", "deploy", 22, "app"),
        server("localhost", "deploy", 22, "app"),
    ));
}

// @kotowari[REQ-backup-021]
#[test]
fn remote_targets_differing_only_in_root_dir_keep_separate_sessions() {
    assert!(!first_session_is_listed_for_second(
        server("example.invalid", "deploy", 22, "app"),
        server("example.invalid", "deploy", 22, "other"),
    ));
}

// @kotowari[REQ-backup-021]
#[test]
fn login_user_does_not_distinguish_remote_targets() {
    assert!(first_session_is_listed_for_second(
        server("example.invalid", "alice", 22, "app"),
        server("example.invalid", "bob", 22, "app"),
    ));
}

// @kotowari[REQ-backup-021]
#[test]
fn rollback_target_uses_only_the_write_target_its_name_now_points_to() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "new\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let before = config_with_servers(
        &local,
        &[("develop", "old.example.invalid", 22, develop.path())],
        true,
    );
    merge_files(merge_args("file.txt"), before, targets(&develop, &store));
    let after = config_with_servers(
        &local,
        &[("develop", "new.example.invalid", 22, develop.path())],
        true,
    );

    assert!(listed_sessions("develop", after.clone(), targets(&develop, &store)).is_empty());
    let error = execute_rollback(
        rollback_args("develop", None),
        after,
        targets(&develop, &store),
    )
    .err()
    .expect("no session belongs to the target the name now points to");
    assert!(
        error.to_string().contains("No backup sessions found"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "new\n"
    );
}

// @kotowari[REQ-backup-021]
#[test]
fn sessions_for_two_write_targets_remain_separate() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = config_with_servers(
        &local,
        &[
            ("develop", "develop.invalid", 22, develop.path()),
            ("staging", "staging.invalid", 22, staging.path()),
        ],
        true,
    );
    let runtime_targets = targets(&develop, &store).with_local("staging", staging.path());
    fs::write(local.path().join("develop.txt"), "new develop\n").unwrap();
    fs::write(develop.path().join("develop.txt"), "old\n").unwrap();
    fs::write(local.path().join("staging.txt"), "new staging\n").unwrap();
    fs::write(staging.path().join("staging.txt"), "old\n").unwrap();
    merge_files(
        merge_args("develop.txt"),
        config.clone(),
        runtime_targets.clone(),
    );
    let mut staging_args = merge_args("staging.txt");
    staging_args.right = Some("staging".into());
    merge_files(staging_args, config.clone(), runtime_targets.clone());

    let develop_sessions = listed_sessions("develop", config.clone(), runtime_targets.clone());
    let staging_sessions = listed_sessions("staging", config, runtime_targets);
    assert_eq!(develop_sessions.len(), 1, "{develop_sessions:?}");
    assert_eq!(develop_sessions[0].files.len(), 1);
    assert_eq!(develop_sessions[0].files[0].path, "develop.txt");
    assert_eq!(staging_sessions.len(), 1, "{staging_sessions:?}");
    assert_eq!(staging_sessions[0].files.len(), 1);
    assert_eq!(staging_sessions[0].files[0].path, "staging.txt");
}

// @kotowari[REQ-backup-021]
#[test]
fn relative_local_root_is_identified_from_the_directory_the_config_was_loaded_in() {
    // 相対の root_dir でローカルへ書き込むため、ビルド先の場所に左右されないよう
    // 作業ディレクトリの直下に一時ディレクトリを作る
    let startup = std::env::current_dir().unwrap();
    let base = TempDir::new_in(&startup).unwrap();
    let relative_root = std::path::Path::new(base.path().file_name().unwrap()).join("project");
    fs::create_dir(base.path().join("project")).unwrap();
    let other_startup = base.path().join("elsewhere");
    fs::create_dir(&other_startup).unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(base.path().join("project/file.txt"), "old local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "new remote\n").unwrap();
    let config_path = base.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"develop.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
            relative_root.display().to_string(),
            develop.path().display().to_string()
        ),
    )
    .unwrap();
    let config = remote_merge::config::load_config_from_paths(Some(&config_path), None).unwrap();
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("local".into());
    let output = merge_files(args, config.clone(), targets(&develop, &store));
    assert_eq!(output.merged.len(), 1, "{output:?}");
    assert_eq!(
        fs::read_to_string(base.path().join("project/file.txt")).unwrap(),
        "new remote\n"
    );

    let from_startup = targets(&develop, &store).with_startup_directory(startup);
    let from_elsewhere = targets(&develop, &store).with_startup_directory(other_startup);
    assert_eq!(
        listed_sessions("local", config.clone(), from_startup).len(),
        1
    );
    assert!(listed_sessions("local", config, from_elsewhere).is_empty());
}

// @kotowari[REQ-backup-021]
#[test]
fn local_root_symlink_retargeting_keeps_existing_sessions_visible() {
    let base = TempDir::new().unwrap();
    let release_a = base.path().join("release-a");
    let release_b = base.path().join("release-b");
    let local_link = base.path().join("current");
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::create_dir(&release_a).unwrap();
    fs::create_dir(&release_b).unwrap();
    symlink(&release_a, &local_link).unwrap();
    fs::write(release_a.join("file.txt"), "old local\n").unwrap();
    fs::write(develop.path().join("file.txt"), "new remote\n").unwrap();
    let config_path = base.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[local]\nroot_dir = \"{}\"\n[servers.develop]\nhost = \"develop.invalid\"\nuser = \"unused\"\nroot_dir = \"{}\"\n[backup]\nenabled = true\n",
            local_link.display(),
            develop.path().display()
        ),
    )
    .unwrap();
    let config = remote_merge::config::load_config_from_paths(Some(&config_path), None).unwrap();
    let runtime_targets = targets(&develop, &store);
    let mut args = merge_args("file.txt");
    args.left = Some("develop".into());
    args.right = Some("local".into());
    execute_merge(args, config.clone(), runtime_targets.clone()).unwrap();
    fs::remove_file(&local_link).unwrap();
    symlink(&release_b, &local_link).unwrap();

    assert_eq!(listed_sessions("local", config, runtime_targets).len(), 1);
}

// ── セッション ID ──

/// 2000 年から 2099 年までの UTC の秒単位の時刻。
fn session_time() -> impl Strategy<Value = DateTime<Utc>> {
    (946_684_800i64..4_102_444_800).prop_map(|seconds| Utc.timestamp_opt(seconds, 0).unwrap())
}

/// "-N" の N。1 は接尾辞なしを表す。一秒の間に作られうる数として 1000 までに限る。
fn sequence() -> impl Strategy<Value = u64> {
    prop_oneof![Just(1u64), 2u64..=1000]
}

/// ある時刻からの秒の差と N。既存の ID を作る時刻の近くに寄せるために使う。
fn near_session_id_parts() -> impl Strategy<Value = (i64, u64)> {
    (-1i64..=1, sequence())
}

fn session_id(time: DateTime<Utc>, sequence: u64) -> String {
    let timestamp = time.format("%Y%m%d-%H%M%S").to_string();
    match sequence {
        1 => timestamp,
        sequence => format!("{timestamp}-{sequence}"),
    }
}

// @kotowari[REQ-backup-022]
#[test]
fn created_session_ids_follow_the_timestamp_and_suffix_format() {
    proptest!(|(
    now in session_time(),
    existing in prop::collection::vec(near_session_id_parts(), 0..8),
    )| {
        let existing = existing
            .into_iter()
            .map(|(offset, sequence)| session_id(now + Duration::seconds(offset), sequence))
            .collect::<Vec<_>>();
        let refs = existing.iter().map(String::as_str).collect::<Vec<_>>();

        let id = next_session_id(now, &refs);

        let timestamp = now.format("%Y%m%d-%H%M%S").to_string();
        prop_assert!(id.starts_with(&timestamp), "{id} does not start with {timestamp}");
        let same_time_exists = existing.iter().any(|other| other.get(..15) == Some(timestamp.as_str()));
        match id.strip_prefix(&timestamp).unwrap() {
            "" => prop_assert!(!same_time_exists, "{id} has no suffix despite {existing:?}"),
            suffix => {
                prop_assert!(same_time_exists, "{id} has a suffix without an ID at {timestamp}");
                let sequence = suffix
                    .strip_prefix('-')
                    .and_then(|number| number.parse::<u64>().ok());
                prop_assert!(matches!(sequence, Some(n) if n >= 2), "bad suffix in {id}");
            }
        }
    });
}

// @kotowari[REQ-backup-022]
#[test]
fn session_ids_order_by_time_then_numeric_suffix() {
    proptest!(|(
    left in (session_time(), sequence()),
    // 同じ日時や一秒違いの組が出やすいよう、半分は近くの時刻にする
    offset in prop_oneof![-1i64..=1, -400_000_000i64..=400_000_000],
    right_sequence in sequence(),
    )| {
        let right = (left.0 + Duration::seconds(offset), right_sequence);
        let expected = left.cmp(&right);

        prop_assert_eq!(
            compare_session_ids(&session_id(left.0, left.1), &session_id(right.0, right.1)),
            Some(expected)
        );
    });
}

// @kotowari[REQ-backup-023]
#[test]
fn next_session_id_differs_from_every_existing_id() {
    proptest!(|(
    now in session_time(),
    existing in prop::collection::vec(near_session_id_parts(), 0..16),
    )| {
        let existing = existing
            .into_iter()
            .map(|(offset, sequence)| session_id(now + Duration::seconds(offset), sequence))
            .collect::<Vec<_>>();
        let refs = existing.iter().map(String::as_str).collect::<Vec<_>>();

        let id = next_session_id(now, &refs);

        prop_assert!(!existing.contains(&id), "{id} repeats one of {existing:?}");
    });
}

// @kotowari[REQ-backup-022]
#[test]
fn tenth_session_sorts_after_ninth_session() {
    assert_eq!(
        compare_session_ids("20260914-155250-10", "20260914-155250-9"),
        Some(std::cmp::Ordering::Greater)
    );
}

// @kotowari[REQ-backup-022, REQ-backup-023]
#[test]
fn same_time_uses_second_session_suffix() {
    let now = Utc.with_ymd_and_hms(2026, 9, 14, 15, 52, 50).unwrap();
    let first = next_session_id(now, &[]);
    let second = next_session_id(now, &[first.as_str()]);

    assert_eq!(first, "20260914-155250");
    assert_eq!(second, "20260914-155250-2");
}

fn create_two_same_second_sessions(
    local: &TempDir,
    develop: &TempDir,
    store: &TempDir,
) -> (AppConfig, RuntimeTargets) {
    let config = config(local, develop, true);
    let runtime_targets = targets(develop, store);
    fs::write(local.path().join("file.txt"), "first merge\n").unwrap();
    fs::write(develop.path().join("file.txt"), "original\n").unwrap();
    let mut args = merge_args("file.txt");
    args.force = true;
    merge_files(args, config.clone(), runtime_targets.clone());
    fs::write(local.path().join("file.txt"), "second merge content\n").unwrap();
    let mut args = merge_args("file.txt");
    args.force = true;
    merge_files(args, config.clone(), runtime_targets.clone());
    (config, runtime_targets)
}

// @kotowari[REQ-backup-022, REQ-backup-040]
#[test]
fn same_second_sessions_are_listed_newest_first() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let (config, runtime_targets) = create_two_same_second_sessions(&local, &develop, &store);

    let sessions = listed_sessions("develop", config, runtime_targets);

    let ids = sessions
        .iter()
        .map(|session| session.session_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["20260914-120000-2", "20260914-120000"]);
}

// @kotowari[REQ-backup-022]
#[test]
fn rollback_accepts_a_same_second_session_id_with_numeric_suffix() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let (mut config, runtime_targets) = create_two_same_second_sessions(&local, &develop, &store);
    config.backup.enabled = false;

    let (output, _) = restore(
        rollback_args("develop", Some("20260914-120000-2")),
        config,
        runtime_targets,
    );

    assert_eq!(output.session_id, "20260914-120000-2");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "first merge\n"
    );
}

// @kotowari[REQ-backup-022]
#[test]
fn rollback_without_session_uses_the_newest_numeric_suffix() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let (mut config, runtime_targets) = create_two_same_second_sessions(&local, &develop, &store);
    config.backup.enabled = false;

    let (output, _) = restore(rollback_args("develop", None), config, runtime_targets);

    assert_eq!(output.session_id, "20260914-120000-2");
    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "first merge\n"
    );
}

fn merged_session(output: remote_merge::service::types::MergeOutput) -> String {
    let backup = output.merged[0].backup.clone().unwrap();
    backup.split('/').next().unwrap().to_owned()
}

// @kotowari[REQ-backup-023]
#[test]
fn merges_started_in_the_same_second_use_distinct_session_ids() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(local.path().join("file.txt"), "first content\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let first = merge_files(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    );
    fs::write(
        local.path().join("file.txt"),
        "second content that differs\n",
    )
    .unwrap();
    let second = merge_files(
        merge_args("file.txt"),
        config(&local, &develop, true),
        targets(&develop, &store),
    );

    assert_ne!(merged_session(first), merged_session(second));
}

// @kotowari[REQ-backup-023]
#[test]
fn concurrent_merges_use_distinct_session_ids() {
    let store = TempDir::new().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for index in 0..2 {
        let local = TempDir::new().unwrap();
        let develop = TempDir::new().unwrap();
        fs::write(
            local.path().join("file.txt"),
            format!("new content {index}\n"),
        )
        .unwrap();
        fs::write(develop.path().join("file.txt"), "old\n").unwrap();
        let config = config(&local, &develop, true);
        let targets = targets(&develop, &store);
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let _guards = (local, develop);
            barrier.wait();
            merged_session(merge_files(merge_args("file.txt"), config, targets))
        }));
    }
    let sessions = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_ne!(sessions[0], sessions[1]);
}

/// 同じ集約先と同じ時刻で、merge を threads 本同時に始める。
/// どれかがエラーで終わればパニックし、成功したもののセッション ID を返す。
fn simultaneous_merge_sessions(threads: usize) -> Vec<String> {
    let store = std::sync::Arc::new(TempDir::new().unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(threads));
    let handles = (0..threads)
        .map(|index| {
            let local = TempDir::new().unwrap();
            let develop = TempDir::new().unwrap();
            fs::write(
                local.path().join("file.txt"),
                format!("new content {index}\n"),
            )
            .unwrap();
            fs::write(develop.path().join("file.txt"), "old\n").unwrap();
            let config = config(&local, &develop, true);
            let targets = targets(&develop, &store);
            let barrier = barrier.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                let _guards = (local, develop, store);
                barrier.wait();
                merged_session(merge_files(merge_args("file.txt"), config, targets))
            })
        })
        .collect::<Vec<_>>();
    handles
        .into_iter()
        .map(|handle| handle.join().expect("a simultaneous merge failed"))
        .collect()
}

// @kotowari[REQ-backup-023]
#[test]
fn many_simultaneous_merges_all_succeed_with_distinct_session_ids() {
    for _ in 0..5 {
        let mut sessions = simultaneous_merge_sessions(16);

        sessions.sort();
        sessions.dedup();
        assert_eq!(sessions.len(), 16, "{sessions:?}");
    }
}

// @kotowari[REQ-backup-024]
#[test]
fn sync_uses_one_session_id_for_all_targets() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(
        local.path().join("file.txt"),
        "new content shared by sync\n",
    )
    .unwrap();
    fs::write(develop.path().join("file.txt"), "old develop\n").unwrap();
    fs::write(staging.path().join("file.txt"), "old staging\n").unwrap();
    let config = config_with_servers(
        &local,
        &[
            ("develop", "develop.invalid", 22, develop.path()),
            ("staging", "staging.invalid", 22, staging.path()),
        ],
        true,
    );
    let runtime_targets = targets(&develop, &store).with_local("staging", staging.path());
    let mut args = sync_args("file.txt");
    args.right = vec!["develop".into(), "staging".into()];

    let SyncCommandOutput::Result(output) =
        execute_sync(args, config.clone(), runtime_targets.clone())
            .unwrap()
            .output
    else {
        panic!("expected result")
    };

    let sessions = output
        .targets
        .iter()
        .map(|target| {
            target.merged[0]
                .backup
                .as_deref()
                .unwrap()
                .split('/')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0], sessions[1]);
    for target in ["develop", "staging"] {
        let listed = listed_sessions(target, config.clone(), runtime_targets.clone());
        assert_eq!(listed.len(), 1, "{target}: {listed:?}");
        assert_eq!(listed[0].session_id, sessions[0], "{target}");
    }
}

// ── 期限切れの整理 ──

fn at(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

fn sync_args(path: &str) -> SyncArgs {
    SyncArgs {
        paths: vec![path.into()],
        left: Some("local".into()),
        right: vec!["develop".into()],
        dry_run: false,
        force: true,
        delete: false,
        with_permissions: false,
        checksum: false,
        format: "json".into(),
        max_entries: None,
    }
}

/// 2020-01-01 のセッションを一つ作る。保持期間は既定の 7 日。
fn expired_session_fixture(local: &TempDir, develop: &TempDir, store: &TempDir) -> AppConfig {
    fs::write(local.path().join("file.txt"), "first\n").unwrap();
    fs::write(develop.path().join("file.txt"), "old\n").unwrap();
    let config = config(local, develop, true);
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets_at(develop, store, at(2020, 1, 1)),
    );
    fs::write(local.path().join("file.txt"), "second\n").unwrap();
    config
}

fn listed_ids(config: AppConfig, develop: &TempDir, store: &TempDir) -> Vec<String> {
    listed_sessions("develop", config, targets(develop, store))
        .into_iter()
        .map(|session| session.session_id)
        .collect()
}

// @kotowari[REQ-backup-025]
#[test]
fn merge_removes_expired_sessions_when_it_starts() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);

    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    );

    assert_eq!(listed_ids(config, &develop, &store), ["20200108-000000"]);
}

// @kotowari[REQ-backup-025]
#[test]
fn sync_removes_expired_sessions_when_it_starts() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);

    execute_sync(
        sync_args("file.txt"),
        config.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    )
    .unwrap();

    assert_eq!(listed_ids(config, &develop, &store), ["20200108-000000"]);
}

// @kotowari[REQ-backup-025]
#[test]
fn dry_run_merge_and_sync_keep_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);
    let mut merge = merge_args("file.txt");
    merge.dry_run = true;
    let mut sync = sync_args("file.txt");
    sync.dry_run = true;

    execute_merge(
        merge,
        config.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    )
    .unwrap();
    execute_sync(
        sync,
        config.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    )
    .unwrap();

    assert_eq!(listed_ids(config, &develop, &store), ["20200101-000000"]);
}

// @kotowari[REQ-backup-025]
#[test]
fn rollback_in_any_mode_keeps_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);
    let current = targets_at(&develop, &store, at(2020, 1, 8));

    execute_rollback(
        rollback_list_args("develop"),
        config.clone(),
        current.clone(),
    )
    .unwrap();
    let mut dry_run = rollback_args("develop", Some("20200101-000000"));
    dry_run.dry_run = true;
    execute_rollback(dry_run, config.clone(), current.clone()).unwrap();
    restore(
        rollback_args("develop", Some("20200101-000000")),
        config.clone(),
        current,
    );

    assert_eq!(
        fs::read_to_string(develop.path().join("file.txt")).unwrap(),
        "old\n"
    );
    let ids = listed_ids(config, &develop, &store);
    assert!(ids.contains(&"20200101-000000".to_string()), "{ids:?}");
}

// @kotowari[REQ-backup-025]
#[test]
fn disabled_backup_merge_still_removes_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    expired_session_fixture(&local, &develop, &store);
    let disabled = config(&local, &develop, false);

    merge_files(
        merge_args("file.txt"),
        disabled.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    );

    assert!(listed_ids(disabled, &develop, &store).is_empty());
}

// @kotowari[REQ-backup-025]
#[test]
fn tui_start_removes_expired_sessions() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);

    let (_, runtime) = bootstrap_tui_with_targets(
        TuiBootstrapParams {
            right_server: "develop".into(),
            left_server: None,
            ref_server: None,
        },
        config.clone(),
        targets_at(&develop, &store, at(2020, 1, 8)),
    )
    .unwrap();
    drop(runtime);

    assert!(listed_ids(config, &develop, &store).is_empty());
}

// @kotowari[REQ-backup-026]
#[test]
fn listing_and_cleanup_share_the_retention_boundary() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);
    let boundary = at(2020, 1, 1) + Duration::days(7);
    let before = boundary - Duration::seconds(1);

    let listed = listed_sessions(
        "develop",
        config.clone(),
        targets_at(&develop, &store, before),
    );
    assert!(!listed[0].expired, "{listed:?}");
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets_at(&develop, &store, before),
    );
    assert!(listed_ids(config.clone(), &develop, &store).contains(&"20200101-000000".into()));

    let listed = listed_sessions(
        "develop",
        config.clone(),
        targets_at(&develop, &store, boundary),
    );
    let original = listed
        .iter()
        .find(|session| session.session_id == "20200101-000000")
        .unwrap();
    assert!(original.expired, "{listed:?}");
    fs::write(local.path().join("file.txt"), "third\n").unwrap();
    merge_files(
        merge_args("file.txt"),
        config.clone(),
        targets_at(&develop, &store, boundary),
    );
    assert!(!listed_ids(config, &develop, &store).contains(&"20200101-000000".into()));
}

// @kotowari[REQ-backup-026, REQ-backup-040]
#[test]
fn expired_session_is_marked_in_text_and_json_at_the_injected_boundary() {
    let local = TempDir::new().unwrap();
    let develop = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let config = expired_session_fixture(&local, &develop, &store);
    let list_targets = RuntimeTargets::production()
        .with_backup_store(Some(store.path().to_path_buf()))
        .with_startup_directory(std::env::current_dir().unwrap())
        .with_now(at(2020, 1, 8));

    let listed = execute_rollback(rollback_list_args("develop"), config, list_targets).unwrap();

    let remote_merge::cli::rollback::RollbackCommandOutput::List(output) = listed.output else {
        panic!("expected backup list")
    };
    assert!(output.sessions[0].expired);
    assert!(format_backup_list_text(&output).contains("[expired]"));
    let json: serde_json::Value = serde_json::from_str(&format_json(&output).unwrap()).unwrap();
    assert_eq!(json["sessions"][0]["expired"], true);
}
