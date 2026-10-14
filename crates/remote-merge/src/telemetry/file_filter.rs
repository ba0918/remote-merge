//! 診断ログのファイルに残す記録の範囲。

use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;

use crate::agent::ssh_transport::REMOTE_STDERR_TARGET;

/// remote-merge 自身の crate の target（`remote_merge` で始まる target をすべて含む）
const OWN_TARGET_PREFIX: &str = "remote_merge";

/// 診断ログのファイルに remote-merge 自身の記録だけを通すフィルタ。
///
/// 外部の crate（SSH ライブラリなど）は細かい指定で暗号化前の送信データを記録するため、
/// 細かさの指定にかかわらず通さない。リモートの標準エラーの中継も外部の出力なので除く。
/// 細かさそのものは別のフィルタで決める。
pub fn own_records() -> Targets {
    Targets::new()
        .with_target(OWN_TARGET_PREFIX, LevelFilter::TRACE)
        .with_target(REMOTE_STDERR_TARGET, LevelFilter::OFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{JsonLogLayer, LogEntry};
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::filter::FilterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Layer;

    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 最も細かい指定で各 target に記録したとき、ファイルに残る target を返す
    fn saved_targets_at_trace(emit: impl FnOnce()) -> Vec<String> {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let layer = JsonLogLayer::new(SharedBuf(buf.clone()))
            .with_filter(tracing_subscriber::EnvFilter::new("trace").and(own_records()));
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, emit);
        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        output
            .lines()
            .map(|line| serde_json::from_str::<LogEntry>(line).unwrap().target)
            .collect()
    }

    #[test]
    fn keeps_records_of_every_own_crate() {
        let saved = saved_targets_at_trace(|| {
            tracing::info!(target: "remote_merge::runtime", "own");
            tracing::trace!(target: "remote_merge_ssh::client", "own ssh");
            tracing::debug!(target: "remote_merge_agent::agent::server", "own agent");
        });
        assert_eq!(
            saved,
            [
                "remote_merge::runtime",
                "remote_merge_ssh::client",
                "remote_merge_agent::agent::server"
            ]
        );
    }

    #[test]
    fn drops_external_crates_even_at_the_finest_level() {
        let saved = saved_targets_at_trace(|| {
            tracing::trace!(target: "russh::cipher", "buf = [1, 2, 3]");
            tracing::info!(target: "tokio::runtime", "external");
        });
        assert!(saved.is_empty(), "{saved:?}");
    }

    #[test]
    fn drops_the_relayed_remote_stderr() {
        let saved = saved_targets_at_trace(|| {
            tracing::debug!(target: REMOTE_STDERR_TARGET, "bridge_loop: stderr: remote text");
        });
        assert!(saved.is_empty(), "{saved:?}");
    }
}
