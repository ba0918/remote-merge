//! SSH チャネル ↔ sync Read/Write ブリッジ。
//!
//! russh の async SSH exec チャネルと AgentClient の sync I/O (UnixStream) を
//! ブリッジスレッドで接続する。
//!
//! チャネルは単一の bridge スレッドが排他的に所有し、`tokio::select!` で
//! 読み書き両方向を多重化する。これにより共有 Mutex によるデッドロックを防ぐ。
//!
//! bridge スレッド: SSH channel を排他所有し select! で双方向中継
//! writer-relay スレッド: UnixStream → mpsc → bridge スレッド

#[cfg(unix)]
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::thread::JoinHandle;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use russh::ChannelMsg;

/// ブリッジスレッドの join タイムアウト（秒）
#[cfg(unix)]
const JOIN_TIMEOUT_SECS: u64 = 2;

/// writer-relay の read タイムアウト（ミリ秒）
///
/// shutdown フラグを定期チェックするためのポーリング間隔。
#[cfg(unix)]
const RELAY_READ_TIMEOUT_MS: u64 = 200;

// ---------------------------------------------------------------------------
// TransportGuard
// ---------------------------------------------------------------------------

/// ブリッジスレッドのライフサイクルを管理するガード。
///
/// `SshAgentTransport::into_streams()` から返され、ストリームとセットで保持する。
/// Drop 時にスレッドを安全にシャットダウン・join する。
#[cfg(unix)]
pub struct TransportGuard {
    bridge_thread: Option<JoinHandle<()>>,
    writer_relay_thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

#[cfg(unix)]
impl Drop for TransportGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);

        let deadline = Instant::now() + Duration::from_secs(JOIN_TIMEOUT_SECS);
        for t in [self.writer_relay_thread.take(), self.bridge_thread.take()]
            .into_iter()
            .flatten()
        {
            while !t.is_finished() {
                if Instant::now() >= deadline {
                    tracing::warn!("Bridge thread did not terminate within timeout");
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SshAgentTransport
// ---------------------------------------------------------------------------

/// SSH exec チャネルと AgentClient の間を中継するトランスポート。
///
/// `start()` でブリッジスレッド（+ writer-relay）を起動し、
/// `into_streams()` で AgentClient に渡す UnixStream ペアと、
/// スレッド管理用の `TransportGuard` を取り出す。
///
/// チャネルは bridge スレッドが排他所有するため、共有ロックによる
/// デッドロックは原理的に発生しない。
#[cfg(unix)]
pub struct SshAgentTransport {
    client_read: Option<UnixStream>,
    client_write: Option<UnixStream>,
    bridge_thread: Option<JoinHandle<()>>,
    writer_relay_thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

#[cfg(unix)]
impl SshAgentTransport {
    /// SSH チャネルとブリッジスレッドを起動する。
    ///
    /// - `handle`: tokio ランタイムハンドル（block_on 用）
    /// - `channel`: SSH exec チャネル（既にコマンド実行済みであること）
    pub fn start(
        handle: tokio::runtime::Handle,
        channel: russh::Channel<russh::client::Msg>,
    ) -> Result<Self> {
        // read 方向: bridge → client_read
        let (client_read, bridge_write) = UnixStream::pair()?;
        // write 方向: client_write → writer-relay → bridge
        let (bridge_read, client_write) = UnixStream::pair()?;

        // writer-relay が shutdown フラグをポーリングできるよう read timeout を設定
        bridge_read.set_read_timeout(Some(Duration::from_millis(RELAY_READ_TIMEOUT_MS)))?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Writer relay: UnixStream から読み取り → mpsc で bridge に送信
        let shutdown2 = shutdown.clone();
        let writer_relay_thread = std::thread::Builder::new()
            .name("agent-write-relay".into())
            .spawn(move || {
                writer_relay_loop(bridge_read, write_tx, shutdown2);
            })?;

        // Bridge: channel を排他所有し、select! で双方向中継
        let shutdown3 = shutdown.clone();
        let bridge_thread = std::thread::Builder::new()
            .name("agent-bridge".into())
            .spawn(move || {
                bridge_loop(handle, channel, bridge_write, write_rx, shutdown3);
            })?;

        Ok(Self {
            client_read: Some(client_read),
            client_write: Some(client_write),
            bridge_thread: Some(bridge_thread),
            writer_relay_thread: Some(writer_relay_thread),
            shutdown,
        })
    }

    /// AgentClient に渡す (read, write) ストリームペアと TransportGuard を取り出す。
    ///
    /// `TransportGuard` はブリッジスレッドのライフサイクルを管理する。
    /// Guard を Drop すればスレッドがシャットダウンされる。
    /// ストリームを先に閉じれば、スレッドは自然終了する。
    pub fn into_streams(mut self) -> (UnixStream, UnixStream, TransportGuard) {
        let r = self.client_read.take().expect("client_read already taken");
        let w = self
            .client_write
            .take()
            .expect("client_write already taken");
        let guard = TransportGuard {
            bridge_thread: self.bridge_thread.take(),
            writer_relay_thread: self.writer_relay_thread.take(),
            shutdown: self.shutdown.clone(),
        };
        // self の Drop は client_read/write=None, threads=None なので何もしない
        (r, w, guard)
    }
}

#[cfg(unix)]
impl Drop for SshAgentTransport {
    fn drop(&mut self) {
        // into_streams() でスレッドが TransportGuard に移された場合は何もしない
        let has_threads = self.bridge_thread.is_some() || self.writer_relay_thread.is_some();
        if !has_threads {
            return;
        }

        self.shutdown.store(true, Ordering::Release);

        // UnixStream をシャットダウンしてブリッジスレッドに EOF を通知
        if let Some(ref s) = self.client_read {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        if let Some(ref s) = self.client_write {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }

        // タイムアウト付きでスレッド終了を待機（is_finished ポーリング）
        let deadline = Instant::now() + Duration::from_secs(JOIN_TIMEOUT_SECS);
        for t in [self.writer_relay_thread.take(), self.bridge_thread.take()]
            .into_iter()
            .flatten()
        {
            while !t.is_finished() {
                if Instant::now() >= deadline {
                    tracing::warn!("Bridge thread did not terminate within timeout");
                    break; // スレッドをリークするが、ハングは回避
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ブリッジループ
// ---------------------------------------------------------------------------

/// bridge スレッド: channel を排他所有し、`tokio::select!` で
/// SSH 受信データの読み取りと mpsc 経由の書き込みを多重化する。
#[cfg(unix)]
fn bridge_loop(
    handle: tokio::runtime::Handle,
    mut channel: russh::Channel<russh::client::Msg>,
    mut bridge_write: UnixStream,
    mut write_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    handle.block_on(async move {
        loop {
            if shutdown.load(Ordering::Acquire) {
                tracing::debug!("bridge_loop: shutdown signal received");
                break;
            }
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { ref data }) => {
                            if bridge_write.write_all(data).is_err() {
                                tracing::debug!("bridge_loop: write to pipe failed");
                                break;
                            }
                            let _ = bridge_write.flush();
                        }
                        Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                            // stderr をデバッグログに出力（診断用）
                            if let Ok(text) = std::str::from_utf8(data) {
                                tracing::debug!("bridge_loop: stderr: {text}");
                            }
                        }
                        Some(ChannelMsg::Eof) | None => {
                            tracing::debug!("bridge_loop: channel EOF");
                            let _ = bridge_write.shutdown(std::net::Shutdown::Write);
                            break;
                        }
                        Some(_) => {} // ExitStatus 等は無視
                    }
                }
                data = write_rx.recv() => {
                    match data {
                        Some(bytes) => {
                            if let Err(e) = channel.data(&bytes[..]).await {
                                tracing::debug!("bridge_loop: channel.data() failed: {e}");
                                break;
                            }
                        }
                        None => {
                            // writer-relay が終了 → stdin EOF を送信
                            tracing::debug!("bridge_loop: write_rx closed, sending EOF");
                            let _ = channel.eof().await;
                            // 読み取り方向はまだ継続する可能性があるが、
                            // 多くのプロトコルでは EOF 後に応答が返るため続行
                        }
                    }
                }
            }
        }
    });
}

/// writer-relay スレッド: UnixStream から読み取り、mpsc で bridge に送信する。
///
/// bridge_read には read timeout が設定されている前提。
/// タイムアウト時に shutdown フラグをチェックして安全に終了する。
#[cfg(unix)]
fn writer_relay_loop(
    mut bridge_read: UnixStream,
    write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        if shutdown.load(Ordering::Acquire) {
            tracing::debug!("writer_relay_loop: shutdown signal received");
            break;
        }
        let n = match bridge_read.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                // read timeout — shutdown チェックのためにループ先頭に戻る
                continue;
            }
            Err(e) => {
                tracing::debug!("writer_relay_loop: read error: {e}");
                break;
            }
        };
        if write_tx.send(buf[..n].to_vec()).is_err() {
            tracing::debug!("writer_relay_loop: mpsc send failed (bridge closed)");
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    /// UnixStream ペアの基本動作を確認（ブリッジの前提条件）
    #[test]
    fn unix_stream_pair_roundtrip() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let msg = b"hello transport";
        a.write_all(msg).unwrap();
        a.flush().unwrap();
        b.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut buf = vec![0u8; 64];
        let n = b.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], msg);
    }

    /// into_streams で取り出したストリームとガードが有効であることを確認
    #[test]
    fn into_streams_returns_valid_pair_and_guard() {
        let (r, w) = UnixStream::pair().unwrap();
        let transport = SshAgentTransport {
            client_read: Some(r),
            client_write: Some(w),
            bridge_thread: None,
            writer_relay_thread: None,
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        let (mut read_end, mut write_end, _guard) = transport.into_streams();
        write_end.write_all(b"test").unwrap();
        write_end.flush().unwrap();
        read_end
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = [0u8; 16];
        let n = read_end.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"test");
    }

    /// into_streams 後の Drop で shutdown が true にならないことを確認
    #[test]
    fn into_streams_does_not_set_shutdown() {
        let (r, w) = UnixStream::pair().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let transport = SshAgentTransport {
            client_read: Some(r),
            client_write: Some(w),
            bridge_thread: None,
            writer_relay_thread: None,
            shutdown: shutdown.clone(),
        };
        let (_r, _w, guard) = transport.into_streams();
        // transport の Drop が走った後も shutdown は false のまま
        assert!(!shutdown.load(Ordering::Acquire));
        // guard を明示的に drop して shutdown が true になることを確認
        drop(guard);
        assert!(shutdown.load(Ordering::Acquire));
    }

    /// Drop 時にスレッドなしでもパニックしないことを確認
    #[test]
    fn drop_without_threads_no_panic() {
        let (r, w) = UnixStream::pair().unwrap();
        let transport = SshAgentTransport {
            client_read: Some(r),
            client_write: Some(w),
            bridge_thread: None,
            writer_relay_thread: None,
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        drop(transport);
    }

    /// Drop 時にスレッドが正常終了することを確認
    #[test]
    fn drop_joins_threads() {
        let (r, w) = UnixStream::pair().unwrap();
        let (notify_tx, notify_rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let _ = notify_tx.send(());
        });
        let _ = notify_rx.recv();
        let transport = SshAgentTransport {
            client_read: Some(r),
            client_write: Some(w),
            bridge_thread: Some(thread),
            writer_relay_thread: None,
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        drop(transport);
    }

    /// TransportGuard の Drop でスレッドが正常終了することを確認
    #[test]
    fn guard_drop_joins_threads() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let (notify_tx, notify_rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let _ = notify_tx.send(());
        });
        let _ = notify_rx.recv();
        let guard = TransportGuard {
            bridge_thread: Some(thread),
            writer_relay_thread: None,
            shutdown,
        };
        drop(guard);
    }

    /// EOF 伝播: write 側を閉じると read 側が EOF になることを確認
    #[test]
    fn eof_propagation_via_shutdown() {
        let (mut read_end, write_end) = UnixStream::pair().unwrap();
        write_end.shutdown(std::net::Shutdown::Both).unwrap();
        read_end
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = [0u8; 16];
        let n = read_end.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    /// bridge_write_end を drop すると read_end で EOF になることを確認
    #[test]
    fn bridge_write_end_propagates_close() {
        let (read_end, write_end) = UnixStream::pair().unwrap();
        drop(write_end);
        let mut read_end = read_end;
        read_end
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = [0u8; 16];
        let n = read_end.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    /// writer_relay_loop が EOF で正常終了することを確認
    #[test]
    fn writer_relay_exits_on_eof() {
        let (relay_read, relay_write) = UnixStream::pair().unwrap();
        relay_read
            .set_read_timeout(Some(Duration::from_millis(RELAY_READ_TIMEOUT_MS)))
            .unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let shutdown = Arc::new(AtomicBool::new(false));

        let handle = std::thread::spawn({
            let shutdown = shutdown.clone();
            move || {
                writer_relay_loop(relay_read, tx, shutdown);
            }
        });

        // データを送信してから EOF
        relay_write.shutdown(std::net::Shutdown::Write).unwrap();
        handle.join().expect("writer_relay should exit cleanly");

        // rx は空（EOF のみだったため何も送信されない、または空）
        // ドロップ時にパニックしなければ OK
        drop(rx);
    }

    /// writer_relay_loop がデータを中継することを確認
    #[test]
    fn writer_relay_forwards_data() {
        let (relay_read, mut relay_write) = UnixStream::pair().unwrap();
        relay_read
            .set_read_timeout(Some(Duration::from_millis(RELAY_READ_TIMEOUT_MS)))
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let shutdown = Arc::new(AtomicBool::new(false));

        let handle = std::thread::spawn({
            let shutdown = shutdown.clone();
            move || {
                writer_relay_loop(relay_read, tx, shutdown);
            }
        });

        relay_write.write_all(b"hello").unwrap();
        relay_write.flush().unwrap();
        relay_write.shutdown(std::net::Shutdown::Write).unwrap();

        handle.join().expect("writer_relay should exit cleanly");

        let data = rx.try_recv().expect("should have received data");
        assert_eq!(data, b"hello");
    }

    /// shutdown フラグで writer_relay_loop が終了することを確認
    #[test]
    fn writer_relay_exits_on_shutdown() {
        let (relay_read, _relay_write) = UnixStream::pair().unwrap();
        relay_read
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let shutdown = Arc::new(AtomicBool::new(false));

        let handle = std::thread::spawn({
            let shutdown = shutdown.clone();
            move || {
                writer_relay_loop(relay_read, tx, shutdown);
            }
        });

        // shutdown フラグをセット
        shutdown.store(true, Ordering::Release);

        // タイムアウト + shutdown チェックで終了するはず
        handle.join().expect("writer_relay should exit on shutdown");
    }

    /// Drop がタイムアウト内に完了することを確認（ハング防止）
    #[test]
    fn drop_does_not_hang() {
        let (r, w) = UnixStream::pair().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));

        // 長時間スリープするスレッド（タイムアウトでリークされるはず）
        let thread = std::thread::spawn({
            let shutdown = shutdown.clone();
            move || {
                while !shutdown.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let transport = SshAgentTransport {
            client_read: Some(r),
            client_write: Some(w),
            bridge_thread: Some(thread),
            writer_relay_thread: None,
            shutdown,
        };

        let start = Instant::now();
        drop(transport);
        let elapsed = start.elapsed();
        // shutdown フラグで終了するので JOIN_TIMEOUT_SECS 以内に完了するはず
        assert!(
            elapsed < Duration::from_secs(JOIN_TIMEOUT_SECS + 1),
            "drop took too long: {elapsed:?}"
        );
    }

    /// TransportGuard の Drop がタイムアウト内に完了することを確認
    #[test]
    fn guard_drop_does_not_hang() {
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread = std::thread::spawn({
            let shutdown = shutdown.clone();
            move || {
                while !shutdown.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let guard = TransportGuard {
            bridge_thread: Some(thread),
            writer_relay_thread: None,
            shutdown,
        };

        let start = Instant::now();
        drop(guard);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(JOIN_TIMEOUT_SECS + 1),
            "guard drop took too long: {elapsed:?}"
        );
    }
}
