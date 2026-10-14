#![cfg(unix)]
//! merge・sync が指定したディレクトリの下だけを走査するときも（TBL-scan-001）、exclude と include
//! を走査の起点からではなく root_dir からのパスに当てること（docs/ir/scan/limits.md の
//! REQ-scan-019）を、読み込み元をローカル・SSH・エージェントの三つの経路にして確かめる契約テスト。

use std::fs;

use super::scan_missing_root::{ROUTES, Sides};

/// 設定に [filter] を足す
fn with_filter(sides: &Sides, filter: &str) {
    let mut config = fs::read_to_string(&sides.fixture.config).unwrap();
    config.push_str(&format!("[filter]\n{filter}\n"));
    fs::write(&sides.fixture.config, config).unwrap();
}

/// 読み込み元（試す側）にだけ `path` を置く
fn put_source(sides: &Sides, path: &str) {
    let file = sides.tested_root().join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, "new\n").unwrap();
}

fn written(sides: &Sides, path: &str) -> bool {
    sides.other_root().join(path).exists()
}

// @kotowari[REQ-scan-019, REQ-config-022]
#[tokio::test(flavor = "multi_thread")]
async fn a_path_pattern_excludes_below_a_named_directory_on_every_route() {
    for command in ["merge", "sync"] {
        for route in ROUTES {
            let sides = Sides::new(route).await;
            with_filter(&sides, "exclude = [\"sub/skip\"]");
            put_source(&sides, "sub/keep.txt");
            put_source(&sides, "sub/skip/x.txt");
            fs::create_dir_all(sides.other_root()).unwrap();
            let output = sides.run_from_tested(&[command, "sub/", "--force"]);
            assert!(
                written(&sides, "sub/keep.txt"),
                "{command} {route:?}: {output:?}"
            );
            assert!(
                !written(&sides, "sub/skip/x.txt"),
                "{command} {route:?}: {output:?}"
            );
        }
    }
}

// @kotowari[REQ-scan-019, REQ-config-004]
#[tokio::test(flavor = "multi_thread")]
async fn include_narrows_a_named_directory_from_the_root_dir_on_every_route() {
    for command in ["merge", "sync"] {
        for route in ROUTES {
            let sides = Sides::new(route).await;
            with_filter(&sides, "include = [\"src/in\"]");
            put_source(&sides, "src/in/a.txt");
            put_source(&sides, "src/out/b.txt");
            put_source(&sides, "docs/c.txt");
            fs::create_dir_all(sides.other_root()).unwrap();
            for dir in ["src/", "docs/"] {
                let output = sides.run_from_tested(&[command, dir, "--force"]);
                assert!(
                    !written(&sides, "docs/c.txt"),
                    "{command} {route:?}: {output:?}"
                );
            }
            assert!(written(&sides, "src/in/a.txt"), "{command} {route:?}");
            assert!(!written(&sides, "src/out/b.txt"), "{command} {route:?}");
        }
    }
}
