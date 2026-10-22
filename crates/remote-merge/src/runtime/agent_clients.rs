//! つないだエージェントのクライアント

#[cfg(unix)]
use crate::agent::ssh_transport::TransportGuard;

use super::core::{BoxedAgentClient, CoreRuntime};

/// つないだエージェント。転送のブリッジスレッドのガードはクライアントと一緒に持ち、クライアントの後に
/// 捨ててスレッドを止める
pub(crate) struct ConnectedAgent {
    pub(crate) client: BoxedAgentClient,
    /// 試験用のエージェントは転送を通さずにつなぐので、ガードを持たない
    #[cfg(unix)]
    pub(crate) _guard: Option<TransportGuard>,
}

impl CoreRuntime {
    /// Agent が利用可能か（接続済みか）を返す
    pub fn has_agent(&self, server_name: &str) -> bool {
        self.agent_clients.contains_key(server_name)
    }

    /// Agent を切断する
    pub(crate) fn disconnect_agent(&mut self, server_name: &str) {
        if let Some(mut agent) = self.agent_clients.remove(server_name)
            && let Err(e) = agent.client.shutdown()
        {
            tracing::debug!("Agent shutdown error for {}: {}", server_name, e);
        }
    }
}
