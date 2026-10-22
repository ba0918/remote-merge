#![cfg(unix)]
//! 状態を集めた後、中身を読む前に消えたファイルを、diff が片側だけとも差分のあるファイルともせず、
//! 判定できないパスとして示すこと（docs/ir/compare/flow.md の REQ-compare-003）の CLI の契約テスト。
//! エージェントの偽サーバが、右のハッシュの問いを受ける前にファイルのあるディレクトリを消す。

use super::agent_support::{AgentFixture, Setup};
use super::common::stdout_json;
use super::server_faults::Faults;
use super::server_removal::{Removal, When};

// @kotowari[REQ-compare-003, REQ-compare-013, EX-compare-003]
#[tokio::test(flavor = "multi_thread")]
async fn ex_compare_003_a_file_removed_before_it_was_read_is_reported_as_undecidable() {
    let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
    fixture.put("d/a.txt", Some("alpha\n"), Some("bravo\n"));
    fixture.server.faults().removal.arm(Removal {
        agent_request: "HashContents",
        occurrence: 1,
        when: When::Before,
        dir: fixture.remote().join("d"),
        replace_with_file: false,
    });

    let output = fixture.run(&["diff"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let result = stdout_json(&output);
    let files = result["files"].as_array().unwrap();
    assert!(files.is_empty(), "{result}");
    let errors = result["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{result}");
    assert_eq!(errors[0]["path"], "d/a.txt", "{result}");
    let reason = errors[0]["reason"].as_str().unwrap();
    assert!(reason.starts_with("read failed: right: "), "{reason}");
}
