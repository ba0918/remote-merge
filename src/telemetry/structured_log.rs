//! 構造化ログ: tracing イベントを JSONL 形式でファイルに出力する Layer。
//!
//! `init_tracing` から使用し、debug.log を JSONL フォーマットで書き出す。
//! 出力フォーマットは `log_reader::LogEntry` と互換。

use std::io::Write;
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use super::log_reader::LogEntry;

/// JSONL 形式でログをファイルに書き出す tracing Layer
pub struct JsonLogLayer<W: Write + Send + 'static> {
    writer: Mutex<W>,
}

impl<W: Write + Send + 'static> JsonLogLayer<W> {
    /// 新しい JsonLogLayer を作成する
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }
}

impl<S, W> Layer<S> for JsonLogLayer<W>
where
    S: Subscriber,
    W: Write + Send + 'static,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        // フィールドを収集
        let mut visitor = FieldCollector::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: visitor.message.unwrap_or_default(),
            fields: if visitor.fields.is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                serde_json::Value::Object(visitor.fields)
            },
        };

        if let Ok(json) = serde_json::to_string(&entry) {
            if let Ok(mut writer) = self.writer.lock() {
                let _ = writeln!(writer, "{}", json);
                let _ = writer.flush();
            }
        }
    }
}

/// tracing フィールドを収集するビジター
#[derive(Default)]
struct FieldCollector {
    message: Option<String>,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let val = format!("{:?}", value);
        if field.name() == "message" {
            self.message = Some(val);
        } else {
            self.fields
                .insert(field.name().to_string(), serde_json::Value::String(val));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(
                field.name().to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::Bool(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn test_json_log_layer_output_format() {
        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        let buf_clone = buf.clone();

        // tracing subscriber をスコープ内で設定
        let layer = JsonLogLayer::new(BufWriter(buf_clone));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "test::module", "hello world");
        });

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let entry: LogEntry = serde_json::from_str(output.trim()).unwrap();

        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.target, "test::module");
        assert_eq!(entry.message, "hello world");
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn test_json_log_layer_with_fields() {
        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        let buf_clone = buf.clone();

        let layer = JsonLogLayer::new(BufWriter(buf_clone));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(server = "develop", elapsed_ms = 500u64, "connection slow");
        });

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let entry: LogEntry = serde_json::from_str(output.trim()).unwrap();

        assert_eq!(entry.level, "WARN");
        assert_eq!(entry.message, "connection slow");
        assert_eq!(entry.fields["server"], "develop");
        assert_eq!(entry.fields["elapsed_ms"], 500);
    }

    #[test]
    fn test_json_log_layer_multiple_events() {
        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        let buf_clone = buf.clone();

        let layer = JsonLogLayer::new(BufWriter(buf_clone));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("first");
            tracing::error!("second");
        });

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = output.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        let entry1: LogEntry = serde_json::from_str(lines[0]).unwrap();
        let entry2: LogEntry = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(entry1.message, "first");
        assert_eq!(entry2.level, "ERROR");
    }

    /// Arc<Mutex<Vec<u8>>> のラッパー（Write trait 実装用）
    struct BufWriter(std::sync::Arc<Mutex<Vec<u8>>>);

    impl Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }
}
