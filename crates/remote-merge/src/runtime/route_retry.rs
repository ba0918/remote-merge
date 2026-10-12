//! 選んだ経路の操作の `接続断` の扱い（docs/ir/ssh/reconnect.md）。
//!
//! 読み取りは同じ経路でつなぎ直して一度だけやり直し、だめならそのサーバへの処理を止める
//! （REQ-ssh-003・REQ-ssh-023）。変更はやり直さずに止め、書き込み先の確かめは `confirm` が行う
//! （REQ-ssh-004・REQ-ssh-022）。どちらも別の経路には切り替えない。

use super::connection::{classify, is_stopped, retry_failed, ProcessingStopped};
use super::core::CoreRuntime;

impl CoreRuntime {
    /// 読み取りの操作。`接続断` なら同じ経路でつなぎ直して一度だけやり直す
    pub(crate) fn read_on_route<T>(
        &mut self,
        server_name: &str,
        mut read: impl FnMut(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let first = match read(self) {
            Err(error) if is_lost(&error) => error,
            other => return other,
        };
        tracing::warn!("Read lost the connection to {server_name}; reconnecting once: {first:#}");
        if let Err(reconnect) = self.reconnect_route(server_name) {
            let error = retry_failed(first, "could not reconnect", &reconnect);
            return Err(self.stop(server_name, error));
        }
        match read(self) {
            Err(second) if is_lost(&second) => {
                let error = retry_failed(first, "the retry also failed", &second);
                Err(self.stop(server_name, error))
            }
            other => other,
        }
    }

    /// 変更の操作。`接続断` ならやり直さずに、そのサーバへの処理を止めて失敗を返す
    pub(crate) fn change_on_route<T>(
        &mut self,
        server_name: &str,
        change: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        match change(self) {
            Err(error) if is_lost(&error) => Err(self.stop(server_name, error)),
            other => other,
        }
    }

    /// SSH の経路の読み取り（大きさの問い合わせなどの中で使う）。[`Self::read_on_route`] と同じ
    pub(crate) fn ssh_read<T>(
        &mut self,
        server_name: &str,
        read: impl FnMut(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        self.read_on_route(server_name, read)
    }

    /// そのサーバへの処理を止めた印を立て、`error` をそのまま返す
    fn stop(&mut self, server_name: &str, error: anyhow::Error) -> anyhow::Error {
        self.stopped
            .entry(server_name.to_string())
            .or_insert_with(|| format!("{error:#}"));
        error
    }

    /// どれかのサーバへの処理を `接続断` で止めたか（その回は終了コード 2 で終える）
    pub fn has_stopped(&self) -> bool {
        !self.stopped.is_empty()
    }

    /// `side` が処理を止めたサーバなら、行わなかったことを示すエラー
    pub fn stopped_error_for(&self, side: &crate::side::Side) -> Option<anyhow::Error> {
        side.server_name()
            .and_then(|server_name| self.stopped_error(server_name))
    }

    /// 処理を止めたサーバなら、行わなかったことを示すエラー
    pub(crate) fn stopped_error(&self, server_name: &str) -> Option<anyhow::Error> {
        self.stopped.get(server_name).map(|reason| {
            ProcessingStopped {
                server: server_name.to_string(),
                reason: reason.clone(),
            }
            .into()
        })
    }
}

/// 接続が使えなくなった失敗か。止めた後の「行わなかった」は、もうつなぎ直さない
fn is_lost(error: &anyhow::Error) -> bool {
    classify(error).is_disconnection() && !is_stopped(error)
}
