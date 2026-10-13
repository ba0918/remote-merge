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
        io(ScanFailure::RootUnreadable),
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
