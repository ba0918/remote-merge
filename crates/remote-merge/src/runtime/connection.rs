//! リモートの操作の失敗の種類の判定（docs/ir/ssh/reconnect.md と、用語の `接続断`）。
//!
//! 判定はここの一つだけで行う。エラーを文字列に変える前に、エラーの型で判定する。

use std::io;

use crate::agent::client::AgentRefused;
use crate::error::AppError;
use remote_merge_core::op_failure::{Disconnection, OpError, OpFailure};

/// リモートの操作の失敗の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// 要求を送る前に接続が使えなかった。送っていないので、変更は行われていない
    NotSent,
    /// 要求を送り始めた後に応答を受け取れなかった（`接続断`）。変更なら結果が分からない
    Lost,
    /// 相手が失敗を返した、またはこの手元で拒んだ。接続は使える
    Refused,
}

impl Failure {
    /// 接続が使えなくなったか（送る前も送った後も）
    pub fn is_disconnection(self) -> bool {
        self != Failure::Refused
    }
}

/// `error` の種類。原因の連なりのうち、最初に種類の分かる原因で決める
pub fn classify(error: &anyhow::Error) -> Failure {
    for cause in error.chain() {
        if let Some(labeled) = cause.downcast_ref::<Labeled>() {
            return labeled.failure;
        }
        if cause.is::<AgentRefused>() {
            return Failure::Refused;
        }
        if let Some(error) = cause.downcast_ref::<OpError>() {
            return match &error.failure {
                OpFailure::Disconnected(Disconnection::NotSent(_)) => Failure::NotSent,
                OpFailure::Disconnected(Disconnection::NoAnswer(_)) => Failure::Lost,
                OpFailure::NotFound
                | OpFailure::NotADirectory
                | OpFailure::PermissionDenied
                | OpFailure::TooLarge { .. }
                | OpFailure::ToolMissing(_)
                | OpFailure::Io(_) => Failure::Refused,
            };
        }
        // 確かめた変更の失敗は、そのサーバへの処理を止めた `接続断` のまま
        if cause.is::<super::confirm::ConfirmedChange>() {
            return Failure::Lost;
        }
        if cause.is::<ProcessingStopped>() {
            return Failure::NotSent;
        }
        if let Some(error) = cause.downcast_ref::<AppError>() {
            return match error {
                AppError::SshConnection { .. } => Failure::NotSent,
                AppError::SshDisconnected { .. } | AppError::SshTimeout { .. } => Failure::Lost,
                _ => Failure::Refused,
            };
        }
        if let Some(error) = cause.downcast_ref::<io::Error>() {
            // 大きすぎるフレームは、手元で何も書かずに拒んだもので、接続は使える
            if error.kind() == io::ErrorKind::InvalidInput {
                return Failure::Refused;
            }
            // エージェントとの通信路の読み書きのほかの失敗は、どの種類でも応答を受け取れなかった
            // ことになる
            return Failure::Lost;
        }
    }
    Failure::Refused
}

/// 接続の使えない失敗 `error` の後のやり直し（再接続かエージェントの起動し直し）が `retry` で
/// 失敗したときのエラー。種類は最初の失敗の `error` で決まり、`retry` は文言にだけ含める
/// （再接続が認証で拒まれても、そのサーバへの処理を止める `接続断` のまま）
pub fn retry_failed(error: anyhow::Error, what: &str, retry: &anyhow::Error) -> anyhow::Error {
    let message = format!("{error}; {what}: {retry:#}");
    error.context(message)
}

/// 読み取りが接続の使えないまま失敗したことを文言に示す（REQ-ssh-003・REQ-ssh-023）。
/// 種類は変えない
pub fn label_read(error: anyhow::Error) -> anyhow::Error {
    let failure = classify(&error);
    if is_stopped(&error) || failure == Failure::Refused {
        return error;
    }
    let message = format!("read failed: the connection was lost: {error:#}");
    Labeled { failure, message }.into()
}

/// 文言を付けた接続の失敗。種類は `failure` に持ち、原因は文言に含める（原因の連なりは持たない
/// ので、`{:#}` で同じ原因が二度並ばない）
#[derive(Debug)]
struct Labeled {
    failure: Failure,
    message: String,
}

impl std::fmt::Display for Labeled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Labeled {}

/// 文言の前に `prefix` を付ける。接続の使えない失敗は種類を残し、それ以外は今までどおり
/// 文言だけの失敗にする
pub fn prefixed(error: anyhow::Error, prefix: &str) -> anyhow::Error {
    match classify(&error) {
        Failure::Refused => anyhow::anyhow!("{prefix}{error}"),
        failure => Labeled {
            failure,
            message: format!("{prefix}{error:#}"),
        }
        .into(),
    }
}

/// `接続断` の後に止めたので行わなかった失敗か
pub(crate) fn is_stopped(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<ProcessingStopped>())
}

/// `接続断` の後、その書き込み先への処理を止めたので行わなかった
#[derive(Debug)]
pub struct ProcessingStopped {
    pub server: String,
    /// 止めた理由（最初の `接続断` のエラー）
    pub reason: String,
}

impl std::fmt::Display for ProcessingStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "not attempted: processing for {} stopped after the connection was lost ({})",
            self.server, self.reason
        )
    }
}

impl std::error::Error for ProcessingStopped {}

#[cfg(test)]
mod tests {
    use super::*;

    fn io(kind: io::ErrorKind) -> anyhow::Error {
        io::Error::new(kind, "pipe").into()
    }

    fn app(error: AppError) -> anyhow::Error {
        error.into()
    }

    #[test]
    fn each_failure_is_classified_by_its_type() {
        let host = || "fixture".to_string();
        let cases: Vec<(&str, anyhow::Error, Failure)> = vec![
            ("broken pipe", io(io::ErrorKind::BrokenPipe), Failure::Lost),
            ("reset", io(io::ErrorKind::ConnectionReset), Failure::Lost),
            (
                "aborted",
                io(io::ErrorKind::ConnectionAborted),
                Failure::Lost,
            ),
            ("eof", io(io::ErrorKind::UnexpectedEof), Failure::Lost),
            (
                "broken frame",
                io(io::ErrorKind::InvalidData),
                Failure::Lost,
            ),
            // 読み込みの時間切れ。sudo のサーバでも SSH に回さない側に入る（A19）
            ("read timeout", io(io::ErrorKind::TimedOut), Failure::Lost),
            ("would block", io(io::ErrorKind::WouldBlock), Failure::Lost),
            ("other io", io(io::ErrorKind::Other), Failure::Lost),
            // 大きすぎて送らなかった要求（write_frame が何も書かずに返す）。接続は使える
            (
                "frame too large to send",
                io(io::ErrorKind::InvalidInput),
                Failure::Refused,
            ),
            (
                "agent error",
                AgentRefused("agent error: x".into()).into(),
                Failure::Refused,
            ),
            (
                "stopped earlier",
                ProcessingStopped {
                    server: host(),
                    reason: "lost".into(),
                }
                .into(),
                Failure::NotSent,
            ),
            (
                "ssh before sending",
                app(AppError::SshConnection {
                    host: host(),
                    message: "Failed to open channel".into(),
                }),
                Failure::NotSent,
            ),
            (
                "ssh without exit status",
                app(AppError::SshDisconnected {
                    host: host(),
                    message: "Remote command ended without an exit status".into(),
                }),
                Failure::Lost,
            ),
            (
                "ssh timeout",
                app(AppError::SshTimeout {
                    host: host(),
                    timeout_sec: 30,
                }),
                Failure::Lost,
            ),
            (
                "ssh command failed",
                app(AppError::SshExec {
                    command: "rm".into(),
                }),
                Failure::Refused,
            ),
            (
                "refused locally",
                anyhow::anyhow!("path traversal not allowed: .."),
                Failure::Refused,
            ),
        ];
        for (name, error, expected) in cases {
            assert_eq!(classify(&error), expected, "{name}");
        }
    }

    #[test]
    fn a_failed_retry_keeps_the_type_of_the_first_failure_and_names_both() {
        let lost = app(AppError::SshDisconnected {
            host: "fixture".into(),
            message: "ended without an exit status".into(),
        });
        let refused = app(AppError::SshAuth {
            user: "u".into(),
            host: "h".into(),
        });
        let error = retry_failed(lost, "could not reconnect", &refused);
        assert_eq!(classify(&error), Failure::Lost);
        let text = format!("{error}");
        assert!(
            text.contains("without an exit status") && text.contains("could not reconnect"),
            "{text}"
        );
    }

    #[test]
    fn a_read_label_keeps_the_type() {
        let read = label_read(io(io::ErrorKind::UnexpectedEof));
        assert!(read.to_string().starts_with("read failed: "), "{read}");
        assert_eq!(classify(&read), Failure::Lost);
    }

    #[test]
    fn the_first_typed_cause_decides_even_under_added_context() {
        let lost = io(io::ErrorKind::UnexpectedEof).context("failed to remove \"a\"");
        assert_eq!(classify(&lost), Failure::Lost);
        let refused =
            anyhow::Error::from(AgentRefused("write failed: denied".into())).context("write");
        assert_eq!(classify(&refused), Failure::Refused);
    }
}
