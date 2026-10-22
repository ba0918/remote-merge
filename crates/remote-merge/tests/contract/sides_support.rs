#![cfg(unix)]
//! 走査する側をローカルとエージェントの二つの経路にして試す契約テストの道具。
//!
//! 一つの [`AgentFixture`] の local と remote の root_dir のうち、試す経路の側を「試す側」とし、
//! もう一方を相手にしてコマンドを動かす。

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

/// 経路ごとの、試す側の root_dir を持つ偽サーバ。試す側はローカルの経路なら local、エージェントの経路なら remote
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
            Route::Agent => self.fixture.remote(),
        }
    }

    /// 試す側でない側の root_dir
    pub(super) fn other_root(&self) -> std::path::PathBuf {
        match self.route {
            Route::Local => self.fixture.remote(),
            Route::Agent => self.fixture.local(),
        }
    }

    /// 試す側の名前（--left・--right に渡すもの）
    fn tested(&self) -> &'static str {
        match self.route {
            Route::Local => "local",
            Route::Agent => "fixture",
        }
    }

    fn other(&self) -> &'static str {
        match self.route {
            Route::Local => "fixture",
            Route::Agent => "local",
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
}
