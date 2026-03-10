//! Agent ディスパッチャー。
//!
//! `AgentRequest` を受け取り、適切なハンドラに振り分けて `AgentResponse` を返す。
//! サービス層として動作し、stdin/stdout の知識は持たない。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::file_io;
use super::protocol::{AgentFileEntry, AgentRequest, AgentResponse, FileReadResult};
use super::tree_scan::{self, ScanOptions};

/// Agent ディスパッチャー。root_dir を保持し、リクエストを適切なハンドラに振り分ける。
pub struct Dispatcher {
    root_dir: PathBuf,
    /// チャンク書き込みで最初の書き込み済みパスを追跡する
    written_paths: HashSet<String>,
}

impl Dispatcher {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            written_paths: HashSet::new(),
        }
    }

    /// リクエストを処理してレスポンスを返す。
    /// Shutdown の場合は None を返す（呼び出し元がループを終了する合図）。
    pub fn dispatch(&mut self, request: AgentRequest) -> Option<AgentResponse> {
        match request {
            AgentRequest::Ping => Some(AgentResponse::Pong),
            AgentRequest::Shutdown => None,
            AgentRequest::ListTree {
                root,
                exclude,
                max_entries,
            } => Some(self.handle_list_tree(&root, &exclude, max_entries)),
            AgentRequest::ReadFiles {
                paths,
                chunk_size_limit,
            } => Some(self.handle_read_files(&paths, chunk_size_limit)),
            AgentRequest::WriteFile {
                path,
                content,
                // TODO: is_binary は将来のバイナリファイル最適化で使用予定
                is_binary: _,
                more_to_follow,
            } => Some(self.handle_write_file(&path, &content, more_to_follow)),
            AgentRequest::StatFiles { paths } => Some(self.handle_stat_files(&paths)),
            AgentRequest::Backup { paths, backup_dir } => {
                Some(self.handle_backup(&paths, &backup_dir))
            }
            AgentRequest::Symlink { path, target } => Some(self.handle_symlink(&path, &target)),
        }
    }

    fn handle_list_tree(
        &self,
        root: &str,
        exclude: &[String],
        max_entries: usize,
    ) -> AgentResponse {
        let scan_root = match resolve_scan_root(&self.root_dir, root) {
            Ok(p) => p,
            Err(e) => return AgentResponse::Error { message: e },
        };
        let options = ScanOptions {
            root: scan_root,
            exclude: exclude.to_vec(),
            max_entries,
            ..Default::default()
        };

        let mut all_entries: Vec<AgentFileEntry> = Vec::new();
        let mut total_scanned = 0;

        for chunk_result in tree_scan::scan_tree(&options) {
            match chunk_result {
                Ok(chunk) => {
                    total_scanned = chunk.total_scanned;
                    all_entries.extend(chunk.entries);
                }
                Err(e) => {
                    return AgentResponse::Error {
                        message: format!("tree scan error: {e}"),
                    };
                }
            }
        }

        AgentResponse::TreeChunk {
            nodes: all_entries,
            is_last: true,
            total_scanned,
        }
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

        match file_io::write_file(&self.root_dir, rel_path, content, is_first_chunk) {
            Ok(()) => {
                if more_to_follow {
                    self.written_paths.insert(rel_path.to_string());
                } else {
                    // 転送完了 — 次回の書き込みは新規扱いにする
                    self.written_paths.remove(rel_path);
                }
                AgentResponse::WriteResult {
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                self.written_paths.remove(rel_path);
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
        match file_io::create_symlink(&self.root_dir, rel_path, target) {
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
}

/// root パラメータを解決する。空文字列なら root_dir をそのまま使用。
/// パストラバーサルを検出した場合はエラーを返す。
fn resolve_scan_root(root_dir: &Path, root: &str) -> Result<PathBuf, String> {
    if root.is_empty() || root == "." {
        return Ok(root_dir.to_path_buf());
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
        let dispatcher = Dispatcher::new(tmp.path().to_path_buf());
        (tmp, dispatcher)
    }

    // ── Ping / Shutdown ──

    #[test]
    fn ping_returns_pong() {
        let (_tmp, mut d) = setup();
        let resp = d.dispatch(AgentRequest::Ping);
        assert_eq!(resp, Some(AgentResponse::Pong));
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

        let resp = d
            .dispatch(AgentRequest::ListTree {
                root: String::new(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        match resp {
            AgentResponse::TreeChunk { nodes, is_last, .. } => {
                assert!(is_last);
                let paths: Vec<&str> = nodes.iter().map(|n| n.path.as_str()).collect();
                assert!(paths.contains(&"a.txt"));
                assert!(paths.contains(&"sub"));
                assert!(paths.contains(&"sub/b.txt"));
            }
            other => panic!("expected TreeChunk, got {other:?}"),
        }
    }

    // ── ReadFiles ──

    #[test]
    fn read_files_existing() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("hello.txt"), "hello world").unwrap();

        let resp = d
            .dispatch(AgentRequest::ReadFiles {
                paths: vec!["hello.txt".into()],
                chunk_size_limit: 4096,
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::ReadFiles {
                paths: vec!["nonexistent.txt".into()],
                chunk_size_limit: 4096,
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::ReadFiles {
                paths: vec!["../etc/passwd".into()],
                chunk_size_limit: 4096,
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::WriteFile {
                path: "out.txt".into(),
                content: b"data".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap();

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
        let resp1 = d
            .dispatch(AgentRequest::WriteFile {
                path: "chunked.bin".into(),
                content: b"chunk1".to_vec(),
                is_binary: true,
                more_to_follow: true,
            })
            .unwrap();
        assert_eq!(
            resp1,
            AgentResponse::WriteResult {
                success: true,
                error: None,
            }
        );

        // 2番目のチャンク（more_to_follow=false で完了）
        let resp2 = d
            .dispatch(AgentRequest::WriteFile {
                path: "chunked.bin".into(),
                content: b"chunk2".to_vec(),
                is_binary: true,
                more_to_follow: false,
            })
            .unwrap();
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

        let resp = d
            .dispatch(AgentRequest::WriteFile {
                path: "../escape.txt".into(),
                content: b"evil".to_vec(),
                is_binary: false,
                more_to_follow: false,
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::StatFiles {
                paths: vec!["s.txt".into()],
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::Backup {
                paths: vec!["orig.txt".into()],
                backup_dir: "backups".into(),
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::Symlink {
                path: "link.txt".into(),
                target: "target.txt".into(),
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::Symlink {
                path: "link".into(),
                target: "../etc/passwd".into(),
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::ListTree {
                root: "../../etc".into(),
                exclude: vec![],
                max_entries: 10000,
            })
            .unwrap();

        match resp {
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

        let resp = d
            .dispatch(AgentRequest::StatFiles {
                paths: vec!["../etc/passwd".into(), "ok.txt".into()],
            })
            .unwrap();

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

        let resp = d
            .dispatch(AgentRequest::Backup {
                paths: vec!["file.txt".into()],
                backup_dir: "../../tmp/evil".into(),
            })
            .unwrap();

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
    fn stat_files_nonexistent_skips_with_no_abort() {
        let (tmp, mut d) = setup();
        fs::write(tmp.path().join("exists.txt"), "hello").unwrap();

        let resp = d
            .dispatch(AgentRequest::StatFiles {
                paths: vec!["nonexistent.txt".into(), "exists.txt".into()],
            })
            .unwrap();

        match resp {
            AgentResponse::Stats { entries } => {
                // nonexistent はスキップされ、exists.txt のみ返る
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].path, "exists.txt");
            }
            other => panic!("expected Stats, got {other:?}"),
        }
    }
}
