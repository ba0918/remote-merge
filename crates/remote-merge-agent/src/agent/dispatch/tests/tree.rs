//! Ping・Shutdown・ListTree

use super::support::*;

// ── Ping / Shutdown ──

#[test]
fn ping_returns_pong() {
    let (_tmp, mut d) = setup();
    let resps = d.dispatch(AgentRequest::Ping).unwrap();
    assert_eq!(single(resps), AgentResponse::Pong);
}

#[test]
fn shutdown_returns_none() {
    let (_tmp, mut d) = setup();
    let resp = d.dispatch(AgentRequest::Shutdown);
    assert!(resp.is_none());
}

// ── ListTree ──

#[test]
fn list_tree_basic() {
    let (tmp, mut d) = setup();
    fs::write(tmp.path().join("a.txt"), "hello").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/b.txt"), "world").unwrap();

    let resps = d.dispatch(list_tree(ScanScope::Whole)).unwrap();

    // 全チャンクからエントリを収集
    let mut all_paths: Vec<String> = Vec::new();
    let mut last_count = 0;
    for resp in &resps {
        match resp {
            AgentResponse::TreeChunk {
                entries, is_last, ..
            } => {
                all_paths.extend(entries.iter().map(|n| n.path.clone()));
                if *is_last {
                    last_count += 1;
                }
            }
            other => panic!("expected TreeChunk, got {other:?}"),
        }
    }
    // 最後のチャンクだけ is_last=true
    assert_eq!(last_count, 1, "exactly one chunk should have is_last=true");
    assert!(resps
        .last()
        .is_some_and(|r| matches!(r, AgentResponse::TreeChunk { is_last: true, .. })));

    let paths: Vec<&str> = all_paths.iter().map(|s| s.as_str()).collect();
    assert!(paths.contains(&"a.txt"));
    // ディレクトリも一覧の項目に載る
    assert!(paths.contains(&"sub"));
    assert!(paths.contains(&"sub/b.txt"));
}

#[test]
fn list_tree_empty_dir_returns_single_chunk() {
    let (_tmp, mut d) = setup();

    let resps = d.dispatch(list_tree(ScanScope::Whole)).unwrap();

    assert_eq!(resps.len(), 1, "empty dir should return exactly one chunk");
    match &resps[0] {
        AgentResponse::TreeChunk {
            entries,
            is_last,
            truncated,
            ..
        } => {
            assert!(entries.is_empty());
            assert!(*is_last);
            assert!(!*truncated);
        }
        other => panic!("expected TreeChunk, got {other:?}"),
    }
}

#[test]
fn list_tree_multi_chunk() {
    let (tmp, mut d) = setup();
    // 分け方は tree_listing のテストで確かめるので、ここでは 3 ファイルで dispatch の経路を通す
    for i in 0..3 {
        fs::write(tmp.path().join(format!("f{i}.txt")), "x").unwrap();
    }

    let resps = d.dispatch(list_tree(ScanScope::Whole)).unwrap();

    // 全チャンクのうち最後だけ is_last=true であること
    let last_flags: Vec<bool> = resps
        .iter()
        .map(|r| match r {
            AgentResponse::TreeChunk { is_last, .. } => *is_last,
            other => panic!("expected TreeChunk, got {other:?}"),
        })
        .collect();

    let true_count = last_flags.iter().filter(|&&b| b).count();
    assert_eq!(
        true_count, 1,
        "exactly one chunk should have is_last=true, flags={last_flags:?}"
    );
    assert!(
        *last_flags.last().unwrap(),
        "the last chunk should have is_last=true"
    );

    // 全エントリが収集できること
    let total_entries: usize = resps
        .iter()
        .map(|r| match r {
            AgentResponse::TreeChunk { entries, .. } => entries.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(total_entries, 3);
}
