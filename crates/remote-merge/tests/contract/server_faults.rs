#![cfg(unix)]
//! 契約テストの SSH の偽サーバに起こさせる故障の設定と、エージェントとの間の中継。
//!
//! 中継は " agent --root " を含む exec を本物のエージェントのプロセスにつなぎ、フレームの境目を
//! 読んで、要求の種類と回数で決めた時点にエージェントだけ、または SSH の session ごと切る。
//! 起動の挨拶の通信の版も書き換えられる。

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use remote_merge::agent::protocol::{
    AgentRequest, AgentResponse, deserialize_request, deserialize_response,
};
use russh::server::Handle;
use russh::{ChannelId, Disconnect};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};

/// 切るのを最初の一度だけにするか、条件に合うたびに切るか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Every,
}

/// 何を切るか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutScope {
    /// SSH の session は残し、エージェントのチャネルとプロセスだけを終える
    Agent,
    /// SSH の session ごと切る
    Session,
}

/// エージェントとの間を切る時点
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutPoint {
    /// 要求をエージェントに渡し、エージェントが応答した後で、その応答を返す前
    BeforeResponse,
    /// 要求の応答（分けて返す応答は最後まで）を中継した後。次の要求を送る前に切れている状態
    AfterResponse,
    /// 応答の代わりに、長さは正しいが中身を読めないフレームを返し、エージェントとの間は切らない
    GarbledAnswer,
}

/// エージェントとの間を切る条件。`request` の種類（`AgentRequest` の名前。`None` はどの種類も）の
/// 要求の `occurrence` 番目（1 から。起動し直した後も続けて数える）で切る。`Repeat::Every` なら
/// それ以降の同じ種類の要求でも毎回切る
#[derive(Debug, Clone, Copy)]
pub struct AgentCut {
    pub request: Option<&'static str>,
    pub occurrence: usize,
    pub point: CutPoint,
    pub scope: CutScope,
    pub repeat: Repeat,
}

/// 偽サーバに起こさせる故障。既定はどれも起こさない
#[derive(Debug, Clone, Copy, Default)]
pub struct Faults {
    /// エージェントの起動の挨拶で答える通信の版
    pub handshake_version: Option<u32>,
    pub agent_cut: Option<AgentCut>,
    /// `sudo -n true` に 0 を返し、`sudo` で始まるコマンドは前置きを外して同じ利用者で実行する
    pub fake_sudo: bool,
    /// この種類（`AgentRequest` の名前）の要求をエージェントに渡さず、権限がないの失敗で答える
    pub refuse_agent: Option<&'static str>,
    /// エージェントとの間を一度切った後は、エージェントを起動させない（起動のコマンドを 1 で終える）
    pub refuse_agent_after_cut: bool,
}

/// サーバ全体（再接続した session をまたいで）で共有する故障の状態
pub struct FaultState {
    pub faults: Faults,
    /// 後から仕掛けたエージェントとの間を切る条件（`faults.agent_cut` より優先する）
    armed_agent_cut: Mutex<Option<AgentCut>>,
    /// 要求の種類ごとの数（起動し直した後も続けて数える。仕掛け直すと数え直す）
    agent_counts: Mutex<HashMap<String, usize>>,
    agent_cut_claimed: AtomicBool,
    agent_starts: AtomicUsize,
    agent_requests: Mutex<Vec<String>>,
    /// エージェントが受けた要求の中身（`Debug` の形）
    agent_request_details: Mutex<Vec<String>>,
    sudo_bin: Option<tempfile::TempDir>,
    /// 決めた回の書き込みの前にディレクトリを消す仕掛け
    pub removal: super::server_removal::ArmedRemoval,
    /// 決めた回の要求の前に書き込み先を書き換える仕掛け
    pub rewrite: super::server_removal::ArmedRewrite,
}

impl FaultState {
    pub fn new(faults: Faults) -> Arc<Self> {
        let sudo_bin = faults.fake_sudo.then(fake_sudo_bin);
        Arc::new(Self {
            faults,
            armed_agent_cut: Mutex::new(None),
            agent_counts: Mutex::new(HashMap::new()),
            agent_cut_claimed: AtomicBool::new(false),
            agent_starts: AtomicUsize::new(0),
            agent_requests: Mutex::new(Vec::new()),
            agent_request_details: Mutex::new(Vec::new()),
            sudo_bin,
            removal: Default::default(),
            rewrite: Default::default(),
        })
    }

    /// これから起動するエージェントとの間を切る条件を仕掛ける（準備の操作の後に切るため）
    pub fn arm_agent_cut(&self, cut: AgentCut) {
        *self.armed_agent_cut.lock().unwrap() = Some(cut);
        self.agent_counts.lock().unwrap().clear();
        self.agent_cut_claimed.store(false, Ordering::SeqCst);
    }

    fn agent_cut(&self) -> Option<AgentCut> {
        (*self.armed_agent_cut.lock().unwrap()).or(self.faults.agent_cut)
    }

    /// エージェントが受けた要求の種類（起動し直した後のものも続けて並ぶ）
    pub fn agent_requests(&self) -> Vec<String> {
        self.agent_requests.lock().unwrap().clone()
    }

    /// エージェントが受けた要求の中身（`Debug` の形。起動し直した後のものも続けて並ぶ）
    pub fn agent_request_details(&self) -> Vec<String> {
        self.agent_request_details.lock().unwrap().clone()
    }

    /// エージェントのプロセスを起動した回数
    pub fn agent_starts(&self) -> usize {
        self.agent_starts.load(Ordering::SeqCst)
    }

    /// エージェントの起動を拒むか（切った後に起動し直させないとき）
    pub fn refuses_agent_start(&self) -> bool {
        self.faults.refuse_agent_after_cut && self.agent_cut_claimed.load(Ordering::SeqCst)
    }

    /// PATH の先頭に置くディレクトリ（偽の sudo）
    pub fn extra_path_dirs(&self) -> Vec<&Path> {
        self.sudo_bin.iter().map(|dir| dir.path()).collect()
    }

    fn claim_agent_cut(&self, repeat: Repeat) -> bool {
        claim(&self.agent_cut_claimed, repeat)
    }
}

fn claim(claimed: &AtomicBool, repeat: Repeat) -> bool {
    match repeat {
        Repeat::Every => true,
        Repeat::Once => !claimed.swap(true, Ordering::SeqCst),
    }
}

/// 前置きの `-n` を除いて残りを同じ利用者で実行する `sudo`
fn fake_sudo_bin() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let sudo = dir.path().join("sudo");
    std::fs::write(&sudo, "#!/bin/sh\n[ \"$1\" = -n ] && shift\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&sudo, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

/// 偽の sudo のサーバで、受けたコマンドを実行する前の書き換えと、実行せずに返す答え。
///
/// 偽の sudo は root の代わりに同じ利用者で動くので、配置先の検査が期待する所有者の root（0）を
/// その利用者の uid に置き換える。設定の利用者は手元にいないので、uid と gid の問い合わせには
/// 手元の利用者の値を答える
pub enum SudoAnswer {
    Run(String),
    Reply(String),
}

pub fn fake_sudo_answer(script: &str, received: &str) -> SudoAnswer {
    if script.starts_with("id -u ") && script.contains("&& id -g ") {
        let id = |flag: &str| {
            let output = std::process::Command::new("id").arg(flag).output().unwrap();
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        return SudoAnswer::Reply(format!("{}\n{}\n", id("-u"), id("-g")));
    }
    SudoAnswer::Run(received.replace("u=0; d=", "u=$(id -u); d="))
}

/// 偽の sudo のサーバで、home のパスを含まなくてよいコマンド
pub fn is_sudo_preflight(script: &str) -> bool {
    script == "sudo -n true" || (script.starts_with("id -u ") && script.contains("&& id -g "))
}

/// PATH の先頭に `dirs` を置いた値。`dirs` が空なら None（PATH を変えない）
pub fn path_with(dirs: &[&Path]) -> Option<std::ffi::OsString> {
    if dirs.is_empty() {
        return None;
    }
    let mut paths: Vec<PathBuf> = dirs.iter().map(PathBuf::from).collect();
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    Some(std::env::join_paths(paths).unwrap())
}

// ── エージェントとの中継 ──

/// 要求の応答をどう扱うか
#[derive(Debug, Clone, Copy)]
struct Pending {
    cut: Option<(CutPoint, CutScope)>,
}

/// 一つのエージェントのチャネルの、クライアントからエージェントへの向き
pub struct AgentInbound {
    state: Arc<FaultState>,
    buffer: Vec<u8>,
    pending: Arc<Mutex<VecDeque<Pending>>>,
    stdin: Arc<tokio::sync::Mutex<Option<ChildStdin>>>,
    handle: Handle,
    channel: ChannelId,
}

impl AgentInbound {
    /// クライアントから受けたバイト列を、要求の境目で区切って記録してからエージェントに渡す（クライアント
    /// はフレームだけを送る）。拒む種類の要求は渡さずに答える。エージェントを終えた後は捨てる（handler
    /// のエラーで session ごと切らないため）
    pub async fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        while let Some(frame) = take_frame(&mut self.buffer) {
            let refused = match deserialize_request(&frame[4..]) {
                Ok(request) => {
                    let kind = request_kind(&request);
                    let refused = self.state.faults.refuse_agent == Some(kind.as_str());
                    if refused {
                        self.record(&request);
                    } else {
                        self.note(&request);
                    }
                    self.state.removal.on_agent_request(&kind);
                    self.state.rewrite.on_agent_request(&kind);
                    refused
                }
                Err(_) => false,
            };
            if refused {
                self.refuse().await;
                continue;
            }
            let mut stdin = self.stdin.lock().await;
            if let Some(writer) = stdin.as_mut()
                && writer.write_all(&frame).await.is_err()
            {
                *stdin = None;
            }
        }
    }

    /// 要求をエージェントに渡さず、権限がないの失敗の変更の結果で答える
    async fn refuse(&self) {
        let response = AgentResponse::Changed {
            failure: Some(remote_merge::agent::protocol::WireFailure::PermissionDenied),
        };
        let body = remote_merge::agent::protocol::serialize_response(&response).unwrap();
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        self.handle
            .data(self.channel, frame)
            .await
            .expect("the client waits for the answer");
    }

    /// クライアントが送り終えたので、エージェントの標準入力を閉じる
    pub async fn close(self) {
        *self.stdin.lock().await = None;
    }

    /// 受けた要求の種類と中身を記録する
    fn record(&self, request: &AgentRequest) {
        self.state
            .agent_requests
            .lock()
            .unwrap()
            .push(request_kind(request));
        self.state
            .agent_request_details
            .lock()
            .unwrap()
            .push(format!("{request:?}"));
    }

    fn note(&mut self, request: &AgentRequest) {
        let kind = request_kind(request);
        self.record(request);
        if matches!(request, AgentRequest::Shutdown) {
            return;
        }
        let (seen_kind, seen_any) = {
            let mut counts = self.state.agent_counts.lock().unwrap();
            *counts.entry(kind.clone()).or_insert(0) += 1;
            (counts[&kind], counts.values().sum::<usize>())
        };
        let cut = self.state.agent_cut().and_then(|cut| {
            let seen = match cut.request {
                Some(name) if name == kind => seen_kind,
                Some(_) => return None,
                None => seen_any,
            };
            let due = match cut.repeat {
                Repeat::Once => seen == cut.occurrence,
                Repeat::Every => seen >= cut.occurrence,
            };
            (due && self.state.claim_agent_cut(cut.repeat)).then_some((cut.point, cut.scope))
        });
        self.pending.lock().unwrap().push_back(Pending { cut });
    }
}

/// エージェントのプロセスを中継につなぐ。標準出力はクライアントに返し、故障の条件で切る
pub fn relay_agent(
    state: Arc<FaultState>,
    mut child: Child,
    channel: ChannelId,
    handle: Handle,
) -> AgentInbound {
    state.agent_starts.fetch_add(1, Ordering::SeqCst);
    let stdin = Arc::new(tokio::sync::Mutex::new(child.stdin.take()));
    let pending = Arc::new(Mutex::new(VecDeque::new()));
    let mut outbound = Outbound {
        handshake_version: state.faults.handshake_version,
        pending: Arc::clone(&pending),
        stdin: Arc::clone(&stdin),
        handle: handle.clone(),
        channel,
    };
    let mut stdout = child.stdout.take().unwrap();
    if let Some(mut stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text).await;
            if !text.is_empty() {
                eprintln!("agent stderr: {text}");
            }
        });
    }
    tokio::spawn(async move {
        let cut = outbound.run(&mut stdout).await;
        match cut {
            Some(CutScope::Agent) => {
                // 応答のすぐ後に閉じたことが届くよう、プロセスを終える前にチャネルを閉じる
                let _ = outbound.handle.eof(channel).await;
                let _ = outbound.handle.close(channel).await;
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            Some(CutScope::Session) => {
                let _ = child.start_kill();
                let _ = outbound
                    .handle
                    .disconnect(
                        Disconnect::ByApplication,
                        "connection lost".into(),
                        "".into(),
                    )
                    .await;
            }
            None => {
                let status = child
                    .wait()
                    .await
                    .ok()
                    .and_then(|status| status.code())
                    .unwrap_or(1);
                let _ = outbound
                    .handle
                    .exit_status_request(channel, status as u32)
                    .await;
                let _ = outbound.handle.eof(channel).await;
                let _ = outbound.handle.close(channel).await;
            }
        }
    });
    AgentInbound {
        state,
        buffer: Vec::new(),
        pending,
        stdin,
        handle,
        channel,
    }
}

/// エージェントからクライアントへの向き
struct Outbound {
    handshake_version: Option<u32>,
    pending: Arc<Mutex<VecDeque<Pending>>>,
    stdin: Arc<tokio::sync::Mutex<Option<ChildStdin>>>,
    handle: Handle,
    channel: ChannelId,
}

impl Outbound {
    /// エージェントの出力を終わるか切るまで中継する。切ったら切り方を返す
    async fn run(&mut self, stdout: &mut tokio::process::ChildStdout) -> Option<CutScope> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 32768];
        let mut greeted = false;
        loop {
            let count = stdout.read(&mut chunk).await.unwrap_or(0);
            if count == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..count]);
            if !greeted {
                let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
                    continue;
                };
                let line: Vec<u8> = buffer.drain(..=end).collect();
                self.send(&self.greeting(&line)).await;
                greeted = true;
            }
            while let Some(frame) = take_frame(&mut buffer) {
                if let Some(scope) = self.response(frame).await {
                    *self.stdin.lock().await = None;
                    return Some(scope);
                }
            }
        }
    }

    fn greeting(&self, line: &[u8]) -> Vec<u8> {
        match self.handshake_version {
            Some(version) => format!("remote-merge agent v{version}\n").into_bytes(),
            None => line.to_vec(),
        }
    }

    /// 応答のフレームを一つ扱う。切ったら切り方を返す
    async fn response(&mut self, frame: Vec<u8>) -> Option<CutScope> {
        let last = deserialize_response(&frame[4..]).map_or(true, |response| is_last(&response));
        let pending = {
            let mut queue = self.pending.lock().unwrap();
            let front = queue.front().copied();
            if last {
                queue.pop_front();
            }
            front
        };
        match pending.and_then(|pending| pending.cut) {
            Some((CutPoint::GarbledAnswer, _)) => {
                // 0xc1 は MessagePack で使われない値なので、どの応答としても読めない
                self.send(&[0, 0, 0, 3, 0xc1, 0xc1, 0xc1]).await;
                None
            }
            Some((CutPoint::BeforeResponse, scope)) => Some(scope),
            Some((CutPoint::AfterResponse, scope)) if last => {
                self.send(&frame).await;
                if scope == CutScope::Agent {
                    // 応答のすぐ後に閉じたことを届ける
                    let _ = self.handle.eof(self.channel).await;
                    let _ = self.handle.close(self.channel).await;
                }
                Some(scope)
            }
            _ => {
                self.send(&frame).await;
                None
            }
        }
    }

    async fn send(&self, bytes: &[u8]) {
        let _ = self.handle.data(self.channel, bytes.to_vec()).await;
    }
}

/// 分けて返す応答で、最後の一つか
fn is_last(response: &AgentResponse) -> bool {
    match response {
        AgentResponse::TreeChunk { is_last, .. }
        | AgentResponse::FileContents { is_last, .. }
        | AgentResponse::FileHashes { is_last, .. } => *is_last,
        _ => true,
    }
}

/// 要求の種類の名前（`AgentRequest` の名前）
fn request_kind(request: &AgentRequest) -> String {
    let text = format!("{request:?}");
    text.split(|ch: char| !ch.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_string()
}

/// 長さの前置き（4 バイトのビッグエンディアン）を含むフレームを一つ取り出す
fn take_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    if buffer.len() < 4 {
        return None;
    }
    let length = u32::from_be_bytes(buffer[..4].try_into().unwrap()) as usize;
    if buffer.len() < 4 + length {
        return None;
    }
    Some(buffer.drain(..4 + length).collect())
}
