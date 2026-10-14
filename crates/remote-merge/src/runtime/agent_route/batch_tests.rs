//! エージェントの経路が、まとめて尋ねるパスを一度に送る上限で分けることの単体テスト
//! （docs/remote-tools.md の「一度に送るパスは 2000 件まで」）
use std::sync::{Arc, Mutex};

use remote_merge_core::relative_path::RelPath;

use super::{AGENT_READ_BATCH_SIZE, inspect_paths};
use crate::agent::protocol::{AgentPathCheck, AgentPathInspection, AgentRequest, AgentResponse};
use crate::runtime::core::CoreRuntime;

// 書き込み先の確認のように多くのパスをまとめて確かめても、一つの要求のパスは上限までにし、答えは
// 尋ねた順に返す（通信の一つの枠を超えて、その書き込み先の変更がすべて失敗しないように）
// @kotowari[REQ-paths-008]
#[test]
fn inspecting_many_paths_sends_them_in_batches_up_to_the_limit() {
    let mut rt = CoreRuntime::new_for_test();
    let sizes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&sizes);
    crate::runtime::scripted_agent::connect_answering(&mut rt, "develop", move |request| {
        let AgentRequest::InspectPaths { paths } = request else {
            return None;
        };
        seen.lock().unwrap().push(paths.len());
        let results = paths
            .into_iter()
            .map(|path| {
                AgentPathCheck::Inspected(AgentPathInspection::Missing {
                    real_parent: format!("/root/{path}"),
                    parent_exists: true,
                })
            })
            .collect();
        Some(AgentResponse::PathInspections { results })
    });
    let total = AGENT_READ_BATCH_SIZE * 2 + 1;
    let paths: Vec<RelPath> = (0..total)
        .map(|index| RelPath::parse(&format!("d/{index}.txt")).unwrap())
        .collect();

    let answers = inspect_paths(&mut rt, "develop", &paths).unwrap();

    assert_eq!(answers.len(), total);
    let sizes = sizes.lock().unwrap().clone();
    assert!(
        sizes.iter().all(|size| *size <= AGENT_READ_BATCH_SIZE),
        "{sizes:?}"
    );
    assert_eq!(sizes.iter().sum::<usize>(), total);
}
