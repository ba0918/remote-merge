//! クリップボードへの書き込みユーティリティ（I/O層）。
//!
//! arboard を使ったクリップボード書き込み時に発生する問題を吸収する:
//! - WSL/Linux 環境で Clipboard インスタンスが即座に drop されると
//!   クリップボードマネージャーが内容を受け取れない問題 → sleep で対処
//! - arboard が stderr に直接警告を出力して TUI を汚す問題 → stderr 抑制で対処
//! - arboard のエラー自体は tracing::warn! で debug.log に記録

use std::time::Duration;

/// クリップボード書き込みの待機時間（ミリ秒）。
/// クリップボードマネージャーが内容を受け取るのに十分な時間を確保する。
const CLIPBOARD_SETTLE_MS: u64 = 100;

/// クリップボード操作の結果。
pub enum ClipboardResult {
    /// 書き込み成功
    Ok,
    /// 書き込み失敗（ユーザー向けメッセージ付き）
    WriteError(String),
    /// クリップボード自体が利用不可（ユーザー向けメッセージ付き）
    Unavailable(String),
}

/// テキストをクリップボードに書き込む。
///
/// arboard の stderr 警告を抑制し、書き込み後にクリップボードマネージャーが
/// 内容を受け取る時間を確保してから Clipboard インスタンスを drop する。
/// エラー発生時は tracing::warn! で debug.log に記録する。
pub fn write_to_clipboard(text: &str) -> ClipboardResult {
    // arboard が stderr に警告を直接出力するため、TUI が壊れないよう一時的に抑制する
    let _stderr_guard = StderrGuard::suppress();

    let result = (|| -> Result<(), (ClipboardResult, String)> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| {
            let log_msg = format!("arboard::Clipboard::new() failed: {e}");
            let user_msg = format!("Clipboard not available: {e}");
            (ClipboardResult::Unavailable(user_msg), log_msg)
        })?;

        clipboard.set_text(text).map_err(|e| {
            let log_msg = format!("arboard set_text failed: {e}");
            let user_msg = format!("Clipboard write failed: {e}");
            (ClipboardResult::WriteError(user_msg), log_msg)
        })?;

        // クリップボードマネージャーが内容を受け取る時間を確保
        std::thread::sleep(Duration::from_millis(CLIPBOARD_SETTLE_MS));

        Ok(())
    })();

    // _stderr_guard が drop されて stderr が復元される

    match result {
        Ok(()) => ClipboardResult::Ok,
        Err((clipboard_result, log_msg)) => {
            tracing::warn!("{log_msg}");
            clipboard_result
        }
    }
}

// --- stderr 抑制ユーティリティ ---

/// stderr を一時的に /dev/null にリダイレクトするガード。
/// drop 時に自動的に元の stderr に復元される。
/// 新規の crate 依存を避けるため `std::os::unix` の raw fd API を使用。
struct StderrGuard {
    #[cfg(unix)]
    saved_fd: Option<std::os::unix::io::OwnedFd>,
}

impl StderrGuard {
    /// stderr を /dev/null にリダイレクトし、復元用ガードを返す。
    /// Unix 以外の環境ではノーオペ（stderr はそのまま）。
    fn suppress() -> Self {
        #[cfg(unix)]
        {
            Self::suppress_unix()
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    #[cfg(unix)]
    fn suppress_unix() -> Self {
        use std::fs::File;
        use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd};

        // SAFETY: fd 2 (stderr) はプロセス起動時から常に有効な fd である。
        let stderr_fd = unsafe { BorrowedFd::borrow_raw(2) };

        // 現在の stderr を複製して保存
        let saved = stderr_fd.try_clone_to_owned().ok();

        if saved.is_some() {
            // /dev/null を開いて stderr に差し替え
            if let Ok(devnull) = File::open("/dev/null") {
                // dup2 相当: devnull の fd を fd 2 に複製
                // std には dup2 がないので nix も libc も使わず、
                // プラットフォーム固有コードとして最小限の unsafe を使用
                // SAFETY: devnull は直前に open 成功した有効な fd。fd 2 (stderr) も有効。
                // dup2 は両方が有効な fd なら安全に呼べる POSIX 関数。
                unsafe {
                    let ret = dup2_raw(devnull.as_fd().as_raw_fd(), 2);
                    if ret < 0 {
                        return Self { saved_fd: None };
                    }
                }
            }
        }

        Self { saved_fd: saved }
    }
}

#[cfg(unix)]
/// POSIX dup2 の最小ラッパー。libc クレート不要。
/// SAFETY: 呼び出し元は oldfd/newfd が有効な fd であることを保証する必要がある。
/// dup2 は POSIX 標準関数であり、有効な fd に対しては安全に動作する。
unsafe fn dup2_raw(oldfd: i32, newfd: i32) -> i32 {
    // libc クレートに依存しない方針のため extern で直接リンクする
    extern "C" {
        fn dup2(oldfd: i32, newfd: i32) -> i32;
    }
    // SAFETY: 呼び出し元が fd の有効性を保証。dup2 は有効な fd 同士であればスレッドセーフ。
    unsafe { dup2(oldfd, newfd) }
}

impl Drop for StderrGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(ref saved) = self.saved_fd {
            use std::os::unix::io::AsRawFd;
            // SAFETY: saved は suppress_unix で dup 成功した有効な fd。fd 2 は常に有効。
            unsafe {
                dup2_raw(saved.as_raw_fd(), 2);
            }
            // saved_fd は OwnedFd なので自動で close される
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_result_variants() {
        let ok = ClipboardResult::Ok;
        assert!(matches!(ok, ClipboardResult::Ok));

        let write_err = ClipboardResult::WriteError("test".to_string());
        assert!(matches!(write_err, ClipboardResult::WriteError(_)));

        let unavail = ClipboardResult::Unavailable("test".to_string());
        assert!(matches!(unavail, ClipboardResult::Unavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn test_suppress_stderr_restores() {
        // stderr 抑制後に復元されることを確認（パニックしないことが主な検証）
        {
            let _guard = StderrGuard::suppress();
            eprintln!("this should be suppressed");
        }
        // guard が drop された後、stderr は復元されている
    }

    // 待機時間の妥当性はコンパイル時に保証
    const _: () = {
        assert!(CLIPBOARD_SETTLE_MS >= 10);
        assert!(CLIPBOARD_SETTLE_MS <= 500);
    };
}
