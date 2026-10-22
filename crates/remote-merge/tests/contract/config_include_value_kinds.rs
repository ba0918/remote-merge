#![cfg(unix)]
//! include の値に、symlink を含むディレクトリと通常ファイルを並べたとき、それぞれを値として載せ、
//! 一方の値の答えをもう一方の値に取り違えないこと（docs/ir/config/include.md の REQ-config-034・
//! 036）を、比べる片側をローカルとエージェントの二つの経路にして確かめる契約テスト。

use std::fs;
use std::os::unix::fs::symlink;

use serde_json::Value;

use super::sides_support::{ROUTES, Sides};

// @kotowari[REQ-config-034, REQ-config-036, REQ-scan-017]
#[tokio::test(flavor = "multi_thread")]
async fn a_directory_holding_a_link_and_a_file_are_both_listed_as_include_values() {
    for route in ROUTES {
        let sides = Sides::new(route).await;
        sides
            .fixture
            .put("a/x.txt", Some("left\n"), Some("right\n"));
        sides.fixture.put("b.txt", Some("left\n"), Some("right\n"));
        for root in [sides.tested_root(), sides.other_root()] {
            symlink("x.txt", root.join("a/l")).unwrap();
        }
        let mut config = fs::read_to_string(&sides.fixture.config).unwrap();
        config.push_str("[filter]\ninclude = [\"a\", \"b.txt\"]\n");
        fs::write(&sides.fixture.config, config).unwrap();

        let output = sides.run_into_tested(&["status", "--all"]);

        assert_eq!(output.status.code(), Some(1), "{route:?}: {output:?}");
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut statuses: Vec<(String, String)> = json["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap().into(),
                    f["status"].as_str().unwrap().into(),
                )
            })
            .collect();
        statuses.sort();
        let expected = [
            ("a/l", "equal"),
            ("a/x.txt", "modified"),
            ("b.txt", "modified"),
        ]
        .map(|(p, s)| (p.to_string(), s.to_string()));
        assert_eq!(statuses, expected, "{route:?}: {json}");
    }
}
