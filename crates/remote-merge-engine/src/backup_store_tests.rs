//! バックアップの集約先の単体テスト
use super::*;

// 何も保存しなかったセッションを終えても、同時に動く別の実行が使う集約先の場所（予約の場所）は
// 消さない。消すと、予約の場所を作った後で自分の予約を作る前の別の実行が、場所がないとして止まる
// （PROJECT.md の Security boundary の「同時に動く別の作業」で黙って失敗させない）
#[test]
fn finishing_a_session_that_saved_nothing_keeps_the_place_other_runs_reserve_in() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let now = Utc::now();
    let first = BackupStore::new(Some(root.clone()), temp.path().to_path_buf(), now);
    let id = first.reserve_session().unwrap();

    first.finish_session(&id);

    // 別の実行は、予約の場所を作った後に、その中を一覧して自分の予約を作る
    let reservations = root.join("reservations");
    assert!(fs::read_dir(&reservations).is_ok(), "{reservations:?}");
    assert!(!reservations.join(&id).exists());
    assert!(!root.join("sessions").join(&id).exists());
}

/// root_dir を `root` にしたローカルだけの設定（保持は 1 日）
fn config_with_root(temp: &Path, root: &Path) -> AppConfig {
    let path = temp.join("config.toml");
    fs::write(
        &path,
        format!(
            "[local]\nroot_dir = {:?}\n[backup]\nenabled = true\nretention_days = 1\n",
            root.display().to_string()
        ),
    )
    .unwrap();
    remote_merge_config::load_config_from_paths(Some(&path), None).unwrap()
}

/// `store` の時刻に、ローカルの書き込み先へ一つのファイルを保存して終えたセッションの ID
fn saved_session(store: &BackupStore, config: &AppConfig, temp: &Path) -> String {
    let id = store.reserve_session().unwrap();
    store.prepare_target(config, &Side::Local, &id).unwrap();
    store
        .save_file(
            config,
            &Side::Local,
            &id,
            "a.txt",
            &temp.join("a.txt"),
            b"old",
        )
        .unwrap();
    store.finish_session(&id);
    id
}

// 保存したセッションの予約は終えても残るので、期限切れのセッションを消すときに一緒に消さないと、
// 予約の場所に溜まり続け、予約のたびにその全部を読むことになる
#[test]
fn removing_an_expired_session_also_removes_its_reservation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let config = config_with_root(temp.path(), temp.path());
    let two_days_ago = Utc::now() - chrono::Duration::days(2);
    let old = BackupStore::new(Some(root.clone()), temp.path().to_path_buf(), two_days_ago);
    let id = saved_session(&old, &config, temp.path());
    let reservation = root.join("reservations").join(&id);
    assert!(reservation.exists(), "{reservation:?}");
    let store = BackupStore::new(Some(root.clone()), temp.path().to_path_buf(), Utc::now());

    assert_eq!(store.cleanup_expired(&config).unwrap(), 1);

    assert!(!root.join("sessions").join(&id).exists());
    assert!(!reservation.exists(), "{reservation:?}");
}

// 設定にない書き込み先の領域が残って消えなかったセッションは、予約も残す。予約を消すと、同じ ID を
// 別の実行に割り当てられるようになり、残ったセッションと ID が重なりうる
#[test]
fn an_expired_session_left_for_an_unconfigured_target_keeps_its_reservation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let config = config_with_root(temp.path(), temp.path());
    let two_days_ago = Utc::now() - chrono::Duration::days(2);
    let old = BackupStore::new(Some(root.clone()), temp.path().to_path_buf(), two_days_ago);
    let id = saved_session(&old, &config, temp.path());
    let unconfigured = root.join("sessions").join(&id).join("unconfigured");
    fs::create_dir_all(&unconfigured).unwrap();
    let store = BackupStore::new(Some(root.clone()), temp.path().to_path_buf(), Utc::now());

    assert_eq!(store.cleanup_expired(&config).unwrap(), 1);

    assert!(unconfigured.exists());
    assert!(root.join("reservations").join(&id).exists());
}

// 同時に動く別の merge・sync が同じ期限切れのセッションを先に消していても、消すものがないとして
// 誤りにしない（PROJECT.md の Security boundary の「同時に動く別の作業」）。先に消されたセッションは、
// 一覧した後に中を読めなくなったセッション（ここでは辿った先のない名前）として起こす
// @kotowari[REQ-backup-025]
#[cfg(unix)]
#[test]
fn an_expired_session_another_run_removed_first_is_not_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let sessions = root.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    std::os::unix::fs::symlink(temp.path().join("gone"), sessions.join("20000101-000000")).unwrap();
    let config = config_with_root(temp.path(), temp.path());
    let store = BackupStore::new(Some(root), temp.path().to_path_buf(), Utc::now());

    assert_eq!(store.cleanup_expired(&config).unwrap(), 0);
}

// 集約先の sessions を読めないときは、黙って 0 件にせず、どこを読めなかったかを示して止める
// @kotowari[REQ-backup-025]
#[cfg(unix)]
#[test]
#[expect(
    clippy::print_stderr,
    reason = "root で走っているために前提を作れなかったことを知らせる"
)]
fn sessions_that_cannot_be_read_are_an_error_and_not_zero_removed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let sessions = root.join("sessions");
    fs::create_dir_all(sessions.join("20000101-000000")).unwrap();
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o000)).unwrap();
    let applies = fs::read_dir(&sessions).is_err();
    let config = config_with_root(temp.path(), temp.path());
    let store = BackupStore::new(Some(root), temp.path().to_path_buf(), Utc::now());

    let result = store.cleanup_expired(&config);
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
    if !applies {
        eprintln!("note: running as root; an unreadable directory cannot be made");
        return;
    }

    let error = result.expect_err("an unreadable store must not be read as nothing to remove");
    assert!(
        error.to_string().contains(&sessions.display().to_string()),
        "{error:#}"
    );
}
