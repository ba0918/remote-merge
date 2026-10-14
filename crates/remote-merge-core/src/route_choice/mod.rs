//! エージェントを使うか、使えずに止めるか（docs/ir/ssh/fallback.md の TBL-ssh-001）。
//!
//! リモートのファイルの操作はエージェントだけで行う。サーバにつなぐときに一度だけ呼び、その後は
//! 選び直さない（REQ-ssh-010・REQ-ssh-024）。

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

/// 選んだ扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteChoice {
    Agent,
    /// ファイルを読み書きする前に、`reason` を示す接続のエラーで止める（REQ-ssh-005）
    Stop {
        reason: String,
    },
}

/// エージェントの状態から、エージェントで操作するか止めるかを決める。sudo の指定にはよらない
pub fn choose_route(state: &AgentState) -> RouteChoice {
    match state {
        AgentState::Ready => RouteChoice::Agent,
        AgentState::Disabled
        | AgentState::UnsafeLocation { .. }
        | AgentState::VersionMismatch { .. }
        | AgentState::CannotDeploy { .. }
        | AgentState::CannotStart { .. } => RouteChoice::Stop {
            reason: format!(
                "remote files are handled only through the agent, and the agent cannot be used: {}",
                why_unusable(state)
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
        AgentState::Disabled => {
            "[agent] enabled is false. Set [agent] enabled = true in your config.".to_string()
        }
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
