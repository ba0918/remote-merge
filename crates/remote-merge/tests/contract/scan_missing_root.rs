#![cfg(unix)]
//! root_dir がないとき、全体の走査と部分の走査のどちらも走査の時点で止まり、何も書かないこと
//! （docs/ir/scan/listing.md の REQ-scan-015、limits.md の REQ-scan-009）を、ローカルと
//! エージェントの二つの経路で確かめる契約テスト。
//!
//! 段 1 のレビューで、エージェントの全体の走査が root_dir がなくても空の完全な一覧を返し、
//! root_dir 直下のファイルの merge・--delete・status・パスなしの diff が root_dir のないことに
//! 気づかなかった（docs/decision/records/2026-10-12-foundation-2.md の C9）。

use std::fs;
use std::process::Output;

use super::agent_support::{AgentFixture, Setup};
use super::server_faults::Faults;

/// 走査する経路
#[derive(Debug, Clone, Copy)]
pub(super) enum Route {
    Local,
    Agent,
}

pub(super) const ROUTES: [Route; 2] = [Route::Local, Route::Agent];

/// 経路ごとの、試す側の root_dir を持つ偽サーバ。試す側はローカルの経路なら local、ほかは remote
pub(super) struct Sides {
    pub fixture: AgentFixture,
    pub route: Route,
}

impl Sides {
    pub(super) async fn new(route: Route) -> Self {
        let fixture = AgentFixture::new(Setup::default(), Faults::default()).await;
        Self { fixture, route }
    }

    /// 試す側の root_dir
    pub(super) fn tested_root(&self) -> std::path::PathBuf {
        match self.route {
            Route::Local => self.fixture.local(),
            _ => self.fixture.remote(),
        }
    }

    /// 試す側でない側の root_dir
    pub(super) fn other_root(&self) -> std::path::PathBuf {
        match self.route {
            Route::Local => self.fixture.remote(),
            _ => self.fixture.local(),
        }
    }

    /// 試す側の名前（--left・--right に渡すもの）
    fn tested(&self) -> &'static str {
        match self.route {
            Route::Local => "local",
            _ => "fixture",
        }
    }

    fn other(&self) -> &'static str {
        match self.route {
            Route::Local => "fixture",
            _ => "local",
        }
    }

    /// 試す側を書き込み先（右）にして動かす
    pub(super) fn run_into_tested(&self, args: &[&str]) -> Output {
        self.fixture
            .run_with_sides(args, &["--left", self.other(), "--right", self.tested()])
    }

    /// 試す側を読み込み元（左）にして動かす
    pub(super) fn run_from_tested(&self, args: &[&str]) -> Output {
        self.fixture
            .run_with_sides(args, &["--left", self.tested(), "--right", self.other()])
    }

    /// 両側に `path` と "sub/g.txt" を置き、試す側の root_dir を消す
    fn without_tested_root(&self, path: &str) {
        self.fixture.put(path, Some("left\n"), Some("right\n"));
        self.fixture
            .put("sub/g.txt", Some("left\n"), Some("right\n"));
        fs::remove_dir_all(self.tested_root()).unwrap();
    }
}

/// エラーで止まり、試す側の root_dir を作らず、もう一方も書き換えていない
fn assert_stopped(sides: &Sides, output: &Output, command: &[&str], other_content: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "{:?} {command:?}: {output:?}",
        sides.route
    );
    assert!(
        !sides.tested_root().exists(),
        "{:?} {command:?}: {output:?}",
        sides.route
    );
    let other = fs::read_to_string(sides.other_root().join("f.txt")).unwrap();
    assert_eq!(other, other_content, "{:?} {command:?}", sides.route);
    if !matches!(sides.route, Route::Local) {
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("Remote root_dir not found: fixture:"),
            "{:?} {command:?}: {output:?}",
            sides.route
        );
    }
}

/// 試す側を書き込み先にした、root_dir 直下のファイルと全体を扱うコマンド
const INTO_MISSING_ROOT: [&[&str]; 6] = [
    &["merge", "f.txt"],
    &["merge", "f.txt", "--dry-run"],
    &["merge", "f.txt", "--delete", "--force"],
    &["sync", ".", "--force"],
    &["sync", ".", "--dry-run"],
    &["merge", "sub/"],
];

/// 試す側を読み込み元にしたコマンド
const FROM_MISSING_ROOT: [&[&str]; 5] = [
    &["merge", "f.txt"],
    &["merge", "sub/", "--force"],
    &["sync", ".", "--delete", "--force"],
    &["status"],
    &["diff"],
];

// @kotowari[REQ-scan-015, REQ-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_destination_root_dir_stops_every_scan_before_writing() {
    for route in ROUTES {
        for command in INTO_MISSING_ROOT {
            let sides = Sides::new(route).await;
            sides.without_tested_root("f.txt");
            let output = sides.run_into_tested(command);
            let left = match route {
                Route::Local => "right\n",
                _ => "left\n",
            };
            assert_stopped(&sides, &output, command, left);
        }
    }
}

// @kotowari[REQ-scan-015, REQ-scan-009]
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_source_root_dir_stops_every_scan_before_writing() {
    for route in ROUTES {
        for command in FROM_MISSING_ROOT {
            let sides = Sides::new(route).await;
            sides.without_tested_root("f.txt");
            let output = sides.run_from_tested(command);
            let other = match route {
                Route::Local => "right\n",
                _ => "left\n",
            };
            assert_stopped(&sides, &output, command, other);
        }
    }
}
