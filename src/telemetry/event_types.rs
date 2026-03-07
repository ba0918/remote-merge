//! TUI イベントの型定義。
//!
//! TUI操作中の構造化イベントを定義する。
//! 全型は Serialize を derive し、JSONL 出力に対応する。

use serde::Serialize;

/// TUI イベント（1レコード = JSONL の1行）
#[derive(Debug, Clone, Serialize)]
pub struct TuiEvent {
    /// ISO 8601 タイムスタンプ
    pub ts: String,
    /// イベント種別
    pub event: EventKind,
    /// イベント固有の詳細データ
    #[serde(flatten)]
    pub detail: EventDetail,
}

/// イベント種別
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// キー入力
    KeyPress,
    /// フレーム描画が遅い（閾値超え）
    RenderSlow,
    /// SSHコマンド実行
    SshExec,
    /// AppState 変更
    StateChange,
    /// エラー発生
    Error,
    /// ダイアログ表示/操作
    Dialog,
}

/// イベント固有の詳細
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EventDetail {
    /// キー入力イベント
    KeyPress { key: String, result: String },
    /// 描画遅延イベント
    RenderSlow { frame: u64, duration_ms: u64 },
    /// SSH実行イベント
    SshExec {
        cmd: String,
        status: String,
        elapsed_ms: u64,
    },
    /// 状態変更イベント
    StateChange { field: String, description: String },
    /// エラーイベント
    Error {
        kind: String,
        target: String,
        message: String,
    },
    /// ダイアログイベント
    Dialog { action: String, dialog_kind: String },
}

/// 現在時刻を ISO 8601 文字列で取得
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// キー入力イベントを構築する
pub fn key_press_event(key: &str, result: &str) -> TuiEvent {
    TuiEvent {
        ts: now_iso8601(),
        event: EventKind::KeyPress,
        detail: EventDetail::KeyPress {
            key: key.to_string(),
            result: result.to_string(),
        },
    }
}

/// 描画遅延イベントを構築する
pub fn render_slow_event(frame: u64, duration_ms: u64) -> TuiEvent {
    TuiEvent {
        ts: now_iso8601(),
        event: EventKind::RenderSlow,
        detail: EventDetail::RenderSlow { frame, duration_ms },
    }
}

/// エラーイベントを構築する
pub fn error_event(kind: &str, target: &str, message: &str) -> TuiEvent {
    TuiEvent {
        ts: now_iso8601(),
        event: EventKind::Error,
        detail: EventDetail::Error {
            kind: kind.to_string(),
            target: target.to_string(),
            message: message.to_string(),
        },
    }
}

/// ダイアログイベントを構築する
pub fn dialog_event(action: &str, dialog_kind: &str) -> TuiEvent {
    TuiEvent {
        ts: now_iso8601(),
        event: EventKind::Dialog,
        detail: EventDetail::Dialog {
            action: action.to_string(),
            dialog_kind: dialog_kind.to_string(),
        },
    }
}

/// 状態変更イベントを構築する
pub fn state_change_event(field: &str, description: &str) -> TuiEvent {
    TuiEvent {
        ts: now_iso8601(),
        event: EventKind::StateChange,
        detail: EventDetail::StateChange {
            field: field.to_string(),
            description: description.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_press_event_serialization() {
        let event = key_press_event("j", "cursor_moved");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"key_press\""));
        assert!(json.contains("\"key\":\"j\""));
        assert!(json.contains("\"result\":\"cursor_moved\""));
        assert!(json.contains("\"ts\":"));
    }

    #[test]
    fn test_render_slow_event_serialization() {
        let event = render_slow_event(42, 150);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"render_slow\""));
        assert!(json.contains("\"frame\":42"));
        assert!(json.contains("\"duration_ms\":150"));
    }

    #[test]
    fn test_error_event_serialization() {
        let event = error_event("connection_lost", "ssh::client", "timeout after 30s");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"error\""));
        assert!(json.contains("\"kind\":\"connection_lost\""));
        assert!(json.contains("\"target\":\"ssh::client\""));
    }

    #[test]
    fn test_dialog_event_serialization() {
        let event = dialog_event("open", "confirm");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"dialog\""));
        assert!(json.contains("\"action\":\"open\""));
        assert!(json.contains("\"dialog_kind\":\"confirm\""));
    }

    #[test]
    fn test_state_change_event_serialization() {
        let event = state_change_event("focus", "changed to diff_view");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"state_change\""));
        assert!(json.contains("\"field\":\"focus\""));
    }

    #[test]
    fn test_event_kind_equality() {
        assert_eq!(EventKind::KeyPress, EventKind::KeyPress);
        assert_ne!(EventKind::KeyPress, EventKind::Error);
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        // ISO 8601 format check: starts with 20xx
        assert!(ts.starts_with("20"));
        assert!(ts.contains("T"));
        assert!(ts.ends_with("Z"));
    }
}
