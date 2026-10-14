//! 書き込みの中継（writer_relay_loop）が、セッションの終わりに相手が先に閉じたことを利用者への
//! 警告にしないことの単体テスト。終わり方の順は実行ごとに違うので、警告にすると標準エラーが実行ごとに
//! 変わる
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use super::{RELAY_READ_TIMEOUT_MS, writer_relay_loop};

/// 警告以上の記録を数える記録先
struct CountWarnings(Arc<AtomicUsize>);

impl tracing::Subscriber for CountWarnings {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() <= tracing::Level::WARN {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[test]
fn the_relay_ends_without_a_warning_when_the_bridge_closed_first() {
    let (relay_read, mut relay_write) = UnixStream::pair().unwrap();
    relay_read
        .set_read_timeout(Some(Duration::from_millis(RELAY_READ_TIMEOUT_MS)))
        .unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    // 橋渡しの側が先に終わって受け手を閉じた後に、手元が最後のバイト列を書く
    drop(rx);
    relay_write.write_all(b"last request").unwrap();
    relay_write.shutdown(std::net::Shutdown::Write).unwrap();
    let warnings = Arc::new(AtomicUsize::new(0));
    let subscriber = CountWarnings(Arc::clone(&warnings));

    tracing::subscriber::with_default(subscriber, || {
        writer_relay_loop(relay_read, &tx, &Arc::new(AtomicBool::new(false)));
    });

    assert_eq!(warnings.load(Ordering::SeqCst), 0);
}
