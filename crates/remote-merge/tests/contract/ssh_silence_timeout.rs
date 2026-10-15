#![cfg(unix)]
//! リモートが黙っていてよい時間の時間切れ（docs/ir/ssh/reconnect.md の REQ-ssh-025）の契約テスト。

// 短い timeout_sec でも、黙っていてよい時間の下限（120 秒）を当てる
// @kotowari[REQ-ssh-025]
#[test]
fn a_short_timeout_still_gets_the_floor_for_silence() {
    assert_eq!(
        remote_merge_ssh::silence_timeout(3),
        std::time::Duration::from_secs(120)
    );
    assert_eq!(
        remote_merge_ssh::silence_timeout(300),
        std::time::Duration::from_secs(300)
    );
}
