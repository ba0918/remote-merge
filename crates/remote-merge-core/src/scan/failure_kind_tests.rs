use super::*;
use crate::op_failure::Disconnection;
use crate::scan::outcome::ReadCause;

fn path() -> String {
    "d/a.txt".into()
}

// @kotowari[REQ-compare-015, REQ-compare-013]
#[test]
fn each_scan_failure_carries_the_kind_of_operation_failure_it_is() {
    let io = |failure: ScanFailure| {
        let text = failure.to_string();
        (failure, OpFailure::Io(text))
    };
    let cases = [
        (ScanFailure::RootMissing, OpFailure::RootGone),
        (ScanFailure::RootChanged, OpFailure::RootGone),
        (ScanFailure::RootNotDirectory, OpFailure::RootGone),
        (
            ScanFailure::RootUnreadable {
                cause: ReadCause::PermissionDenied,
            },
            OpFailure::PermissionDenied,
        ),
        (
            ScanFailure::StartBlocked { path: path() },
            OpFailure::NotADirectory,
        ),
        (
            ScanFailure::StartUnreadable {
                path: path(),
                cause: ReadCause::PermissionDenied,
            },
            OpFailure::PermissionDenied,
        ),
        (
            ScanFailure::UnreadableDirectory {
                path: path(),
                cause: ReadCause::PermissionDenied,
            },
            OpFailure::PermissionDenied,
        ),
        (
            ScanFailure::InconsistentAnswer { detail: "x".into() },
            OpFailure::Disconnected(Disconnection::NoAnswer(
                "The scan answers did not match the questions: x".into(),
            )),
        ),
        (
            ScanFailure::CollectFailed {
                message: "m".into(),
            },
            OpFailure::Io("Cannot collect the entries to scan: m".into()),
        ),
        io(ScanFailure::RootUnreadable {
            cause: ReadCause::Other,
        }),
        io(ScanFailure::StartUnreadable {
            path: path(),
            cause: ReadCause::Other,
        }),
        io(ScanFailure::UnreadableDirectory {
            path: path(),
            cause: ReadCause::Other,
        }),
        io(ScanFailure::UnresolvableLink { path: path() }),
        io(ScanFailure::UnreadableLinkTarget { path: path() }),
        io(ScanFailure::UnreadableTime { path: path() }),
        io(ScanFailure::ChangedDuringScan { path: path() }),
    ];
    for (failure, kind) in cases {
        assert_eq!(failure_kind(&failure), kind, "{failure:?}");
    }
}

#[test]
fn an_operation_failure_inside_a_scan_error_keeps_its_kind() {
    use crate::op_failure::{OpError, Operation, Tool};
    let tool = anyhow::Error::from(OpError::new(
        Operation::Scan,
        "/root".to_string(),
        OpFailure::ToolMissing(Tool::Cat),
    ))
    .context("scan");
    assert_eq!(
        failure_of_scan_error(&tool),
        Some(OpFailure::ToolMissing(Tool::Cat))
    );
    let scan = anyhow::Error::from(ScanFailure::RootMissing);
    assert_eq!(failure_of_scan_error(&scan), Some(OpFailure::RootGone));
    assert_eq!(failure_of_scan_error(&anyhow::anyhow!("other")), None);
}

// 経路ごとの文言の誤りにしても、走査を止めた失敗の種類は型で読める
// @kotowari[REQ-compare-015, REQ-merge-044]
#[test]
fn a_scan_failure_shown_with_the_route_message_keeps_its_kind() {
    use crate::scan::outcome::ScanStopped;
    let shown = anyhow::Error::from(ScanStopped::new(
        ScanFailure::RootChanged,
        anyhow::anyhow!("root_dir changed during the scan: server:/srv"),
    ))
    .context("comparing");
    assert_eq!(failure_of_scan_error(&shown), Some(OpFailure::RootGone));
    assert_eq!(
        format!(
            "{:#}",
            anyhow::Error::from(ScanStopped::new(
                ScanFailure::RootMissing,
                anyhow::anyhow!("Remote root_dir not found: server:/srv"),
            ))
        ),
        "Remote root_dir not found: server:/srv"
    );
}
