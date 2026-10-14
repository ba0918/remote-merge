//! つないだエージェントのクライアント

use std::sync::{Arc, Mutex};

use super::core::{BoxedAgentClient, CoreRuntime};

impl CoreRuntime {
    /// Agent が利用可能か（接続済みか）を返す
    pub fn has_agent(&self, server_name: &str) -> bool {
        self.agent_clients.contains_key(server_name)
    }

    /// Agent クライアントの Arc<Mutex<>> を取得する
    pub fn get_agent(&self, server_name: &str) -> Option<Arc<Mutex<BoxedAgentClient>>> {
        self.agent_clients.get(server_name).cloned()
    }

    /// Agent を切断する
    pub fn disconnect_agent(&mut self, server_name: &str) {
        if let Some(client_arc) = self.agent_clients.remove(server_name)
            && let Ok(mut client) = client_arc.lock()
            && let Err(e) = client.shutdown()
        {
            tracing::debug!("Agent shutdown error for {}: {}", server_name, e);
        }
        // TransportGuard を drop してブリッジスレッドをシャットダウン
        #[cfg(unix)]
        self.transport_guards.remove(server_name);
    }
}
