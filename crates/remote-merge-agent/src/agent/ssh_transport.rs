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
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::thread::JoinHandle;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use russh::ChannelMsg;
#[cfg(unix)]
use tokio::sync::Notify;

/// リモートの agent が標準エラーに出した文字列を中継する記録の target。
///
/// 中身はリモートのシェルや sudo も含む外部の出力なので、診断ログのファイルには残さない。
pub const REMOTE_STDERR_TARGET: &str = "remote_merge::agent::remote_stderr";

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
/// [`start`] がストリームと一緒に返し、ストリームとセットで保持する。
/// Drop 時にスレッドを安全にシャットダウン・join する。
#[cfg(unix)]
pub struct TransportGuard {
    bridge_thread: Option<JoinHandle<()>>,
    writer_relay_thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    wakeup: Arc<Notify>,
}

#[cfg(unix)]
impl Drop for TransportGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.wakeup.notify_one();

        let deadline = Instant::now() + Duration::from_secs(JOIN_TIMEOUT_SECS);
        for t in [self.writer_relay_thread.take(), self.bridge_thread.take()]
            .into_iter()
            .flatten()
        {
            while !t.is_finished() {
                if Instant::now() >= deadline {
                    tracing::warn!(target: "remote_merge::agent::ssh_transport", "Bridge thread did not terminate within timeout");
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
// 起動
// ---------------------------------------------------------------------------

/// SSH exec チャネルと AgentClient の間を中継するブリッジスレッド（+ writer-relay）を起動し、
/// AgentClient に渡す (read, write) の UnixStream と、スレッドを管理する `TransportGuard` を返す。
///
/// チャネルは bridge スレッドが排他所有するため、共有ロックによるデッドロックは原理的に発生しない。
/// Guard を Drop すればスレッドがシャットダウンされる。ストリームを先に閉じれば、スレッドは自然終了する。
///
/// - `handle`: tokio ランタイムハンドル（block_on 用）
/// - `channel`: SSH exec チャネル（既にコマンド実行済みであること）
/// - `read_timeout`: 手元が応答を読むときの時間切れ。エージェントが答えないまま接続だけが
///   生きている場合に、コマンドが返らないままにならないようにする（`接続断` の時間切れ。
///   docs/ir/ssh/reconnect.md の REQ-ssh-003）
#[cfg(unix)]
pub fn start(
    handle: tokio::runtime::Handle,
    channel: russh::Channel<russh::client::Msg>,
    read_timeout: Duration,
) -> Result<(UnixStream, UnixStream, TransportGuard)> {
    // read 方向: bridge → client_read
    let (client_read, bridge_write) = UnixStream::pair()?;
    // write 方向: client_write → writer-relay → bridge
    let (bridge_read, client_write) = UnixStream::pair()?;

    // writer-relay が shutdown フラグをポーリングできるよう read timeout を設定
    bridge_read.set_read_timeout(Some(Duration::from_millis(RELAY_READ_TIMEOUT_MS)))?;
    // 手元が読む側の時間切れ（時間切れは応答なしの `接続断` として運ばれる）
    client_read.set_read_timeout(Some(read_timeout))?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let wakeup = Arc::new(Notify::new());
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    // Writer relay: UnixStream から読み取り → mpsc で bridge に送信
    let shutdown2 = shutdown.clone();
    let writer_relay_thread = std::thread::Builder::new()
        .name("agent-write-relay".into())
        .spawn(move || {
            writer_relay_loop(bridge_read, &write_tx, &shutdown2);
        })?;

    // Bridge: channel を排他所有し、select! で双方向中継
    let shutdown3 = shutdown.clone();
    let bridge_wakeup = wakeup.clone();
    let bridge_thread = std::thread::Builder::new()
        .name("agent-bridge".into())
        .spawn(move || {
            bridge_loop(
                &handle,
                channel,
                bridge_write,
                write_rx,
                shutdown3,
                bridge_wakeup,
            );
        })?;

    let guard = TransportGuard {
        bridge_thread: Some(bridge_thread),
        writer_relay_thread: Some(writer_relay_thread),
        shutdown,
        wakeup,
    };
    Ok((client_read, client_write, guard))
}

// ---------------------------------------------------------------------------
// ブリッジループ
// ---------------------------------------------------------------------------

/// bridge スレッド: channel を排他所有し、`tokio::select!` で
/// SSH 受信データの読み取りと mpsc 経由の書き込みを多重化する。
#[cfg(unix)]
fn bridge_loop(
    handle: &tokio::runtime::Handle,
    mut channel: russh::Channel<russh::client::Msg>,
    mut bridge_write: UnixStream,
    mut write_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
    wakeup: Arc<Notify>,
) {
    handle.block_on(async move {
        let mut write_open = true;
        loop {
            if shutdown.load(Ordering::Acquire) {
                tracing::debug!(target: "remote_merge::agent::ssh_transport", "bridge_loop: shutdown signal received");
                break;
            }
            tokio::select! {
                // One bridge waits here; notify_one retains a permit if Drop wins the race.
                _ = wakeup.notified() => break,
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { ref data }) => {
                            if let Err(e) = bridge_write.write_all(data) {
                                tracing::warn!(target: "remote_merge::agent::ssh_transport", "bridge_loop: write to pipe failed: {e}");
                                break;
                            }
                            let _ = bridge_write.flush();
                        }
                        Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                            // stderr をデバッグログに出力（診断用）
                            if let Ok(text) = std::str::from_utf8(data) {
                                tracing::debug!(target: REMOTE_STDERR_TARGET, "bridge_loop: stderr: {text}");
                            }
                        }
                        Some(ChannelMsg::Eof) | None => {
                            tracing::debug!(target: "remote_merge::agent::ssh_transport", "bridge_loop: channel EOF");
                            let _ = bridge_write.shutdown(std::net::Shutdown::Write);
                            break;
                        }
                        Some(_) => {} // ExitStatus 等は無視
                    }
                }
                data = write_rx.recv(), if write_open => {
                    match data {
                        Some(bytes) => {
                            if let Err(e) = channel.data(&bytes[..]).await {
                                tracing::warn!(target: "remote_merge::agent::ssh_transport", "bridge_loop: channel.data() failed: {e}");
                                break;
                            }
                        }
                        None => {
                            write_open = false;
                            // writer-relay が終了 → stdin EOF を送信
                            tracing::debug!(target: "remote_merge::agent::ssh_transport", "bridge_loop: write_rx closed, sending EOF");
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
    write_tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    shutdown: &Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        if shutdown.load(Ordering::Acquire) {
            tracing::debug!(target: "remote_merge::agent::ssh_transport", "writer_relay_loop: shutdown signal received");
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
                tracing::warn!(target: "remote_merge::agent::ssh_transport", "writer_relay_loop: read error: {e}");
                break;
            }
        };
        if write_tx.send(buf[..n].to_vec()).is_err() {
            // 橋渡しの側が先に終わった。セッションの終わりにふつうに起きる順なので警告にしない
            // （本当の切断は、手元の読み込みが接続断として知らせる）
            tracing::debug!(target: "remote_merge::agent::ssh_transport", "writer_relay_loop: mpsc send failed (bridge closed)");
            break;
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
#[path = "ssh_transport_relay_tests.rs"]
mod relay_tests;

#[cfg(all(test, unix))]
#[path = "ssh_transport_bridge_tests.rs"]
mod bridge_tests;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

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
                writer_relay_loop(relay_read, &tx, &shutdown);
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
                writer_relay_loop(relay_read, &tx, &shutdown);
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
                writer_relay_loop(relay_read, &tx, &shutdown);
            }
        });

        // shutdown フラグをセット
        shutdown.store(true, Ordering::Release);

        // タイムアウト + shutdown チェックで終了するはず
        handle.join().expect("writer_relay should exit on shutdown");
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
            wakeup: Arc::new(Notify::new()),
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
