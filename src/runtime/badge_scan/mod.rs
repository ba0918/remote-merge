//! ディレクトリ展開時の非同期バッジスキャン。
//!
//! 展開されたディレクトリの直下ファイルをバックグラウンドで diff スキャンし、
//! バッジを `[?]` → `[M]`/`[=]` 等に更新する。
//!
//! 責務分離:
//! - helpers: 純粋関数（ファイル収集、バッジ操作）
//! - task: ワーカースレッド処理（ファイル読み込み + 結果送信）
//! - poll: イベントループからのポーリング + バッジ更新

pub mod helpers;
pub mod poll;
pub mod task;

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use crate::app::BadgeScanMsg;

/// バッジスキャンの進行中エントリ
pub struct BadgeScanEntry {
    pub receiver: mpsc::Receiver<BadgeScanMsg>,
    pub cancel_flag: Arc<AtomicBool>,
}
