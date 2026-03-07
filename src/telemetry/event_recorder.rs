//! イベントレコーダー: TUI イベントを JSONL ファイルに記録する。
//!
//! イベントループから呼び出され、`events.jsonl` に追記する。
//! ファイル I/O エラーは無視する（テレメトリ障害で TUI を止めない）。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::event_types::TuiEvent;

/// イベントレコーダー
///
/// ファイルハンドルを保持し、イベントを JSONL 形式で追記する。
pub struct EventRecorder {
    path: PathBuf,
    /// 書き込み用ファイルハンドル（遅延オープン）
    file: Option<std::fs::File>,
}

impl EventRecorder {
    /// 新しいレコーダーを作成する
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: None,
        }
    }

    /// イベントを記録する
    ///
    /// シリアライズやファイル I/O に失敗しても無視する。
    pub fn record(&mut self, event: &TuiEvent) {
        let json = match serde_json::to_string(event) {
            Ok(j) => j,
            Err(_) => return,
        };

        let file = match self.ensure_file() {
            Some(f) => f,
            None => return,
        };

        let _ = writeln!(file, "{}", json);
        let _ = file.flush();
    }

    /// イベントを構築して即座に記録するヘルパー
    pub fn record_key_press(&mut self, key: &str, result: &str) {
        let event = super::event_types::key_press_event(key, result);
        self.record(&event);
    }

    /// 描画遅延イベントを記録する
    pub fn record_render_slow(&mut self, frame: u64, duration_ms: u64) {
        let event = super::event_types::render_slow_event(frame, duration_ms);
        self.record(&event);
    }

    /// エラーイベントを記録する
    pub fn record_error(&mut self, kind: &str, target: &str, message: &str) {
        let event = super::event_types::error_event(kind, target, message);
        self.record(&event);
    }

    /// ダイアログイベントを記録する
    pub fn record_dialog(&mut self, action: &str, dialog_kind: &str) {
        let event = super::event_types::dialog_event(action, dialog_kind);
        self.record(&event);
    }

    /// ファイルハンドルを確保する（遅延オープン）
    fn ensure_file(&mut self) -> Option<&mut std::fs::File> {
        if self.file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok()?;
            self.file = Some(file);
        }
        self.file.as_mut()
    }
}

/// イベントファイルを読み込んでフィルタリングする（純粋関数）
///
/// CLI の `events` サブコマンドから使用する。
pub fn read_events(
    path: &Path,
    event_type: Option<&str>,
    tail: Option<usize>,
) -> std::io::Result<Vec<String>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = content
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| {
            if let Some(kind) = event_type {
                // JSON の "event":"<kind>" パターンでフィルタ
                let pattern = format!("\"event\":\"{}\"", kind);
                line.contains(&pattern)
            } else {
                true
            }
        })
        .map(|s| s.to_string())
        .collect();

    if let Some(n) = tail {
        if lines.len() > n {
            lines = lines.split_off(lines.len() - n);
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::event_types;

    #[test]
    fn test_recorder_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut recorder = EventRecorder::new(&path);

        let event = event_types::key_press_event("j", "cursor_moved");
        recorder.record(&event);

        let event2 = event_types::error_event("timeout", "ssh", "connection lost");
        recorder.record(&event2);

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"key_press\""));
        assert!(lines[1].contains("\"error\""));
    }

    #[test]
    fn test_recorder_helper_methods() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let mut recorder = EventRecorder::new(&path);

        recorder.record_key_press("k", "cursor_up");
        recorder.record_render_slow(10, 200);
        recorder.record_error("io", "file", "not found");
        recorder.record_dialog("open", "confirm");

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_recorder_nonexistent_dir_fails_gracefully() {
        let path = std::path::PathBuf::from("/nonexistent/dir/events.jsonl");
        let mut recorder = EventRecorder::new(&path);

        // Should not panic, just silently fail
        let event = event_types::key_press_event("j", "ok");
        recorder.record(&event);
    }

    #[test]
    fn test_read_events_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let events = read_events(&path, None, None).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_read_events_filter_by_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let lines = [
            r#"{"ts":"2026-01-01T00:00:00Z","event":"key_press","key":"j","result":"ok"}"#,
            r#"{"ts":"2026-01-01T00:00:01Z","event":"error","kind":"io","target":"ssh","message":"fail"}"#,
            r#"{"ts":"2026-01-01T00:00:02Z","event":"key_press","key":"k","result":"ok"}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        let result = read_events(&path, Some("key_press"), None).unwrap();
        assert_eq!(result.len(), 2);

        let result = read_events(&path, Some("error"), None).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_read_events_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let lines: Vec<String> = (0..50)
            .map(|i| {
                format!(
                    r#"{{"ts":"t","event":"key_press","key":"{}","result":"ok"}}"#,
                    i
                )
            })
            .collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        let result = read_events(&path, None, Some(5)).unwrap();
        assert_eq!(result.len(), 5);
        assert!(result[4].contains("\"key\":\"49\""));
    }
}
