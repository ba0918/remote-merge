//! エージェントを使うか SSH にするか（docs/ir/ssh/fallback.md の TBL-ssh-001）。
//!
//! サーバにつなぐときに一度だけ呼び、その後は選び直さない（REQ-ssh-010・REQ-ssh-024）。

/// つなぐときに分かったエージェントの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    /// [agent] enabled が true で、配置先が安全で、起動でき、通信の版が手元と同じ
    Ready,
    /// [agent] enabled が false
    Disabled,
    /// 配置先が REQ-ssh-014 か REQ-ssh-017 に当たる。`location` は配置先のパス
    UnsafeLocation {
        location: String,
    },
    /// 接続のときに答えた通信の版が手元と違う
    VersionMismatch {
        remote: u32,
        local: u32,
    },
    CannotDeploy {
        reason: String,
    },
    CannotStart {
        reason: String,
    },
}

/// 選んだ経路
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteChoice {
    Agent,
    /// SSH の経路で行う。`warning` は標準エラーに出す英語の警告
    Ssh {
        warning: Option<String>,
    },
    /// どちらも使わずに、ファイルを読み書きする前に接続のエラーで止める
    Stop {
        reason: String,
    },
}

/// sudo の指定とエージェントの状態から経路を選ぶ
pub fn choose_route(sudo: bool, state: &AgentState) -> RouteChoice {
    match (sudo, state) {
        (_, AgentState::Ready) => RouteChoice::Agent,
        (false, AgentState::Disabled) => RouteChoice::Ssh { warning: None },
        (false, unusable) => RouteChoice::Ssh {
            warning: Some(format!(
                "not using the agent and continuing over SSH: {}",
                why_unusable(unusable)
            )),
        },
        (true, AgentState::Disabled) => RouteChoice::Stop {
            reason: "sudo = true requires agent to be enabled. Set [agent] enabled = true in your config."
                .to_string(),
        },
        (true, unusable) => RouteChoice::Stop {
            reason: format!(
                "sudo = true requires the agent, and the agent cannot be used: {}",
                why_unusable(unusable)
            ),
        },
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&why_unusable(self))
    }
}

/// エージェントを使えない理由の文
fn why_unusable(state: &AgentState) -> String {
    match state {
        AgentState::Ready => "the agent is ready".to_string(),
        AgentState::Disabled => "[agent] enabled is false".to_string(),
        AgentState::UnsafeLocation { location } => format!(
            "agent deploy directory {location} could not be confirmed safe: it must be a \
             directory, not a symlink, owned by the login user (root with sudo), and every \
             directory on the path to it, including directories holding intermediate symlinks, \
             must be owned by root or the login user (root with sudo) and not writable by group \
             or others unless it has the sticky bit"
        ),
        AgentState::VersionMismatch { remote, local } => format!(
            "the agent answered protocol version {remote}, but this remote-merge uses protocol \
             version {local}"
        ),
        AgentState::CannotDeploy { reason } => format!("the agent could not be placed: {reason}"),
        AgentState::CannotStart { reason } => format!("the agent could not be started: {reason}"),
    }
}

#[cfg(test)]
mod tests;
