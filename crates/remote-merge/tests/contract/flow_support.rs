#![cfg(unix)]
//! 比べる流れと経路の操作を、ローカルとエージェントの二つの経路で直接呼ぶ契約テストの道具（子の
//! プロセスの側）。
//!
//! 経路ごとに左右と参照先の root_dir を作り、左はいつもローカル、右（サーバ "fixture"）と参照先
//! （サーバ "reference"）は調べる経路（ローカル・エージェント）にする。
//! 製品の実行時処理は HOME の known_hosts を読み書きするので、HOME を一時ディレクトリにしたテストの
//! プロセスで動かす（path_operations_agree と同じ形）。親の側は flow_routes にある。

use std::fs;
use std::path::{Path, PathBuf};

use remote_merge::runtime::{CompareSides, CoreRuntime, RuntimeTargets};
use remote_merge::side::Side;

pub(super) use super::flow_routes::FlowRoutes;

pub(super) const CONFIG: &str = "REMOTE_MERGE_FLOW_CONFIG";
pub(super) const ROUTE: &str = "REMOTE_MERGE_FLOW_ROUTE";
pub(super) const REQUEST: &str = "REMOTE_MERGE_FLOW_REQUEST";
pub(super) const RESULT: &str = "REMOTE_MERGE_FLOW_RESULT";
pub(super) const HOME: &str = "REMOTE_MERGE_FLOW_HOME";

/// 右の側と参照先の経路
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    Local,
    Agent,
}

pub(super) const ROUTES: [Route; 2] = [Route::Local, Route::Agent];

impl Route {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Agent => "agent",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "local" => Self::Local,
            "agent" => Self::Agent,
            other => panic!("unknown route {other}"),
        }
    }
}

/// 一つの経路の左右と参照先の root_dir
pub(super) struct Roots {
    pub(super) left: PathBuf,
    pub(super) right: PathBuf,
    pub(super) reference: PathBuf,
}

impl Roots {
    pub(super) fn of(home: &Path, route: Route) -> Self {
        let base = home.join(route.name());
        Self {
            left: base.join("left"),
            right: base.join("right"),
            reference: base.join("ref"),
        }
    }
}

/// 子のプロセスで受けた求め
pub(super) struct Child {
    pub(super) runtime: CoreRuntime,
    pub(super) request: String,
    pub(super) roots: Roots,
    /// コマンドをこの経路で動かすための設定と差し替え
    pub(super) config: remote_merge::config::AppConfig,
    pub(super) targets: RuntimeTargets,
    result: PathBuf,
}

impl Child {
    pub(super) fn right(&self) -> Side {
        Side::Remote("fixture".into())
    }

    /// 左（ローカル）と右と、`with_ref` なら参照先
    pub(super) fn sides(&self, with_ref: bool) -> CompareSides {
        CompareSides {
            left: Side::Local,
            right: self.right(),
            reference: with_ref.then(|| Side::Remote("reference".into())),
        }
    }

    /// 答えを親のプロセスに返す
    pub(super) fn answer(self, text: &str) {
        fs::write(&self.result, text).unwrap();
    }
}

/// 子のプロセスなら、設定を読んで右と参照先の経路につないだ実行時処理と求めを返す
pub(super) fn child() -> Option<Child> {
    let config = std::env::var(CONFIG).ok()?;
    let route = Route::from_name(&std::env::var(ROUTE).unwrap());
    let roots = Roots::of(Path::new(&std::env::var(HOME).unwrap()), route);
    let config =
        remote_merge::config::load_config_from_paths(Some(Path::new(&config)), None).unwrap();
    let targets = match route {
        Route::Local => RuntimeTargets::production()
            .with_local("fixture", &roots.right)
            .with_local("reference", &roots.reference),
        Route::Agent => RuntimeTargets::production(),
    };
    let mut runtime = CoreRuntime::with_targets(config.clone(), targets.clone());
    for server in ["fixture", "reference"] {
        if route != Route::Local {
            runtime.connect(server).unwrap();
        }
        // 調べる経路を通っていることを確かめる
        assert_eq!(runtime.has_agent(server), route == Route::Agent);
    }
    Some(Child {
        runtime,
        request: std::env::var(REQUEST).unwrap(),
        roots,
        config,
        targets,
        result: PathBuf::from(std::env::var(RESULT).unwrap()),
    })
}
