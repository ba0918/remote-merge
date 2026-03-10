//! SSH チャネル ↔ sync Read/Write ブリッジ。
//!
//! russh の async SSH exec チャネルと AgentClient の sync I/O (UnixStream) を
//! ブリッジスレッドで接続する。
//!
//! **現在はスタブ実装。** 実際の async ↔ sync ブリッジは
//! SSH transport 対応の PR で実装予定。

// TODO: SshAgentTransport 構造体
//
// pub struct SshAgentTransport {
//     client_read: UnixStream,
//     client_write: UnixStream,
//     _reader_thread: JoinHandle<()>,  // SSH stdout → client_read
//     _writer_thread: JoinHandle<()>,  // client_write → SSH stdin
// }
//
// impl SshAgentTransport {
//     pub fn start(
//         rt: &tokio::runtime::Runtime,
//         ssh_client: &mut SshClient,
//         command: &str,
//     ) -> Result<Self> { ... }
//
//     pub fn into_streams(self) -> (UnixStream, UnixStream) { ... }
// }
