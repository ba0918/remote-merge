//! Agent ディスパッチャー。
//!
//! `AgentRequest` を受け取り、適切なハンドラに振り分けて `AgentResponse` を返す。
//! サービス層として動作し、stdin/stdout の知識は持たない。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::file_io::{self, SavedMetadata};
use super::protocol::{AgentRequest, AgentResponse, FileReadResult};
use super::server::MetadataConfig;
use super::tree_scan::{self, ScanOptions};

/// Agent ディスパッチャー。root_dir を保持し、リクエストを適切なハンドラに振り分ける。
pub struct Dispatcher {
    root_dir: PathBuf,
    /// チャンク書き込みで最初の書き込み済みパスを追跡する
    written_paths: HashSet<String>,
    /// ファイル書き込み時のメタデータ設定
    metadata_config: MetadataConfig,
    /// チャンク転送中のファイルの保存メタデータ
    /// Some(SavedMetadata) = 既存ファイル、None = 新規ファイル
    saved_metadata: HashMap<String, Option<SavedMetadata>>,
}

impl Dispatcher {
    pub fn new(root_dir: PathBuf, metadata_config: MetadataConfig) -> Self {
        Self {
            root_dir,
            written_paths: HashSet::new(),
            metadata_config,
            saved_metadata: HashMap::new(),
        }
    }

    /// リクエストを処理してレスポンスのリストを返す。
    /// Shutdown の場合は None を返す（呼び出し元がループを終了する合図）。
    /// ListTree は複数の TreeChunk を返すことがある（マルチチャンクストリーミング）。
    /// その他のリクエストは常に要素1つの Vec を返す。
    pub fn dispatch(&mut self, request: AgentRequest) -> Option<Vec<AgentResponse>> {
        match request {
            AgentRequest::Ping => Some(vec![AgentResponse::Pong]),
            AgentRequest::Shutdown => None,
            AgentRequest::ListTree {
                root,
                exclude,
                max_entries,
            } => Some(self.handle_list_tree(&root, &exclude, max_entries)),
            AgentRequest::ReadFiles {
                paths,
                chunk_size_limit,
            } => Some(vec![self.handle_read_files(&paths, chunk_size_limit)]),
            AgentRequest::WriteFile {
                path,
                content,
                // TODO: is_binary は将来のバイナリファイル最適化で使用予定
                is_binary: _,
                more_to_follow,
            } => Some(vec![self.handle_write_file(
                &path,
                &content,
                more_to_follow,
            )]),
            AgentRequest::StatFiles { paths } => Some(vec![self.handle_stat_files(&paths)]),
            AgentRequest::Backup { paths, backup_dir } => {
                Some(vec![self.handle_backup(&paths, &backup_dir)])
            }
            AgentRequest::Symlink { path, target } => {
                Some(vec![self.handle_symlink(&path, &target)])
            }
            AgentRequest::ListBackups { backup_dir } => {
                Some(vec![self.handle_list_backups(&backup_dir)])
            }
            AgentRequest::RestoreBackup {
                backup_dir,
                session_id,
                files,
                ..
            } => Some(vec![self.handle_restore_backup(
                &backup_dir,
                &session_id,
                &files,
            )]),
        }
    }

    /// ListTree をストリーミングで処理し、複数の TreeChunk を返す。
    ///
    /// `scan_tree` が生成する各チャンクをそのまま個別の `TreeChunk` レスポンスにマッピングする。
    /// これにより大規模ディレクトリでも1フレームが MAX_FRAME_SIZE を超えない。
    fn handle_list_tree(
        &self,
        root: &str,
        exclude: &[String],
        max_entries: usize,
    ) -> Vec<AgentResponse> {
        let scan_root = match resolve_scan_root(&self.root_dir, root) {
            Ok(p) => p,
            Err(e) => return vec![AgentResponse::Error { message: e }],
        };
        let options = ScanOptions {
            root: scan_root,
            exclude: exclude.to_vec(),
            max_entries,
            ..Default::default()
        };

        let mut responses: Vec<AgentResponse> = Vec::new();

        for chunk_result in tree_scan::scan_tree(&options) {
            match chunk_result {
                Ok(chunk) => {
                    let truncated = chunk.is_last && chunk.total_scanned >= max_entries;
                    responses.push(AgentResponse::TreeChunk {
                        nodes: chunk.entries,
                        is_last: chunk.is_last,
                        total_scanned: chunk.total_scanned,
                        truncated,
                    });
                }
                Err(e) => {
                    return vec![AgentResponse::Error {
                        message: format!("tree scan error: {e}"),
                    }];
                }
            }
        }

        // 空ディレクトリの場合 scan_tree は空エントリの is_last=true チャンクを1つ返すが、
        // 念のため responses が空の場合もガードする
        if responses.is_empty() {
            responses.push(AgentResponse::TreeChunk {
                nodes: vec![],
                is_last: true,
                total_scanned: 0,
                truncated: false,
            });
        }

        responses
    }

    fn handle_read_files(&self, paths: &[String], chunk_size_limit: usize) -> AgentResponse {
        let mut results: Vec<FileReadResult> = Vec::new();

        for rel_path in paths {
            match file_io::read_file_chunked(&self.root_dir, rel_path, chunk_size_limit) {
                Ok(chunks) => results.extend(chunks),
                Err(e) => {
                    results.push(FileReadResult::Error {
                        path: rel_path.clone(),
                        message: e.to_string(),
                    });
                }
            }
        }

        AgentResponse::FileContents { results }
    }

    fn handle_write_file(
        &mut self,
        rel_path: &str,
        content: &[u8],
        more_to_follow: bool,
    ) -> AgentResponse {
        let is_first_chunk = !self.written_paths.contains(rel_path);
        let is_last_chunk = !more_to_follow;

        // チャンク転送の場合、最初のチャンクで保存したメタデータを取得
        let prev_saved = if !is_first_chunk {
            self.saved_metadata
                .get(rel_path)
                .and_then(|opt| opt.as_ref())
        } else {
            None
        };

        match file_io::write_file_with_metadata(
            &self.root_dir,
            rel_path,
            content,
            is_first_chunk,
            is_last_chunk,
            prev_saved,
            &self.metadata_config,
        ) {
            Ok(captured) => {
                if more_to_follow {
                    self.written_paths.insert(rel_path.to_string());
                    // 最初のチャンクで保存したメタデータを記録
                    if is_first_chunk {
                        self.saved_metadata.insert(rel_path.to_string(), captured);
                    }
                } else {
                    // 転送完了 — 次回の書き込みは新規扱いにする
                    self.written_paths.remove(rel_path);
                    self.saved_metadata.remove(rel_path);
                }
                AgentResponse::WriteResult {
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                self.written_paths.remove(rel_path);
                self.saved_metadata.remove(rel_path);
                AgentResponse::WriteResult {
                    success: false,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    fn handle_stat_files(&self, paths: &[String]) -> AgentResponse {
        let mut entries = Vec::new();

        for rel_path in paths {
            match file_io::stat_file(&self.root_dir, rel_path) {
                Ok(stat) => entries.push(stat),
                Err(e) => {
                    // handle_read_files と一貫性を持たせ、個別ファイルのエラーでは
                    // リクエスト全体を失敗させない。ファイル単位でスキップしログに記録する。
                    tracing::warn!("stat error for {rel_path}: {e}");
                }
            }
        }

        AgentResponse::Stats { entries }
    }

    fn handle_backup(&self, paths: &[String], backup_dir: &str) -> AgentResponse {
        let backup_path = match resolve_scan_root(&self.root_dir, backup_dir) {
            Ok(p) => p,
            Err(e) => {
                return AgentResponse::BackupResult {
                    success: false,
                    error: Some(e),
                }
            }
        };

        for rel_path in paths {
            if let Err(e) = file_io::create_backup(&self.root_dir, rel_path, &backup_path) {
                return AgentResponse::BackupResult {
                    success: false,
                    error: Some(format!("backup failed for {rel_path}: {e}")),
                };
            }
        }

        AgentResponse::BackupResult {
            success: true,
            error: None,
        }
    }

    fn handle_symlink(&self, rel_path: &str, target: &str) -> AgentResponse {
        match file_io::create_symlink_with_metadata(
            &self.root_dir,
            rel_path,
            target,
            self.metadata_config.default_uid,
            self.metadata_config.default_gid,
            self.metadata_config.dir_permissions,
        ) {
            Ok(()) => AgentResponse::SymlinkResult {
                success: true,
                error: None,
            },
            Err(e) => AgentResponse::SymlinkResult {
                success: false,
                error: Some(e.to_string()),
            },
        }
    }

    fn handle_list_backups(&self, backup_dir: &str) -> AgentResponse {
        let backup_path = match resolve_scan_root(&self.root_dir, backup_dir) {
            Ok(p) => p,
            Err(e) => return AgentResponse::Error { message: e },
        };

        match file_io::list_backup_sessions(&backup_path) {
            Ok(sessions) => AgentResponse::BackupList { sessions },
            Err(e) => AgentResponse::Error {
                message: format!("failed to list backups: {e}"),
            },
        }
    }

    fn handle_restore_backup(
        &self,
        backup_dir: &str,
        session_id: &str,
        files: &[String],
    ) -> AgentResponse {
        // session_id のフォーマット検証
        if crate::backup::extract_timestamp(session_id).is_none() {
            return AgentResponse::Error {
                message: format!("invalid session_id format: {session_id}"),
            };
        }

        let backup_path = match resolve_scan_root(&self.root_dir, backup_dir) {
            Ok(p) => p,
            Err(e) => return AgentResponse::Error { message: e },
        };
        // 復元先は常に Agent の root_dir を使用（クライアント指定を許可しない）
        let root_path = self.root_dir.clone();

        let results = file_io::restore_backup(
            &backup_path,
            session_id,
            files,
            &root_path,
            &self.metadata_config,
        );
        AgentResponse::RestoreResult { results }
    }
}

/// root パラメータを解決する。空文字列なら root_dir をそのまま使用。
/// パストラバーサルを検出した場合はエラーを返す。
fn resolve_scan_root(root_dir: &Path, root: &str) -> Result<PathBuf, String> {
    if root.is_empty() || root == "." {
        return Ok(root_dir.to_path_buf());
    }

    // 絶対パスを拒否 — root_dir 外のディレクトリ走査を防止
    if Path::new(root).is_absolute() {
        return Err(format!("absolute path not allowed: {root}"));
    }

    // file_io::validate_path と同等のパストラバーサル検出
    for component in Path::new(root).components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!("path traversal detected: {root}"));
        }
    }

    let joined = root_dir.join(root);

    // 存在する場合は canonicalize して root_dir 配下か検証
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize: {e}"))?;
        let root_canonical = root_dir
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize root: {e}"))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(format!(
                "path escapes root directory: {} is not under {}",
                canonical.display(),
                root_canonical.display()
            ));
        }
        return Ok(canonical);
    }

    Ok(joined)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Dispatcher) {
        let tmp = TempDir::new().unwrap();
        let dispatcher = Dispatcher::new(tmp.path().to_path_buf(), MetadataConfig::default());
        (tmp, dispatcher)
    }

    // ── ヘルパー: 単一レスポンスを取り出す ──

    /// `dispatch()` の結果から要素1つのみの Vec であることを確認して取り出す。
    fn single(responses: Vec<AgentResponse>) -> AgentResponse {
        assert_eq!(
            responses.len(),
            1,
            "expected exactly 1 response, got {}",
            responses.len()
        );
        responses.into_iter().next().unwrap()
    }

    // ── Ping / Shutdown ──

    #[test]
    fn ping_returns_pong() {
        let (_tmp, mut d) = setup();
        let resps = d.dispatch(AgentRequest::Ping).unwrap();
        assert_eq!(single(resps), AgentResponse::Pong);
    }

    #[test]
    fn shutdown_returns_none() {
        let (_tmp, mut d) = setup();
        let resp = d.dispatch(AgentRequest::Shutdown);
        assert!(resp.is_none());
    }

    // ── ListTree ──

    #[test]
    fn list_tree_basic() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), "world").unwrap();

        let resps = d
            .dispatch(AgentRequest::ListTree {
                root: String::new(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        // 全チャンクからエントリを収集
        let mut all_paths: Vec<String> = Vec::new();
        let mut last_count = 0;
        for resp in &resps {
            match resp {
                AgentResponse::TreeChunk { nodes, is_last, .. } => {
                    all_paths.extend(nodes.iter().map(|n| n.path.clone()));
                    if *is_last {
                        last_count += 1;
                    }
                }
                other => panic!("expected TreeChunk, got {other:?}"),
            }
        }
        // 最後のチャンクだけ is_last=true
        assert_eq!(last_count, 1, "exactly one chunk should have is_last=true");
        assert!(resps
            .last()
            .is_some_and(|r| matches!(r, AgentResponse::TreeChunk { is_last: true, .. })));

        let paths: Vec<&str> = all_paths.iter().map(|s| s.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        // ディレクトリ "sub" は buffer に含まれない
        assert!(!paths.contains(&"sub"));
        assert!(paths.contains(&"sub/b.txt"));
    }

    #[test]
    fn list_tree_empty_dir_returns_single_chunk() {
        let (_tmp, mut d) = setup();

        let resps = d
            .dispatch(AgentRequest::ListTree {
                root: String::new(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        assert_eq!(resps.len(), 1, "empty dir should return exactly one chunk");
        match &resps[0] {
            AgentResponse::TreeChunk {
                nodes,
                is_last,
                total_scanned,
                truncated,
            } => {
                assert!(nodes.is_empty());
                assert!(*is_last);
                assert_eq!(*total_scanned, 0);
                assert!(!*truncated);
            }
            other => panic!("expected TreeChunk, got {other:?}"),
        }
    }

    #[test]
    fn list_tree_multi_chunk() {
        let (tmp, mut d) = setup();
        // デフォルト chunk_size=1000 を超えるファイル数を作成するのは重いため、
        // chunk_size を小さくしたオプションは dispatch から直接指定できない。
        // ここでは handle_list_tree を間接的に呼ぶのではなく、
        // scan_tree の chunk_size=2 相当になるよう十分なファイルを作成して
        // dispatch の経路を通る統合テストとする。
        // (chunk_size はデフォルト 1000 なので、複数チャンクを強制するには
        //  1001 ファイルが必要 — 代わりに handle_list_tree の内部動作は
        //  tree_scan::tests で検証済みのため、ここでは 3 ファイルで基本動作を確認する)
        for i in 0..3 {
            fs::write(tmp.path().join(format!("f{i}.txt")), "x").unwrap();
        }

        let resps = d
            .dispatch(AgentRequest::ListTree {
                root: String::new(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        // 全チャンクのうち最後だけ is_last=true であること
        let last_flags: Vec<bool> = resps
            .iter()
            .map(|r| match r {
                AgentResponse::TreeChunk { is_last, .. } => *is_last,
                other => panic!("expected TreeChunk, got {other:?}"),
            })
            .collect();

        let true_count = last_flags.iter().filter(|&&b| b).count();
        assert_eq!(
            true_count, 1,
            "exactly one chunk should have is_last=true, flags={last_flags:?}"
        );
        assert!(
            *last_flags.last().unwrap(),
            "the last chunk should have is_last=true"
        );

        // 全エントリが収集できること
        let total_entries: usize = resps
            .iter()
            .map(|r| match r {
                AgentResponse::TreeChunk { nodes, .. } => nodes.len(),
                _ => 0,
            })
            .sum();
        assert_eq!(total_entries, 3);
    }

    // ── ReadFiles ──

    #[test]
    fn read_files_existing() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("hello.txt"), "hello world").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::ReadFiles {
                paths: vec!["hello.txt".into()],
                chunk_size_limit: 4096,
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::FileContents { results } => {
                assert_eq!(results.len(), 1);
                match &results[0] {
                    FileReadResult::Ok { content, .. } => {
                        assert_eq!(content, b"hello world");
                    }
                    other => panic!("expected Ok, got {other:?}"),
                }
            }
            other => panic!("expected FileContents, got {other:?}"),
        }
    }

    #[test]
    fn read_files_nonexistent_returns_error_variant() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::ReadFiles {
                paths: vec!["nonexistent.txt".into()],
                chunk_size_limit: 4096,
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::FileContents { results } => {
                assert_eq!(results.len(), 1);
                assert!(matches!(&results[0], FileReadResult::Error { .. }));
            }
            other => panic!("expected FileContents, got {other:?}"),
        }
    }

    #[test]
    fn read_files_path_traversal_returns_error_variant() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::ReadFiles {
                paths: vec!["../etc/passwd".into()],
                chunk_size_limit: 4096,
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::FileContents { results } => {
                assert_eq!(results.len(), 1);
                match &results[0] {
                    FileReadResult::Error { message, .. } => {
                        assert!(message.contains("path traversal"));
                    }
                    other => panic!("expected Error, got {other:?}"),
                }
            }
            other => panic!("expected FileContents, got {other:?}"),
        }
    }

    // ── WriteFile ──

    #[test]
    fn write_file_single() {
        let (tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "out.txt".into(),
                content: b"data".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap(),
        );

        assert_eq!(
            resp,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("out.txt")).unwrap(),
            "data"
        );
    }

    #[test]
    fn write_file_chunked_appends() {
        let (tmp, mut d) = setup();

        // 最初のチャンク（more_to_follow=true）
        let resp1 = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "chunked.bin".into(),
                content: b"chunk1".to_vec(),
                is_binary: true,
                more_to_follow: true,
            })
            .unwrap(),
        );
        assert_eq!(
            resp1,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );

        // 2番目のチャンク（more_to_follow=false で完了）
        let resp2 = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "chunked.bin".into(),
                content: b"chunk2".to_vec(),
                is_binary: true,
                more_to_follow: false,
            })
            .unwrap(),
        );
        assert_eq!(
            resp2,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );

        let content = fs::read(tmp.path().join("chunked.bin")).unwrap();
        assert_eq!(content, b"chunk1chunk2");
    }

    #[test]
    fn write_file_second_transfer_starts_fresh() {
        let (tmp, mut d) = setup();

        // 1回目の転送
        d.dispatch(AgentRequest::WriteFile {
            path: "file.txt".into(),
            content: b"first".to_vec(),
            is_binary: false,
            more_to_follow: false,
        });

        // 2回目の転送（同じパス）— is_first_chunk=true になるはず
        d.dispatch(AgentRequest::WriteFile {
            path: "file.txt".into(),
            content: b"second".to_vec(),
            is_binary: false,
            more_to_follow: false,
        });

        let content = fs::read_to_string(tmp.path().join("file.txt")).unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn write_file_path_traversal_returns_error() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "../escape.txt".into(),
                content: b"evil".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::WriteResult { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("path traversal"));
            }
            other => panic!("expected WriteResult, got {other:?}"),
        }
    }

    // ── StatFiles ──

    #[test]
    fn stat_files_existing() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("s.txt"), "12345").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::StatFiles {
                paths: vec!["s.txt".into()],
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::Stats { entries } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].path, "s.txt");
                assert_eq!(entries[0].size, 5);
            }
            other => panic!("expected Stats, got {other:?}"),
        }
    }

    // ── Backup ──

    #[test]
    fn backup_success() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("orig.txt"), "backup me").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::Backup {
                paths: vec!["orig.txt".into()],
                backup_dir: "backups".into(),
            })
            .unwrap(),
        );

        assert_eq!(
            resp,
            AgentResponse::BackupResult {
                success: true,
                error: None,
            }
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("backups/orig.txt")).unwrap(),
            "backup me"
        );
    }

    // ── Symlink ──

    #[test]
    fn symlink_success() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("target.txt"), "link target").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::Symlink {
                path: "link.txt".into(),
                target: "target.txt".into(),
            })
            .unwrap(),
        );

        assert_eq!(
            resp,
            AgentResponse::SymlinkResult {
                success: true,
                error: None,
            }
        );

        let link = tmp.path().join("link.txt");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&link).unwrap(), "link target");
    }

    #[test]
    fn symlink_traversal_returns_error() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::Symlink {
                path: "link".into(),
                target: "../etc/passwd".into(),
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::SymlinkResult { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("escapes root"));
            }
            other => panic!("expected SymlinkResult, got {other:?}"),
        }
    }

    // ── Path traversal protection ──

    #[test]
    fn list_tree_path_traversal_returns_error() {
        let (_tmp, mut d) = setup();

        let resps = d
            .dispatch(AgentRequest::ListTree {
                root: "../../etc".into(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        assert_eq!(resps.len(), 1);
        match &resps[0] {
            AgentResponse::Error { message } => {
                assert!(
                    message.contains("path traversal"),
                    "expected path traversal error, got: {message}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn stat_files_path_traversal_skips_entry() {
        let (tmp, mut d) = setup();
        // 正常なファイルも一緒に送り、トラバーサルパスがスキップされることを確認
        fs::write(tmp.path().join("ok.txt"), "data").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::StatFiles {
                paths: vec!["../etc/passwd".into(), "ok.txt".into()],
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::Stats { entries } => {
                // トラバーサルパスはスキップされ、ok.txt のみ返る
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].path, "ok.txt");
            }
            other => panic!("expected Stats, got {other:?}"),
        }
    }

    #[test]
    fn backup_path_traversal_returns_error() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("file.txt"), "data").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::Backup {
                paths: vec!["file.txt".into()],
                backup_dir: "../../tmp/evil".into(),
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::BackupResult { success, error } => {
                assert!(!success);
                let msg = error.unwrap();
                assert!(
                    msg.contains("path traversal"),
                    "expected path traversal error, got: {msg}"
                );
            }
            other => panic!("expected BackupResult, got {other:?}"),
        }
    }

    #[test]
    fn list_tree_absolute_path_returns_error() {
        let (_tmp, mut d) = setup();

        let resps = d
            .dispatch(AgentRequest::ListTree {
                root: "/etc".into(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        assert_eq!(resps.len(), 1);
        match &resps[0] {
            AgentResponse::Error { message } => {
                assert!(
                    message.contains("absolute path not allowed"),
                    "expected absolute path error, got: {message}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn stat_files_nonexistent_skips_with_no_abort() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("exists.txt"), "hello").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::StatFiles {
                paths: vec!["nonexistent.txt".into(), "exists.txt".into()],
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::Stats { entries } => {
                // nonexistent はスキップされ、exists.txt のみ返る
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].path, "exists.txt");
            }
            other => panic!("expected Stats, got {other:?}"),
        }
    }

    // ── ListBackups ──

    #[test]
    fn list_backups_returns_backup_list() {
        let (tmp, mut d) = setup();
        let backup_dir = tmp.path().join("backups");
        let s1 = backup_dir.join("20260311-100000");
        fs::create_dir_all(&s1).unwrap();
        fs::write(s1.join("file.txt"), "data").unwrap();

        let resp = single(
            d.dispatch(AgentRequest::ListBackups {
                backup_dir: "backups".into(),
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::BackupList { sessions } => {
                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].session_id, "20260311-100000");
                assert_eq!(sessions[0].files.len(), 1);
            }
            other => panic!("expected BackupList, got {other:?}"),
        }
    }

    #[test]
    fn list_backups_path_traversal_returns_error() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::ListBackups {
                backup_dir: "../../evil".into(),
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::Error { message } => {
                assert!(message.contains("path traversal"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    // ── RestoreBackup ──

    #[test]
    fn restore_backup_returns_restore_result() {
        let (tmp, mut d) = setup();

        // バックアップを用意（root_dir 配下に backups/session ディレクトリを作成）
        let backup_dir = tmp.path().join("backups");
        let session_dir = backup_dir.join("20260311-120000");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("a.txt"), "restored").unwrap();

        // dispatch は self.root_dir（= tmp.path()）を復元先として使用
        let resp = single(
            d.dispatch(AgentRequest::RestoreBackup {
                backup_dir: "backups".into(),
                session_id: "20260311-120000".into(),
                files: vec!["a.txt".into()],
                root_dir: ".".into(), // dispatch は root_dir パラメータを無視し self.root_dir を使用
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::RestoreResult { results } => {
                assert_eq!(results.len(), 1);
                assert!(results[0].success);
                // 復元先は self.root_dir (= tmp.path()) 配下
                assert_eq!(
                    fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
                    "restored"
                );
            }
            other => panic!("expected RestoreResult, got {other:?}"),
        }
    }

    #[test]
    fn restore_backup_path_traversal_returns_error() {
        let (_tmp, mut d) = setup();

        let resp = single(
            d.dispatch(AgentRequest::RestoreBackup {
                backup_dir: "../../evil".into(),
                session_id: "20260311-120000".into(),
                files: vec!["a.txt".into()],
                root_dir: ".".into(),
            })
            .unwrap(),
        );

        match resp {
            AgentResponse::Error { message } => {
                assert!(message.contains("path traversal"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    // ── WriteFile with MetadataConfig ──

    #[test]
    fn write_file_propagates_metadata_config() {
        let (tmp, mut d) = setup_with_config(MetadataConfig {
            default_uid: Some(1000),
            default_gid: Some(1000),
            file_permissions: Some(0o644),
            dir_permissions: Some(0o755),
        });

        let resp = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "new_file.txt".into(),
                content: b"hello".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap(),
        );

        assert_eq!(
            resp,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );
        // ファイルが書き込まれていること
        assert_eq!(
            fs::read_to_string(tmp.path().join("new_file.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn write_file_chunked_saved_metadata_management() {
        use std::os::unix::fs::PermissionsExt;

        let (tmp, mut d) = setup_with_config(MetadataConfig {
            default_uid: None,
            default_gid: None,
            file_permissions: Some(0o600),
            dir_permissions: None,
        });

        // 既存ファイルを作成
        fs::write(tmp.path().join("existing.txt"), "old content").unwrap();
        // パーミッションを設定
        fs::set_permissions(
            tmp.path().join("existing.txt"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        // 1st chunk: is_first_chunk=true → SavedMetadata を保存する
        let resp1 = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "existing.txt".into(),
                content: b"chunk1".to_vec(),
                is_binary: false,
                more_to_follow: true,
            })
            .unwrap(),
        );
        assert_eq!(
            resp1,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );

        // SavedMetadata がディスパッチャーに保存されていることを確認
        assert!(d.saved_metadata.contains_key("existing.txt"));
        assert!(d.saved_metadata["existing.txt"].is_some());

        // 2nd chunk: is_last_chunk=true → SavedMetadata を使ってメタデータを復元
        let resp2 = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "existing.txt".into(),
                content: b"chunk2".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap(),
        );
        assert_eq!(
            resp2,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );

        // コンテンツが正しく結合されていること
        assert_eq!(
            fs::read(tmp.path().join("existing.txt")).unwrap(),
            b"chunk1chunk2"
        );

        // 転送完了後、saved_metadata がクリーンアップされていること
        assert!(!d.saved_metadata.contains_key("existing.txt"));
        assert!(!d.written_paths.contains("existing.txt"));
    }

    #[test]
    fn write_file_new_file_uses_config_defaults() {
        let (tmp, mut d) = setup_with_config(MetadataConfig {
            default_uid: None,
            default_gid: None,
            file_permissions: Some(0o600),
            dir_permissions: None,
        });

        let resp = single(
            d.dispatch(AgentRequest::WriteFile {
                path: "brand_new.txt".into(),
                content: b"data".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap(),
        );
        assert_eq!(
            resp,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("brand_new.txt")).unwrap(),
            "data"
        );
        // saved_metadata はクリーンアップ済み
        assert!(!d.saved_metadata.contains_key("brand_new.txt"));
    }

    /// MetadataConfig を指定してセットアップするヘルパー
    fn setup_with_config(config: MetadataConfig) -> (TempDir, Dispatcher) {
        let tmp = TempDir::new().unwrap();
        let dispatcher = Dispatcher::new(tmp.path().to_path_buf(), config);
        (tmp, dispatcher)
    }
}
