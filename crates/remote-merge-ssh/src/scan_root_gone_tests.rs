//! 走査の途中で root_dir そのものが消えたときは、sh や find がどんな知らせを出しても、root_dir が
//! 変わったものとして答える（REQ-scan-017）。スクリプトに `rm -rf "$PWD"` を差し込んで確かめる
use std::fs;
use std::path::PathBuf;

use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::ScanScope;
use remote_merge_core::scan::raw::{Answer, CollectError, Collector, Question};

use super::*;
use crate::scan_test_shell::*;

/// スクリプトの決まった位置で root_dir を消す集め方。`recheck` なら尋ね直しの回で消す
struct Sabotaged {
    root: PathBuf,
    recheck: bool,
}

impl Collector for Sabotaged {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        let recheck = self.recheck;
        let run = |script: &str| {
            let first_pass = script.contains("q() {");
            let script = match (first_pass, recheck) {
                (true, false) => script.replacen("q \"$a\"", "rm -rf \"$PWD\"; q \"$a\"", 1),
                (false, true) => script.replacen(
                    "printf '\\000@QS\\000' >&2;",
                    "rm -rf \"$PWD\"; printf '\\000@QS\\000' >&2;",
                    1,
                ),
                _ => script.to_string(),
            };
            shell_command(&script)
                .output()
                .map(|output| (output.stdout, output.stderr))
        };
        collect_with(
            self.root.as_os_str().as_encoded_bytes(),
            questions,
            exclude,
            crate::command_limit::MAX_COMMAND_LEN,
            run,
        )
        .map_err(|failure| match failure {
            CollectFailure::Run(error) => CollectError::Failed(error.into()),
            CollectFailure::Read(message) => CollectError::Failed(anyhow::anyhow!(message)),
            CollectFailure::RootGone => CollectError::RootGone,
        })
    }
}

fn root_with_an_empty_directory() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("sub/empty")).unwrap();
    fs::write(root.join("sub/f.txt"), "f").unwrap();
    fs::write(root.join("a.txt"), "a").unwrap();
    (temp, root)
}

// @kotowari[REQ-scan-017]
#[test]
fn a_root_dir_removed_during_the_scan_is_answered_as_root_changed() {
    let (_temp, root) = root_with_an_empty_directory();
    let mut collector = Sabotaged {
        root,
        recheck: false,
    };
    let result = scan_with(&mut collector, &exclude(&[]), ScanScope::Whole);
    assert_eq!(result, "Err(Failure(RootChanged))");
}

// @kotowari[REQ-scan-017]
#[test]
fn a_root_dir_removed_before_the_second_question_is_answered_as_root_changed() {
    let (_temp, root) = root_with_an_empty_directory();
    let mut collector = Sabotaged {
        root,
        recheck: true,
    };
    let result = scan_with(&mut collector, &exclude(&[]), ScanScope::Whole);
    assert_eq!(result, "Err(Failure(RootChanged))");
}
