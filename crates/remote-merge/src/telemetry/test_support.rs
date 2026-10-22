//! telemetry の単体テストが使う、書いたバイトを手元に残す書き込み先

use std::io::Write;
use std::sync::{Arc, Mutex};

/// 書いたバイトを共有の Vec に足す書き込み先（ログの層に渡し、書いたものを後で読む）
pub(super) struct SharedBuf(pub(super) Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
