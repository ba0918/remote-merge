//! 選んだ経路の操作の `接続断` の扱い（docs/ir/ssh/reconnect.md）。
//!
//! 読み取りは同じ経路でつなぎ直して一度だけやり直し、だめならそのサーバへの処理を止める
//! （REQ-ssh-003・REQ-ssh-023）。変更はやり直さずに止め、書き込み先の確かめは `confirm` が行う
//! （REQ-ssh-004・REQ-ssh-022）。どちらも別の経路には切り替えない。

use super::connection::{classify, is_stopped, retry_failed, ProcessingStopped, StopCause};
use super::core::CoreRuntime;
use crate::side::Side;

/// 処理を止める単位（サーバか、ローカルの側）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StopKey {
    Server(String),
    Local,
}

impl StopKey {
    pub(crate) fn of(side: &Side) -> Self {
        match side.server_name() {
            Some(name) => Self::Server(name.to_string()),
            None => Self::Local,
        }
    }

    fn shown(&self) -> &str {
        match self {
            Self::Server(name) => name,
            Self::Local => "local",
        }
    }
}

/// 止めた理由
#[derive(Debug, Clone)]
pub(crate) struct Stop {
    cause: StopCause,
    reason: String,
}

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

    /// そのサーバへの処理を止めた印を立て、`error` をそのまま返す
    fn stop(&mut self, server_name: &str, error: anyhow::Error) -> anyhow::Error {
        let key = StopKey::Server(server_name.to_string());
        self.stop_with(key, StopCause::ConnectionLost, &error);
        error
    }

    /// root_dir がなくなったので、`key` への処理を止めた印を立てる（D47）
    pub(crate) fn stop_for_root_gone(&mut self, key: StopKey, error: &anyhow::Error) {
        self.stop_with(key, StopCause::RootGone, error);
    }

    fn stop_with(&mut self, key: StopKey, cause: StopCause, error: &anyhow::Error) {
        self.stopped.entry(key).or_insert_with(|| Stop {
            cause,
            reason: format!("{error:#}"),
        });
    }

    /// どれかの書き込み先への処理を止めたか（その回は終了コード 2 で終える）
    pub fn has_stopped(&self) -> bool {
        !self.stopped.is_empty()
    }

    /// root_dir がなくなって処理を止めた側があれば、その「行わなかった」のエラー（止めたときの最初の
    /// root_dir がない誤りを持つ。D48）。読むだけのコマンドが、それを出して止めるのに使う
    pub fn root_gone_stop(&self) -> Option<anyhow::Error> {
        self.stopped
            .iter()
            .find(|(_, stop)| stop.cause == StopCause::RootGone)
            .and_then(|(key, _)| self.stopped_error_of(key))
    }

    /// `side` への処理を止めていたら、行わなかったことを示すエラー
    pub fn stopped_error_for(&self, side: &Side) -> Option<anyhow::Error> {
        self.stopped_error_of(&StopKey::of(side))
    }

    pub(crate) fn stopped_error_of(&self, key: &StopKey) -> Option<anyhow::Error> {
        self.stopped.get(key).map(|stop| {
            ProcessingStopped {
                server: key.shown().to_string(),
                reason: stop.reason.clone(),
                cause: stop.cause,
            }
            .into()
        })
    }
}

/// 接続が使えなくなった失敗か。止めた後の「行わなかった」は、もうつなぎ直さない
fn is_lost(error: &anyhow::Error) -> bool {
    classify(error).is_disconnection() && !is_stopped(error)
}
