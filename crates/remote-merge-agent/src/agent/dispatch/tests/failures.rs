//! 操作の失敗は種類で返る（REQ-paths-020）

use super::support::*;

fn read(d: &mut Dispatcher, path: &str, size_limit: Option<u64>) -> FileReadResult {
    let responses = d
        .dispatch(AgentRequest::ReadFiles {
            paths: vec![path.into()],
            chunk_size_limit: 4096,
            size_limit,
        })
        .unwrap();
    collect_file_contents(responses).remove(0)
}

fn failed(path: &str, failure: WireFailure) -> FileReadResult {
    FileReadResult::Failed {
        path: path.into(),
        failure,
    }
}

#[test]
fn a_read_answers_the_kind_of_its_failure() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("f"), "x").unwrap();
    assert_eq!(
        read(&mut d, "missing.txt", None),
        failed("missing.txt", WireFailure::NotFound)
    );
    assert_eq!(
        read(&mut d, "f/a.txt", None),
        failed("f/a.txt", WireFailure::NotADirectory)
    );
}

#[test]
fn a_file_over_the_size_limit_is_answered_without_its_content() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("big"), vec![0u8; 11]).unwrap();
    assert_eq!(
        read(&mut d, "big", Some(10)),
        failed(
            "big",
            WireFailure::TooLarge {
                size: 11,
                limit: 10
            }
        )
    );
    assert!(matches!(
        read(&mut d, "big", None),
        FileReadResult::Ok { .. }
    ));
}

#[test]
fn a_change_answers_the_kind_of_its_failure() {
    let (_tmp, mut d) = setup();
    let removed = single(
        d.dispatch(AgentRequest::Remove {
            path: "missing".into(),
        })
        .unwrap(),
    );
    assert_eq!(
        removed,
        AgentResponse::Changed {
            failure: Some(WireFailure::NotFound)
        }
    );
    let widened = single(
        d.dispatch(AgentRequest::Chmod {
            path: "missing".into(),
            mode: 0o4755,
        })
        .unwrap(),
    );
    assert!(
        matches!(&widened, AgentResponse::Changed { failure: Some(WireFailure::Io { reason }) } if reason.contains("0o777")),
        "{widened:?}"
    );
}

#[test]
fn the_link_of_a_missing_entry_is_a_not_found_failure() {
    let (_tmp, mut d) = setup();
    let answer = single(
        d.dispatch(AgentRequest::ReadEntryLink {
            path: "gone".into(),
        })
        .unwrap(),
    );
    assert_eq!(
        answer,
        AgentResponse::EntryLink {
            result: crate::agent::protocol::AgentEntryLink::Failed {
                failure: WireFailure::NotFound
            }
        }
    );
}
