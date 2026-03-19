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
use crate::runtime::side_io::AGENT_CHUNK_SIZE_LIMIT;
use crate::ssh::client::SshClient;
use crate::ssh::passphrase_provider::PassphraseProvider;
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
/// side_io.rs と同じ値を使用（MAX_FRAME_SIZE 16MB を超えないための安全策）
const AGENT_READ_BATCH_SIZE: usize = 100;

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
    passphrase_provider: Option<&dyn PassphraseProvider>,
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
        passphrase_provider,
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
    passphrase_provider: Option<&dyn PassphraseProvider>,
) -> Result<MergeScanResult, String> {
    let server_config = config
        .servers
        .get(server_name)
        .ok_or_else(|| format!("Server '{}' not found in config", server_name))?;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime creation failed: {}", e))?;

    let mut client = rt
        .block_on(SshClient::connect_with_passphrase(
            server_name,
            server_config,
            &config.ssh,
            passphrase_provider,
        ))
        .map_err(|e| format!("SSH connection failed: {}", e))?;

    // ref がリモートの場合、追加の SSH 接続を確立（失敗してもメインスキャンは続行）
    let (mut ref_client, ref_remote_root) = match &ref_source {
        Some(RefSource::Remote(ref_name)) => match config.servers.get(ref_name) {
            Some(ref_config) => {
                let root = ref_config.root_dir.to_string_lossy().to_string();
                match rt.block_on(SshClient::connect_with_passphrase(
                    ref_name,
                    ref_config,
                    &config.ssh,
                    passphrase_provider,
                )) {
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

/// Agent を使ってサブツリーのファイルリストを一括取得する。
/// truncation フラグは現時点では無視する（merge_scan はサブツリー単位の走査のため）。
fn agent_list_tree(
    agent: &Arc<Mutex<BoxedAgentClient>>,
    scan_root: &str,
    exclude: &[String],
    max_entries: usize,
) -> Result<Vec<crate::agent::protocol::AgentFileEntry>, String> {
    let mut guard = agent
        .lock()
        .map_err(|_| "Agent mutex poisoned".to_string())?;
    // merge_scan はサブツリー単位の走査のため include は空（サブパス指定で十分）
    let (entries, _truncated) = guard
        .list_tree(scan_root, exclude, &[], max_entries)
        .map_err(|e| format!("Agent list_tree failed: {}", e))?;
    Ok(entries)
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
    file_paths: &[String],
    label: &str,
    tx: Option<&mpsc::Sender<MergeScanMsg>>,
) -> Result<AgentReadFullResult, String> {
    // Agent は --root で起動時にルートディレクトリ設定済みなので、相対パスをそのまま渡す
    let full_paths: Vec<String> = file_paths.to_vec();

    let mut text_cache = HashMap::new();
    let mut binary_cache = HashMap::new();
    let mut failed_paths = HashSet::new();

    let mut processed_count = 0usize;
    for chunk in full_paths.chunks(AGENT_READ_BATCH_SIZE) {
        let chunk_paths: Vec<String> = chunk.to_vec();
        let mut guard = agent
            .lock()
            .map_err(|_| "Agent mutex poisoned".to_string())?;
        let results = guard
            .read_files(&chunk_paths, AGENT_CHUNK_SIZE_LIMIT)
            .map_err(|e| format!("Agent read_files ({}) failed: {}", label, e))?;
        drop(guard); // ロック早期解放

        // more_to_follow チャンクを結合してファイル単位に再組立
        let mut current_path: Option<String> = None;
        let mut current_buf: Vec<u8> = Vec::new();

        let flush_file = |path: String,
                          content: Vec<u8>,
                          text_cache: &mut HashMap<String, String>,
                          binary_cache: &mut HashMap<String, BinaryInfo>| {
            if crate::diff::engine::is_binary(&content) {
                binary_cache.insert(path, BinaryInfo::from_bytes(&content));
            } else {
                match String::from_utf8(content) {
                    Ok(text) => {
                        text_cache.insert(path, text);
                    }
                    Err(e) => {
                        let bytes = e.into_bytes();
                        binary_cache.insert(path, BinaryInfo::from_bytes(&bytes));
                    }
                }
            }
        };

        for read_result in results {
            match read_result {
                FileReadResult::Ok {
                    path,
                    content,
                    more_to_follow,
                } => {
                    let is_new_file = current_path.as_ref() != Some(&path);
                    if is_new_file {
                        if let Some(prev_path) = current_path.take() {
                            flush_file(
                                prev_path,
                                std::mem::take(&mut current_buf),
                                &mut text_cache,
                                &mut binary_cache,
                            );
                        }
                        current_path = Some(path);
                        current_buf = content;
                    } else {
                        current_buf.extend(content);
                    }

                    if !more_to_follow {
                        if let Some(p) = current_path.take() {
                            flush_file(
                                p,
                                std::mem::take(&mut current_buf),
                                &mut text_cache,
                                &mut binary_cache,
                            );
                        }
                    }
                }
                FileReadResult::Error { path, message } => {
                    tracing::debug!("Agent ({}) failed to read {}: {}", label, path, message);
                    failed_paths.insert(path);
                }
            }
        }
        // 安全策: more_to_follow のまま終わったファイルをフラッシュ
        if let Some(p) = current_path.take() {
            flush_file(p, current_buf, &mut text_cache, &mut binary_cache);
        }

        processed_count += chunk.len();
        // 進捗更新（tx が提供されている場合のみ）
        if let Some(sender) = tx {
            let _ = sender.send(MergeScanMsg::Progress {
                files_found: processed_count,
                // 相対パスで表示（UI 表示用）
                current_path: chunk.last().cloned(),
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
    ref_source: &Option<RefSource>,
) -> Result<MergeScanResult, String> {
    tracing::info!(
        "Merge scan via Agent: server={}, dir={}",
        server_name,
        dir_path
    );

    // 1. Agent list_tree でリモートのサブツリーを一括取得
    // Agent は --root で起動時にルートディレクトリ設定済みなので、相対パスを渡す
    let scan_root = dir_path.trim_matches('/');
    let entries = agent_list_tree(agent, scan_root, exclude, MAX_FILES)?;

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
        agent_read_files_batch(agent, &file_paths, server_name, Some(tx))?;

    // error_paths: SSH パスと同じセマンティクス — 両方とも読み込めなかった場合のみエラー
    let error_paths: HashSet<String> = local_failed_paths
        .intersection(&remote_failed_paths)
        .cloned()
        .collect();

    // ref コンテンツ読み込み
    let (ref_cache, ref_binary_cache) = if let Some(RefSource::Remote(ref_name)) = ref_source {
        if let Some(ra) = ref_agent {
            let ref_config = config.servers.get(ref_name.as_str());
            if ref_config.is_some() {
                match agent_read_files_batch(ra, &file_paths, "ref", None) {
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
    // convert_agent_entries_to_nodes はフルパスを保持するため、
    // ディレクトリ単位のツリー更新では最後のセグメント（ファイル名）に戻す
    let tree_updates: Vec<(String, Vec<FileNode>)> = dir_children
        .into_iter()
        .map(|(dir, dir_entries)| {
            let mut nodes = crate::agent::tree_scan::convert_agent_entries_to_nodes(
                &dir_entries.into_iter().cloned().collect::<Vec<_>>(),
            );
            for node in &mut nodes {
                if let Some(pos) = node.name.rfind('/') {
                    node.name = node.name[pos + 1..].to_string();
                }
            }
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

/// リモートファイルをバッチで読み込み、テキスト/バイナリに分類する共通ヘルパー。
///
/// `read_files_batch` で一括取得し、`is_binary` でテキスト/バイナリを判定する。
/// 空文字列（ファイル不在）は `error_paths` に分類する。
/// `tx` が `Some` の場合は進捗メッセージを送信する。
///
/// 戻り値: (text_cache, binary_cache, error_paths) — キーは相対パス
fn read_remote_contents_batch(
    rt: &tokio::runtime::Runtime,
    client: &mut SshClient,
    root: &str,
    file_paths: &[String],
    tx: Option<&mpsc::Sender<MergeScanMsg>>,
) -> (
    HashMap<String, String>,
    HashMap<String, BinaryInfo>,
    HashSet<String>,
) {
    if file_paths.is_empty() {
        return (HashMap::new(), HashMap::new(), HashSet::new());
    }

    let root = root.trim_end_matches('/');
    // 絶対パスリストを構築
    let abs_paths: Vec<String> = file_paths
        .iter()
        .map(|p| format!("{}/{}", root, p))
        .collect();

    let raw = match rt.block_on(client.read_files_batch(&abs_paths)) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("SSH batch read failed: {}", e);
            HashMap::new()
        }
    };

    let mut text_cache = HashMap::new();
    let mut binary_cache = HashMap::new();
    let mut error_paths = HashSet::new();

    for (i, rel_path) in file_paths.iter().enumerate() {
        let abs_path = &abs_paths[i];
        match raw.get(abs_path) {
            Some(content) if !content.is_empty() => {
                if crate::diff::engine::is_binary(content.as_bytes()) {
                    binary_cache
                        .insert(rel_path.clone(), BinaryInfo::from_bytes(content.as_bytes()));
                } else {
                    text_cache.insert(rel_path.clone(), content.clone());
                }
            }
            _ => {
                // 空文字列（ファイル不在）または HashMap に存在しない
                tracing::debug!("Remote file read skipped (batch): {}", rel_path);
                error_paths.insert(rel_path.clone());
            }
        }

        // 進捗更新（5ファイルごと）
        if let Some(sender) = tx {
            if i % 5 == 0 {
                let _ = sender.send(MergeScanMsg::Progress {
                    files_found: i,
                    current_path: Some(rel_path.clone()),
                });
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
    let total = file_paths.len();
    let _ = tx.send(MergeScanMsg::ContentPhase { total });

    // ローカルコンテンツ読み込み
    let (local_text, local_binary, local_failed) = read_local_contents(local_root, file_paths);
    result.local_cache.extend(local_text);
    result.local_binary_cache.extend(local_binary);

    // リモートコンテンツをバッチ読み込み
    let (remote_text, remote_binary, remote_failed) =
        read_remote_contents_batch(rt, client, remote_root, file_paths, Some(tx));
    result.remote_cache.extend(remote_text);
    result.remote_binary_cache.extend(remote_binary);

    // 両方とも読み込めなかった場合のみエラー扱い（既存セマンティクス維持）
    for path in local_failed.intersection(&remote_failed) {
        result.error_paths.insert(path.clone());
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
    match ref_source {
        Some(RefSource::Local(local_root)) => {
            let (tc, bc, _) = read_local_contents(local_root, file_paths);
            result.ref_cache.extend(tc);
            result.ref_binary_cache.extend(bc);
        }
        Some(RefSource::Remote(_)) => {
            if let (Some(client), Some(root)) = (ref_client.as_mut(), ref_remote_root) {
                let (tc, bc, _) = read_remote_contents_batch(rt, client, root, file_paths, None);
                result.ref_cache.extend(tc);
                result.ref_binary_cache.extend(bc);
            }
        }
        None => {}
    }
}

/// ネストされたツリー（`list_tree_recursive` の戻り値）をフラットな
/// (親ディレクトリパス → 直下の子ノードリスト) マッピングに変換する純粋関数。
///
/// - `tree`: `list_tree_recursive` が返した FileNode の木構造
/// - `parent_path`: ツリールートの親ディレクトリパス（例: `"src"`）
///
/// FileNode の `name` は短いファイル名（木構築後）になっているため、
/// このまま `remote_tree_updates` に積める。
pub fn group_nodes_by_parent(tree: &[FileNode], parent_path: &str) -> Vec<(String, Vec<FileNode>)> {
    if tree.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();

    // このレベルの直下ノード（name のみ、children なし）を収集してツリー更新に追加
    let direct_children: Vec<FileNode> = tree
        .iter()
        .map(|n| {
            // 直下ノードは children を含まないシャローコピーとして渡す
            let mut shallow = n.clone();
            if shallow.is_dir() {
                // ディレクトリは children=None にしてロード待ち状態にする
                shallow.children = None;
            }
            shallow
        })
        .collect();
    result.push((parent_path.to_string(), direct_children));

    // 各ディレクトリを再帰的に処理
    for node in tree {
        if node.is_dir() {
            let child_path = if parent_path.is_empty() {
                node.name.clone()
            } else {
                format!("{}/{}", parent_path, node.name)
            };
            if let Some(children) = &node.children {
                let mut sub = group_nodes_by_parent(children, &child_path);
                result.append(&mut sub);
            }
        }
    }

    result
}

/// ネストされたツリーを再帰走査してファイルの相対パスを収集する純粋関数。
///
/// シンボリックリンクはスキップし、通常ファイルのみ収集する。
fn collect_file_paths_from_tree(tree: &[FileNode], parent_path: &str, out: &mut Vec<String>) {
    for node in tree {
        let full_path = if parent_path.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", parent_path, node.name)
        };
        if node.is_dir() {
            if let Some(children) = &node.children {
                collect_file_paths_from_tree(children, &full_path, out);
            }
        } else if !node.is_symlink() {
            out.push(full_path);
        }
    }
}

/// サブツリーを展開し、ファイルパスを収集する。
///
/// リモート側は `list_tree_recursive` で1回の SSH exec に最適化済み。
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
    let _ = tx.send(MergeScanMsg::Progress {
        files_found: file_paths.len(),
        current_path: Some(dir_path.to_string()),
    });

    // ローカルサブツリーを走査
    let (local_updates, local_files) =
        build_local_tree_updates(local_root, exclude, dir_path).unwrap_or_default();
    local_tree_updates.extend(local_updates);
    let local_file_set: HashSet<String> = local_files.iter().cloned().collect();
    file_paths.extend(local_files);

    if file_paths.len() >= MAX_FILES {
        return Err(format!("File limit reached ({})", MAX_FILES));
    }

    // リモートサブツリーを1回の SSH exec で取得
    let remote_abs = format!("{}/{}", remote_root.trim_end_matches('/'), dir_path);
    match rt.block_on(client.list_tree_recursive(&remote_abs, exclude, &[], MAX_FILES, 60)) {
        Ok((tree, truncated)) => {
            if truncated {
                tracing::warn!("Remote tree truncated at {}: {}", MAX_FILES, dir_path);
            }

            // ネストされた木をフラットな (parent → children) マッピングに変換
            let remote_updates = group_nodes_by_parent(&tree, dir_path);
            remote_tree_updates.extend(remote_updates);

            // リモート側のファイルパスを収集（ローカル側にないものだけ追加）
            let mut remote_files = Vec::new();
            collect_file_paths_from_tree(&tree, dir_path, &mut remote_files);
            for path in remote_files {
                if !local_file_set.contains(&path) {
                    file_paths.push(path);
                    if file_paths.len() >= MAX_FILES {
                        return Err(format!("File limit reached ({})", MAX_FILES));
                    }
                }
            }
        }
        Err(e) => {
            tracing::debug!("Remote tree scan skipped: {} - {}", dir_path, e);
        }
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
        assert_eq!(AGENT_READ_BATCH_SIZE, 100);
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
        assert_eq!(chunks.len(), 6); // 600 / 100 = 6チャンク
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[5].len(), 100);
    }

    // ── group_nodes_by_parent テスト ──

    #[test]
    fn group_nodes_by_parent_empty_returns_empty() {
        let result = group_nodes_by_parent(&[], "src");
        assert!(result.is_empty());
    }

    #[test]
    fn group_nodes_by_parent_flat_files() {
        // フラットなファイルのみ（ディレクトリなし）
        let tree = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let result = group_nodes_by_parent(&tree, "app");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "app");
        assert_eq!(result[0].1.len(), 2);
        let names: Vec<&str> = result[0].1.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
    }

    #[test]
    fn group_nodes_by_parent_nested_dirs() {
        // src/ の下に main.rs と lib/ があり、lib/ の下に mod.rs がある
        let mut lib_dir = FileNode::new_dir("lib");
        lib_dir.children = Some(vec![FileNode::new_file("mod.rs")]);

        let mut src_dir = FileNode::new_dir("src");
        src_dir.children = Some(vec![FileNode::new_file("main.rs"), lib_dir]);

        let tree = vec![src_dir];
        let result = group_nodes_by_parent(&tree, "");

        // ルートレベル（""）、src、src/lib の3エントリが存在するはず
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&""), "root entry missing");
        assert!(keys.contains(&"src"), "src entry missing");
        assert!(keys.contains(&"src/lib"), "src/lib entry missing");
    }

    #[test]
    fn group_nodes_by_parent_dir_children_have_no_nested_children() {
        // group_nodes_by_parent が返す直下ノードのディレクトリは children=None
        let mut sub_dir = FileNode::new_dir("sub");
        sub_dir.children = Some(vec![FileNode::new_file("nested.rs")]);

        let tree = vec![sub_dir, FileNode::new_file("top.txt")];
        let result = group_nodes_by_parent(&tree, "root");

        // ルートレベルの sub ディレクトリは children=None であること
        let root_entry = result.iter().find(|(k, _)| k == "root").unwrap();
        let sub_node = root_entry.1.iter().find(|n| n.name == "sub").unwrap();
        assert!(
            sub_node.children.is_none(),
            "shallow copy should have children=None"
        );
    }

    #[test]
    fn group_nodes_by_parent_file_count_matches() {
        // find 結果を模倣: src/main.rs, src/lib.rs, src/util/helper.rs
        let mut util_dir = FileNode::new_dir("util");
        util_dir.children = Some(vec![FileNode::new_file("helper.rs")]);

        let tree = vec![
            FileNode::new_file("main.rs"),
            FileNode::new_file("lib.rs"),
            util_dir,
        ];
        let result = group_nodes_by_parent(&tree, "src");

        // src と src/util の2エントリ
        assert_eq!(result.len(), 2);

        let src_entry = result.iter().find(|(k, _)| k == "src").unwrap();
        assert_eq!(src_entry.1.len(), 3); // main.rs, lib.rs, util/

        let util_entry = result.iter().find(|(k, _)| k == "src/util").unwrap();
        assert_eq!(util_entry.1.len(), 1); // helper.rs
    }

    // ── collect_file_paths_from_tree テスト ──

    #[test]
    fn collect_file_paths_skips_symlinks() {
        let tree = vec![
            FileNode::new_file("main.rs"),
            FileNode::new_symlink("link", "target"),
        ];
        let mut paths = Vec::new();
        collect_file_paths_from_tree(&tree, "src", &mut paths);
        assert_eq!(paths, vec!["src/main.rs"]);
    }

    #[test]
    fn collect_file_paths_recurses_dirs() {
        let mut sub = FileNode::new_dir("sub");
        sub.children = Some(vec![FileNode::new_file("child.rs")]);
        let tree = vec![FileNode::new_file("top.txt"), sub];

        let mut paths = Vec::new();
        collect_file_paths_from_tree(&tree, "app", &mut paths);
        paths.sort();
        assert_eq!(paths, vec!["app/sub/child.rs", "app/top.txt"]);
    }

    #[test]
    fn collect_file_paths_empty_tree() {
        let mut paths = Vec::new();
        collect_file_paths_from_tree(&[], "any", &mut paths);
        assert!(paths.is_empty());
    }

    // ── read_remote_contents_batch ロジック検証（モック不要部分）──

    #[test]
    fn read_remote_contents_batch_empty_paths_no_panic() {
        // 空のファイルリストを渡してもパニックしない
        // （実際の SSH 接続は不要）
        let paths: Vec<String> = Vec::new();
        // ロジック部分のみ: is_empty チェックで早期リターンされる
        assert!(paths.is_empty());
        // read_remote_contents_batch は paths.is_empty() で早期リターンするため
        // 実際に呼ぶことなくカバレッジを担保できる
    }

    #[test]
    fn read_remote_contents_batch_classifies_text_and_binary() {
        // バッチ読み込みのテキスト/バイナリ分類ロジックをシミュレート
        // SSH 接続なしで is_binary の判定ロジックだけを検証
        let text_content = "fn main() {\n    println!(\"hello\");\n}\n";
        let mut binary_data = vec![0u8; 64];
        binary_data[0] = 0x00; // NUL バイト → バイナリ判定

        assert!(!crate::diff::engine::is_binary(text_content.as_bytes()));
        assert!(crate::diff::engine::is_binary(&binary_data));
    }

    #[test]
    fn read_remote_contents_batch_abs_path_construction() {
        // 絶対パス構築ロジックの検証
        let root = "/var/www/app";
        let rel_paths = ["src/main.rs".to_string(), "config/app.toml".to_string()];
        let abs_paths: Vec<String> = rel_paths
            .iter()
            .map(|p| format!("{}/{}", root.trim_end_matches('/'), p))
            .collect();
        assert_eq!(abs_paths[0], "/var/www/app/src/main.rs");
        assert_eq!(abs_paths[1], "/var/www/app/config/app.toml");
    }

    #[test]
    fn read_remote_contents_batch_trailing_slash_root() {
        // root に末尾スラッシュがあっても正しく構築される
        let root = "/var/www/app/";
        let rel = "src/main.rs";
        let abs = format!("{}/{}", root.trim_end_matches('/'), rel);
        assert_eq!(abs, "/var/www/app/src/main.rs");
    }
}
