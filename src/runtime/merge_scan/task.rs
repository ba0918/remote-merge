//! マージ走査スレッド内の処理。
//!
//! SSH接続・サブツリー展開・コンテンツ読み込みを実行する。
//! AppState には一切触らず、結果を MergeScanResult で返す。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::agent::protocol::{FileKind, FileReadResult};
use crate::app::{MergeScanMsg, MergeScanResult};
use crate::config::AppConfig;
use crate::diff::binary::BinaryInfo;
use crate::runtime::core::BoxedAgentClient;
use crate::ssh::client::SshClient;
use crate::tree::FileNode;

/// ファイル数上限（DoS 防止）
const MAX_FILES: usize = 10_000;

/// reference サーバの接続情報（スレッドに渡す用）
#[derive(Debug, Clone)]
pub enum RefSource {
    /// ローカルファイルシステム
    Local(PathBuf),
    /// リモートサーバ（サーバ名）
    Remote(String),
}

/// Agent を使ったバッチ読み込みの1チャンクあたりのファイル数
const AGENT_READ_BATCH_SIZE: usize = 256;

/// 走査スレッドのメイン処理
#[allow(clippy::too_many_arguments)]
pub fn run_merge_scan(
    tx: &mpsc::Sender<MergeScanMsg>,
    agent: Option<Arc<Mutex<BoxedAgentClient>>>,
    ref_agent: Option<Arc<Mutex<BoxedAgentClient>>>,
    local_root: &Path,
    exclude: &[String],
    config: &AppConfig,
    server_name: &str,
    dir_path: &str,
    ref_source: Option<RefSource>,
) -> Result<MergeScanResult, String> {
    let scan_start = std::time::Instant::now();
    tracing::info!(
        "Merge scan started: server={}, dir={}, ref={:?}",
        server_name,
        dir_path,
        ref_source
    );

    let server_config = config
        .servers
        .get(server_name)
        .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;
    let remote_root = server_config.root_dir.to_string_lossy().to_string();

    // Agent パスを試行
    if let Some(ref agent_arc) = agent {
        match run_merge_scan_via_agent(
            tx,
            agent_arc,
            ref_agent.as_ref(),
            local_root,
            exclude,
            config,
            server_name,
            dir_path,
            &remote_root,
            &ref_source,
        ) {
            Ok(result) => {
                log_scan_completion(&result, scan_start);
                return Ok(result);
            }
            Err(e) => {
                tracing::warn!("Agent merge scan failed, falling back to SSH: {}", e);
                let _ = tx.send(MergeScanMsg::AgentFailed {
                    server_name: server_name.to_string(),
                });
                // ref_agent が失敗した場合もここで通知（ref_agent の失敗は後で個別に通知するが、
                // メイン Agent の失敗はここでまとめて処理）
            }
        }
    }

    // SSH パス（Agent なし or Agent 失敗のフォールバック）
    run_merge_scan_via_ssh(
        tx,
        local_root,
        exclude,
        config,
        server_name,
        dir_path,
        &remote_root,
        ref_source,
        scan_start,
    )
}

/// 走査完了ログを出力するヘルパー
fn log_scan_completion(result: &MergeScanResult, scan_start: std::time::Instant) {
    let duration = scan_start.elapsed();
    let total_files = result.local_cache.len() + result.local_binary_cache.len();
    let ref_files = result.ref_cache.len() + result.ref_binary_cache.len();
    let errors = result.error_paths.len();
    tracing::info!(
        "Merge scan completed: files={}, ref_files={}, errors={}, duration={:.2}s",
        total_files,
        ref_files,
        errors,
        duration.as_secs_f64()
    );
}

/// SSH 経由でマージ走査を実行する（既存の SSH パス）
#[allow(clippy::too_many_arguments)]
fn run_merge_scan_via_ssh(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    exclude: &[String],
    config: &AppConfig,
    server_name: &str,
    dir_path: &str,
    remote_root: &str,
    ref_source: Option<RefSource>,
    scan_start: std::time::Instant,
) -> Result<MergeScanResult, String> {
    let server_config = config
        .servers
        .get(server_name)
        .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

    let mut client = rt
        .block_on(SshClient::connect(server_name, server_config, &config.ssh))
        .map_err(|e| format!("SSH connection failed: {}", e))?;

    // ref がリモートの場合、追加の SSH 接続を確立（失敗してもメインスキャンは続行）
    let (mut ref_client, ref_remote_root) = match &ref_source {
        Some(RefSource::Remote(ref_name)) => match config.servers.get(ref_name) {
            Some(ref_config) => {
                let root = ref_config.root_dir.to_string_lossy().to_string();
                match rt.block_on(SshClient::connect(ref_name, ref_config, &config.ssh)) {
                    Ok(c) => (Some(c), Some(root)),
                    Err(e) => {
                        tracing::warn!("Ref SSH connection failed (skipping ref): {}", e);
                        (None, None)
                    }
                }
            }
            None => {
                tracing::warn!(
                    "Ref server '{}' not found in config (skipping ref)",
                    ref_name
                );
                (None, None)
            }
        },
        _ => (None, None),
    };

    // サブツリーを再帰的に展開
    let mut local_tree_updates = Vec::new();
    let mut remote_tree_updates = Vec::new();
    let mut file_paths = Vec::new();

    expand_subtree_recursive(
        tx,
        local_root,
        exclude,
        remote_root,
        &rt,
        &mut client,
        dir_path,
        &mut local_tree_updates,
        &mut remote_tree_updates,
        &mut file_paths,
    )?;

    // コンテンツ読み込みフェーズ
    let mut result = MergeScanResult {
        local_cache: HashMap::new(),
        remote_cache: HashMap::new(),
        local_binary_cache: HashMap::new(),
        remote_binary_cache: HashMap::new(),
        ref_cache: HashMap::new(),
        ref_binary_cache: HashMap::new(),
        local_tree_updates,
        remote_tree_updates,
        error_paths: HashSet::new(),
    };
    read_all_contents(
        tx,
        local_root,
        remote_root,
        &rt,
        &mut client,
        &file_paths,
        &mut result,
    );

    // ref コンテンツ読み込み
    if ref_source.is_some() {
        read_ref_contents(
            &rt,
            &ref_source,
            &mut ref_client,
            ref_remote_root.as_deref(),
            &file_paths,
            &mut result,
        );
    }

    let _ = rt.block_on(client.disconnect());
    if let Some(rc) = ref_client.take() {
        let _ = rt.block_on(rc.disconnect());
    }

    log_scan_completion(&result, scan_start);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Agent パス
// ---------------------------------------------------------------------------

/// Agent を使ってサブツリーのファイルリストを一括取得する
fn agent_list_tree(
    agent: &Arc<Mutex<BoxedAgentClient>>,
    scan_root: &str,
    exclude: &[String],
    max_entries: usize,
) -> Result<Vec<crate::agent::protocol::AgentFileEntry>, String> {
    let mut guard = agent
        .lock()
        .map_err(|_| "Agent mutex poisoned".to_string())?;
    guard
        .list_tree(scan_root, exclude, max_entries)
        .map_err(|e| format!("Agent list_tree failed: {}", e))
}

/// Agent を使って全ファイルのリモートコンテンツをバッチ読み込みする
/// Agent 読み込み結果（テキスト/バイナリキャッシュ + 読み込み失敗パスセット）
type AgentReadFullResult = (
    HashMap<String, String>,
    HashMap<String, BinaryInfo>,
    HashSet<String>,
);

/// Agent を使ってファイルコンテンツをバッチ読み込みする共通ヘルパー
///
/// 256ファイルずつバッチ分割し、テキスト/バイナリを分類する。
/// 読み込み失敗パスは `failed_paths` として返す（error_paths 計算に使用）。
/// `tx` が Some の場合は進捗メッセージを送信する。
fn agent_read_files_batch(
    agent: &Arc<Mutex<BoxedAgentClient>>,
    remote_root: &str,
    file_paths: &[String],
    label: &str,
    tx: Option<&mpsc::Sender<MergeScanMsg>>,
) -> Result<AgentReadFullResult, String> {
    let full_paths: Vec<String> = file_paths
        .iter()
        .map(|p| format!("{}/{}", remote_root.trim_end_matches('/'), p))
        .collect();

    let mut text_cache = HashMap::new();
    let mut binary_cache = HashMap::new();
    let mut failed_paths = HashSet::new();

    for (chunk_idx, chunk) in full_paths.chunks(AGENT_READ_BATCH_SIZE).enumerate() {
        let chunk_paths: Vec<String> = chunk.to_vec();
        let mut guard = agent
            .lock()
            .map_err(|_| "Agent mutex poisoned".to_string())?;
        let results = guard
            .read_files(&chunk_paths, 0)
            .map_err(|e| format!("Agent read_files ({}) failed: {}", label, e))?;
        drop(guard); // ロック早期解放

        let base_idx = chunk_idx * AGENT_READ_BATCH_SIZE;
        for (i, read_result) in results.into_iter().enumerate() {
            let rel_path = &file_paths[base_idx + i];
            match read_result {
                FileReadResult::Ok { content, .. } => {
                    if crate::diff::engine::is_binary(&content) {
                        binary_cache.insert(rel_path.clone(), BinaryInfo::from_bytes(&content));
                    } else {
                        match String::from_utf8(content) {
                            Ok(text) => {
                                text_cache.insert(rel_path.clone(), text);
                            }
                            Err(e) => {
                                // UTF-8 ではないがバイナリ判定を通過したケース
                                let bytes = e.into_bytes();
                                binary_cache
                                    .insert(rel_path.clone(), BinaryInfo::from_bytes(&bytes));
                            }
                        }
                    }
                }
                FileReadResult::Error { path, message } => {
                    tracing::debug!("Agent ({}) failed to read {}: {}", label, path, message);
                    failed_paths.insert(rel_path.clone());
                }
            }
        }

        // 進捗更新（tx が提供されている場合のみ）
        if let Some(sender) = tx {
            let _ = sender.send(MergeScanMsg::Progress {
                files_found: base_idx + chunk.len(),
                // 相対パスで表示（UI 表示用）
                current_path: file_paths.get(base_idx + chunk.len() - 1).cloned(),
            });
        }
    }

    Ok((text_cache, binary_cache, failed_paths))
}

/// Agent 経由でマージ走査全体を実行する
#[allow(clippy::too_many_arguments)]
fn run_merge_scan_via_agent(
    tx: &mpsc::Sender<MergeScanMsg>,
    agent: &Arc<Mutex<BoxedAgentClient>>,
    ref_agent: Option<&Arc<Mutex<BoxedAgentClient>>>,
    local_root: &Path,
    exclude: &[String],
    config: &AppConfig,
    server_name: &str,
    dir_path: &str,
    remote_root: &str,
    ref_source: &Option<RefSource>,
) -> Result<MergeScanResult, String> {
    tracing::info!(
        "Merge scan via Agent: server={}, dir={}",
        server_name,
        dir_path
    );

    // 1. Agent list_tree でリモートのサブツリーを一括取得
    let scan_root = format!(
        "{}/{}",
        remote_root.trim_end_matches('/'),
        dir_path.trim_matches('/')
    );
    let entries = agent_list_tree(agent, &scan_root, exclude, MAX_FILES)?;

    // エントリを FileNode に変換し、ツリー更新を構築
    let (remote_tree_updates, file_paths_from_remote) =
        build_tree_updates_from_agent_entries(&entries, dir_path);

    // ローカルのサブツリーも走査
    let (local_tree_updates, file_paths_from_local) =
        build_local_tree_updates(local_root, exclude, dir_path)?;

    // ファイルパスを統合（重複排除）
    let mut all_file_paths_set = HashSet::new();
    let mut file_paths = Vec::new();
    for p in file_paths_from_local
        .iter()
        .chain(file_paths_from_remote.iter())
    {
        if all_file_paths_set.insert(p.clone()) {
            file_paths.push(p.clone());
        }
    }
    file_paths.sort();

    let _ = tx.send(MergeScanMsg::Progress {
        files_found: file_paths.len(),
        current_path: None,
    });

    // 2. コンテンツ読み込みフェーズ
    let _ = tx.send(MergeScanMsg::ContentPhase {
        total: file_paths.len(),
    });

    // ローカルコンテンツ読み込み
    let (local_cache, local_binary_cache, local_failed_paths) =
        read_local_contents(local_root, &file_paths);

    // リモートコンテンツを Agent でバッチ読み込み
    let (remote_cache, remote_binary_cache, remote_failed_paths) =
        agent_read_files_batch(agent, remote_root, &file_paths, server_name, Some(tx))?;

    // error_paths: SSH パスと同じセマンティクス — 両方とも読み込めなかった場合のみエラー
    let error_paths: HashSet<String> = local_failed_paths
        .intersection(&remote_failed_paths)
        .cloned()
        .collect();

    // ref コンテンツ読み込み
    let (ref_cache, ref_binary_cache) = if let Some(RefSource::Remote(ref_name)) = ref_source {
        if let Some(ra) = ref_agent {
            let ref_config = config.servers.get(ref_name.as_str());
            if let Some(rc) = ref_config {
                let ref_root = rc.root_dir.to_string_lossy().to_string();
                match agent_read_files_batch(ra, &ref_root, &file_paths, "ref", None) {
                    Ok((tc, bc, _)) => (tc, bc),
                    Err(e) => {
                        tracing::warn!("Agent ref read failed (skipping ref): {}", e);
                        let _ = tx.send(MergeScanMsg::AgentFailed {
                            server_name: ref_name.clone(),
                        });
                        (HashMap::new(), HashMap::new())
                    }
                }
            } else {
                (HashMap::new(), HashMap::new())
            }
        } else {
            (HashMap::new(), HashMap::new())
        }
    } else if let Some(RefSource::Local(ref_local_root)) = ref_source {
        let (rc, rbc, _) = read_local_contents(ref_local_root, &file_paths);
        (rc, rbc)
    } else {
        (HashMap::new(), HashMap::new())
    };

    Ok(MergeScanResult {
        local_cache,
        remote_cache,
        local_binary_cache,
        remote_binary_cache,
        ref_cache,
        ref_binary_cache,
        local_tree_updates,
        remote_tree_updates,
        error_paths,
    })
}

/// Agent のエントリからツリー更新情報とファイルパスリストを構築する
fn build_tree_updates_from_agent_entries(
    entries: &[crate::agent::protocol::AgentFileEntry],
    dir_path: &str,
) -> TreeUpdatesAndPaths {
    use std::collections::BTreeMap;

    let dir_prefix = if dir_path.is_empty() {
        String::new()
    } else {
        format!("{}/", dir_path.trim_end_matches('/'))
    };

    // ディレクトリごとにエントリをグルーピング
    let mut dir_children: BTreeMap<String, Vec<&crate::agent::protocol::AgentFileEntry>> =
        BTreeMap::new();
    let mut file_paths = Vec::new();

    for entry in entries {
        // エントリパスは scan_root からの相対パス。dir_path を prefix として付与
        let full_rel_path = format!("{}{}", dir_prefix, entry.path);

        // 親ディレクトリを取得
        let parent = match full_rel_path.rfind('/') {
            Some(pos) => &full_rel_path[..pos],
            None => dir_path,
        };

        dir_children
            .entry(parent.to_string())
            .or_default()
            .push(entry);

        // ファイルのみパスリストに追加（シンボリックリンクは除外）
        if entry.kind == FileKind::File {
            file_paths.push(full_rel_path);
        }
    }

    // 各ディレクトリの子ノードリストを FileNode に変換
    let tree_updates: Vec<(String, Vec<FileNode>)> = dir_children
        .into_iter()
        .map(|(dir, dir_entries)| {
            let nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(
                &dir_entries.into_iter().cloned().collect::<Vec<_>>(),
            );
            (dir, nodes)
        })
        .collect();

    (tree_updates, file_paths)
}

/// ツリー更新情報の型（ディレクトリパス -> 子ノード）とファイルパスリスト
type TreeUpdatesAndPaths = (Vec<(String, Vec<FileNode>)>, Vec<String>);

/// ローカルサブツリーを走査してツリー更新とファイルパスを返す
fn build_local_tree_updates(
    local_root: &Path,
    exclude: &[String],
    dir_path: &str,
) -> Result<TreeUpdatesAndPaths, String> {
    let mut tree_updates = Vec::new();
    let mut file_paths = Vec::new();
    build_local_tree_recursive(
        local_root,
        exclude,
        dir_path,
        &mut tree_updates,
        &mut file_paths,
    )?;
    Ok((tree_updates, file_paths))
}

/// ローカルツリーを再帰的に構築する
fn build_local_tree_recursive(
    local_root: &Path,
    exclude: &[String],
    dir_path: &str,
    tree_updates: &mut Vec<(String, Vec<FileNode>)>,
    file_paths: &mut Vec<String>,
) -> Result<(), String> {
    let local_full = local_root.join(dir_path);
    if !local_full.is_dir() {
        return Ok(());
    }

    let children = match crate::local::scan_dir(&local_full, exclude, dir_path) {
        Ok(children) => {
            tree_updates.push((dir_path.to_string(), children.clone()));
            children
        }
        Err(e) => {
            tracing::debug!("Local dir scan skipped: {} - {}", dir_path, e);
            return Ok(());
        }
    };

    let mut sub_dirs = Vec::new();
    for child in &children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.push(child_path);
        } else if !child.is_symlink() {
            file_paths.push(child_path);
        }
    }

    sub_dirs.sort();
    for sub_dir in sub_dirs {
        build_local_tree_recursive(local_root, exclude, &sub_dir, tree_updates, file_paths)?;
    }

    Ok(())
}

/// ローカルファイルのコンテンツを一括読み込みする
fn read_local_contents(
    local_root: &Path,
    file_paths: &[String],
) -> (
    HashMap<String, String>,
    HashMap<String, BinaryInfo>,
    HashSet<String>,
) {
    let mut text_cache = HashMap::new();
    let mut binary_cache = HashMap::new();
    let mut error_paths = HashSet::new();

    for path in file_paths {
        match crate::merge::executor::read_local_file(local_root, path) {
            Ok(content) => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    binary_cache.insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    text_cache.insert(path.clone(), content);
                }
            }
            Err(e) => {
                tracing::debug!("Local file read skipped: {} - {}", path, e);
                error_paths.insert(path.clone());
            }
        }
    }

    (text_cache, binary_cache, error_paths)
}

/// 全ファイルのコンテンツを読み込み、結果を MergeScanResult に蓄積する
fn read_all_contents(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    remote_root: &str,
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    file_paths: &[String],
    result: &mut MergeScanResult,
) {
    use crate::diff::binary::BinaryInfo;
    use crate::merge::executor;

    let total = file_paths.len();
    let _ = tx.send(MergeScanMsg::ContentPhase { total });

    for (i, path) in file_paths.iter().enumerate() {
        if i % 5 == 0 {
            let _ = tx.send(MergeScanMsg::Progress {
                files_found: i,
                current_path: Some(path.clone()),
            });
        }

        // ローカルコンテンツ
        let local_ok = match executor::read_local_file(local_root, path) {
            Ok(content) => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    result
                        .local_binary_cache
                        .insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    result.local_cache.insert(path.clone(), content);
                }
                true
            }
            Err(e) => {
                tracing::debug!("Local file read skipped: {} - {}", path, e);
                false
            }
        };

        // リモートコンテンツ
        let full_remote = format!("{}/{}", remote_root.trim_end_matches('/'), path);
        let remote_ok = match rt.block_on(client.read_file(&full_remote)) {
            Ok(content) => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    result
                        .remote_binary_cache
                        .insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    result.remote_cache.insert(path.clone(), content);
                }
                true
            }
            Err(e) => {
                tracing::debug!("Remote file read skipped: {} - {}", path, e);
                false
            }
        };

        // 両方とも読み込めなかった場合のみエラー扱い
        if !local_ok && !remote_ok {
            result.error_paths.insert(path.clone());
        }
    }
}

/// reference サーバのコンテンツを読み込む
fn read_ref_contents(
    rt: &tokio::runtime::Runtime,
    ref_source: &Option<RefSource>,
    ref_client: &mut Option<SshClient>,
    ref_remote_root: Option<&str>,
    file_paths: &[String],
    result: &mut MergeScanResult,
) {
    use crate::diff::binary::BinaryInfo;

    for path in file_paths {
        match ref_source {
            Some(RefSource::Local(local_root)) => {
                // ローカルファイルシステムから読み込み
                match crate::merge::executor::read_local_file(local_root, path) {
                    Ok(content) => {
                        if crate::diff::engine::is_binary(content.as_bytes()) {
                            result
                                .ref_binary_cache
                                .insert(path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                        } else {
                            result.ref_cache.insert(path.clone(), content);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Ref local file read skipped: {} - {}", path, e);
                    }
                }
            }
            Some(RefSource::Remote(_)) => {
                // リモートサーバから SSH で読み込み
                if let (Some(client), Some(root)) = (ref_client.as_mut(), ref_remote_root) {
                    let full_path = format!("{}/{}", root.trim_end_matches('/'), path);
                    match rt.block_on(client.read_file(&full_path)) {
                        Ok(content) => {
                            if crate::diff::engine::is_binary(content.as_bytes()) {
                                result.ref_binary_cache.insert(
                                    path.clone(),
                                    BinaryInfo::from_bytes(content.as_bytes()),
                                );
                            } else {
                                result.ref_cache.insert(path.clone(), content);
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Ref remote file read skipped: {} - {}", path, e);
                        }
                    }
                }
            }
            None => {}
        }
    }
}

/// サブツリーを再帰的に展開し、ファイルパスを収集する
#[allow(clippy::too_many_arguments)]
fn expand_subtree_recursive(
    tx: &mpsc::Sender<MergeScanMsg>,
    local_root: &Path,
    exclude: &[String],
    remote_root: &str,
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    dir_path: &str,
    local_tree_updates: &mut Vec<(String, Vec<FileNode>)>,
    remote_tree_updates: &mut Vec<(String, Vec<FileNode>)>,
    file_paths: &mut Vec<String>,
) -> Result<(), String> {
    if file_paths.len() >= MAX_FILES {
        return Err(format!("File limit reached ({})", MAX_FILES));
    }

    let _ = tx.send(MergeScanMsg::Progress {
        files_found: file_paths.len(),
        current_path: Some(dir_path.to_string()),
    });

    // ローカルディレクトリの走査
    let local_full = local_root.join(dir_path);
    let local_children = if local_full.is_dir() {
        match crate::local::scan_dir(&local_full, exclude, dir_path) {
            Ok(children) => {
                local_tree_updates.push((dir_path.to_string(), children.clone()));
                children
            }
            Err(e) => {
                tracing::debug!("Local dir scan skipped: {} - {}", dir_path, e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // リモートディレクトリの走査
    let remote_full = format!("{}/{}", remote_root.trim_end_matches('/'), dir_path);
    let remote_children = match rt.block_on(client.list_dir(&remote_full, exclude, dir_path)) {
        Ok(children) => {
            remote_tree_updates.push((dir_path.to_string(), children.clone()));
            children
        }
        Err(e) => {
            tracing::debug!("Remote dir scan skipped: {} - {}", dir_path, e);
            Vec::new()
        }
    };

    // ファイルパスを収集し、サブディレクトリを再帰的に展開
    let mut sub_dirs = HashSet::new();
    let mut local_file_set = HashSet::new();

    for child in &local_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else if !child.is_symlink() {
            // シンボリックリンクはツリーノードから直接比較するためスキップ
            local_file_set.insert(child_path.clone());
            file_paths.push(child_path);
        }
    }

    for child in &remote_children {
        let child_path = format!("{}/{}", dir_path, child.name);
        if child.is_dir() {
            sub_dirs.insert(child_path);
        } else if !child.is_symlink() && !local_file_set.contains(&child_path) {
            file_paths.push(child_path);
        }
    }

    // サブディレクトリを再帰
    let mut sorted_dirs: Vec<String> = sub_dirs.into_iter().collect();
    sorted_dirs.sort();
    for sub_dir in sorted_dirs {
        expand_subtree_recursive(
            tx,
            local_root,
            exclude,
            remote_root,
            rt,
            client,
            &sub_dir,
            local_tree_updates,
            remote_tree_updates,
            file_paths,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::protocol::{AgentFileEntry, FileKind};
    use tempfile::TempDir;

    // ── build_tree_updates_from_agent_entries テスト ──

    #[test]
    fn build_tree_updates_empty_entries() {
        let (updates, paths) = build_tree_updates_from_agent_entries(&[], "src");
        assert!(updates.is_empty());
        assert!(paths.is_empty());
    }

    #[test]
    fn build_tree_updates_files_only() {
        let entries = vec![
            AgentFileEntry {
                path: "a.txt".to_string(),
                kind: FileKind::File,
                size: 100,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
            AgentFileEntry {
                path: "b.txt".to_string(),
                kind: FileKind::File,
                size: 200,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
        ];
        let (updates, paths) = build_tree_updates_from_agent_entries(&entries, "app");
        // ファイルパスが dir_path prefix 付きで生成される
        assert_eq!(paths, vec!["app/a.txt", "app/b.txt"]);
        // ツリー更新にエントリが含まれる
        assert!(!updates.is_empty());
    }

    #[test]
    fn build_tree_updates_directories_excluded_from_file_paths() {
        let entries = vec![
            AgentFileEntry {
                path: "sub".to_string(),
                kind: FileKind::Directory,
                size: 0,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o755,
                symlink_target: None,
            },
            AgentFileEntry {
                path: "sub/file.txt".to_string(),
                kind: FileKind::File,
                size: 50,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
        ];
        let (_, paths) = build_tree_updates_from_agent_entries(&entries, "root");
        // ディレクトリは file_paths に含まれない
        assert_eq!(paths, vec!["root/sub/file.txt"]);
    }

    #[test]
    fn build_tree_updates_symlinks_excluded_from_file_paths() {
        let entries = vec![
            AgentFileEntry {
                path: "link".to_string(),
                kind: FileKind::Symlink,
                size: 0,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o777,
                symlink_target: Some("target".to_string()),
            },
            AgentFileEntry {
                path: "file.rs".to_string(),
                kind: FileKind::File,
                size: 100,
                mtime_secs: 0,
                mtime_nanos: 0,
                permissions: 0o644,
                symlink_target: None,
            },
        ];
        let (_, paths) = build_tree_updates_from_agent_entries(&entries, "src");
        assert_eq!(paths, vec!["src/file.rs"]);
    }

    // ── read_local_contents テスト ──

    #[test]
    fn read_local_contents_text_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let (text, binary, errors) = read_local_contents(dir.path(), &["hello.txt".to_string()]);
        assert_eq!(text.get("hello.txt").unwrap(), "hello world");
        assert!(binary.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn read_local_contents_binary_files() {
        let dir = TempDir::new().unwrap();
        // NUL バイトを含むデータはバイナリ判定される
        let mut data = vec![0u8; 100];
        data[0] = 0x00;
        data[50] = 0x00;
        std::fs::write(dir.path().join("bin.dat"), &data).unwrap();

        let (text, binary, errors) = read_local_contents(dir.path(), &["bin.dat".to_string()]);
        assert!(text.is_empty());
        assert!(binary.contains_key("bin.dat"));
        assert!(errors.is_empty());
    }

    #[test]
    fn read_local_contents_missing_files() {
        let dir = TempDir::new().unwrap();

        let (text, binary, errors) =
            read_local_contents(dir.path(), &["nonexistent.txt".to_string()]);
        assert!(text.is_empty());
        assert!(binary.is_empty());
        assert!(errors.contains("nonexistent.txt"));
    }

    // ── build_local_tree_updates テスト ──

    #[test]
    fn build_local_tree_updates_with_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("src/lib.rs"), "// lib").unwrap();

        let (updates, paths) = build_local_tree_updates(&root, &[], "src").unwrap();
        assert!(!updates.is_empty());
        // ファイルパスが正しく収集される
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn build_local_tree_updates_nonexistent_dir() {
        let dir = TempDir::new().unwrap();
        let (updates, paths) = build_local_tree_updates(dir.path(), &[], "nonexistent").unwrap();
        assert!(updates.is_empty());
        assert!(paths.is_empty());
    }

    // ── AGENT_READ_BATCH_SIZE テスト ──

    #[test]
    fn agent_read_batch_size_is_reasonable() {
        // 定数アサーションは const ブロックで検証
        const {
            assert!(AGENT_READ_BATCH_SIZE > 0);
            assert!(AGENT_READ_BATCH_SIZE <= 1024);
        }
        assert_eq!(AGENT_READ_BATCH_SIZE, 256);
    }

    // ── log_scan_completion テスト ──

    #[test]
    fn log_scan_completion_does_not_panic() {
        let result = MergeScanResult {
            local_cache: HashMap::new(),
            remote_cache: HashMap::new(),
            local_binary_cache: HashMap::new(),
            remote_binary_cache: HashMap::new(),
            ref_cache: HashMap::new(),
            ref_binary_cache: HashMap::new(),
            local_tree_updates: Vec::new(),
            remote_tree_updates: Vec::new(),
            error_paths: HashSet::new(),
        };
        let start = std::time::Instant::now();
        log_scan_completion(&result, start);
    }

    // ── chunking ロジックテスト ──

    #[test]
    fn agent_read_batch_chunking_logic() {
        // AGENT_READ_BATCH_SIZE でチャンク分割されることを検証
        let paths: Vec<String> = (0..600).map(|i| format!("file_{}.txt", i)).collect();
        let chunks: Vec<&[String]> = paths.chunks(AGENT_READ_BATCH_SIZE).collect();
        assert_eq!(chunks.len(), 3); // 600 / 256 = 2.34 → 3チャンク
        assert_eq!(chunks[0].len(), 256);
        assert_eq!(chunks[1].len(), 256);
        assert_eq!(chunks[2].len(), 88);
    }
}
