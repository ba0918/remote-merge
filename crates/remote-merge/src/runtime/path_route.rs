//! パスの確認をどの経路で行うか（docs/ir/paths/routes.md の REQ-paths-010）。

use crate::agent::protocol::PATH_RESOLUTION_PROTOCOL_VERSION;

/// パスの確認の経路
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathInspectionRoute {
    Agent,
    Ssh,
    /// 確認せずに止める。値は利用者に示すエラーの文言
    RedeployAgent(String),
}

/// 交渉したエージェントの通信の版（エージェントがなければ `None`）と sudo を使うかから選ぶ。
/// 古いエージェントは古い辿り方をするので使わない。sudo を使うサーバでは SSH の経路が
/// 読み書きをする root と違う利用者で動くので、SSH にも回さずエージェントの置き直しを求める
pub fn path_inspection_route(agent_protocol: Option<u32>, sudo: bool) -> PathInspectionRoute {
    match agent_protocol {
        Some(version) if version >= PATH_RESOLUTION_PROTOCOL_VERSION => PathInspectionRoute::Agent,
        Some(version) if sudo => PathInspectionRoute::RedeployAgent(format!(
            "the agent speaks protocol v{version}, which cannot inspect paths with sudo = true; \
             redeploy the agent (protocol v{PATH_RESOLUTION_PROTOCOL_VERSION} or later)"
        )),
        _ => PathInspectionRoute::Ssh,
    }
}
