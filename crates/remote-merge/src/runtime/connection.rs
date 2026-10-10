//! リモートの操作の失敗の種類の判定（docs/ir/ssh/reconnect.md と、用語の `接続断`）。
//!
//! 判定はここの一つだけで行う。エラーを文字列に変える前に、エラーの型で判定する。

use std::io;

use crate::agent::client::{AgentNotSent, AgentRefused};
use crate::error::AppError;

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
        if cause.is::<AgentNotSent>() || cause.is::<ProcessingStopped>() {
            return Failure::NotSent;
        }
        if let Some(error) = cause.downcast_ref::<AppError>() {
            return match error {
                AppError::SshConnection { .. } => Failure::NotSent,
                AppError::SshDisconnected { .. } | AppError::SshTimeout { .. } => Failure::Lost,
                _ => Failure::Refused,
            };
        }
        // エージェントとの通信路の読み書きの失敗は、どの種類でも応答を受け取れなかったことになる
        if cause.is::<io::Error>() {
            return Failure::Lost;
        }
    }
    Failure::Refused
}

/// 変更の失敗の文言に、結果が分からないことか、手を付けていないことを示す（REQ-ssh-022）。
/// 種類は変えない
pub fn label_change(error: anyhow::Error) -> anyhow::Error {
    if is_stopped(&error) {
        return error;
    }
    let failure = classify(&error);
    let message = match failure {
        Failure::Lost => format!(
            "result unknown: the connection was lost before the change was confirmed: {error:#}"
        ),
        Failure::NotSent => format!(
            "not attempted: the connection was already unusable, so the change was not sent: {error:#}"
        ),
        Failure::Refused => return error,
    };
    Labeled { failure, message }.into()
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

fn is_stopped(error: &anyhow::Error) -> bool {
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
            (
                "agent error",
                AgentRefused("agent error: x".into()).into(),
                Failure::Refused,
            ),
            (
                "closed before sending",
                AgentNotSent.into(),
                Failure::NotSent,
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
    fn labels_tell_an_unknown_result_from_an_untouched_file_and_keep_the_type() {
        let lost = label_change(io(io::ErrorKind::UnexpectedEof));
        assert!(lost.to_string().starts_with("result unknown: "), "{lost}");
        assert_eq!(classify(&lost), Failure::Lost);
        let unsent = label_change(AgentNotSent.into());
        assert!(
            unsent.to_string().starts_with("not attempted: "),
            "{unsent}"
        );
        let stopped: anyhow::Error = ProcessingStopped {
            server: "fixture".into(),
            reason: "lost".into(),
        }
        .into();
        let stopped_text = stopped.to_string();
        assert_eq!(label_change(stopped).to_string(), stopped_text);
        let refused = label_change(AgentRefused("write failed: denied".into()).into());
        assert_eq!(refused.to_string(), "write failed: denied");
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
