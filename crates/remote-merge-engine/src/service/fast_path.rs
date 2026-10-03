//! Fast path スキャン戦略の判定ロジック。
//!
//! CLI の diff/merge/sync コマンドで共通のスキャン戦略判定を行い、
//! 分岐条件のずれを防止する。全て純粋関数。

use super::path_resolver::check_path_traversal;

/// fast path のパス数上限。超えると FullScan にフォールバック。
pub const FAST_PATH_MAX_PATHS: usize = 20;

/// CLI のパス指定に基づくスキャン戦略。
/// diff/merge/sync の3コマンドで共通の戦略判定を行い、分岐条件のずれを防止する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanStrategy {
    /// 対象ファイルを直接読む（ツリースキャンなし）
    FastPath(Vec<String>),
    /// 指定ディレクトリ配下のみツリー取得
    PartialScan(Vec<String>),
    /// 全ツリー走査
    FullScan,
}

/// パスリストとフラグからスキャン戦略を判定する純粋関数。
///
/// 判定ロジック:
/// - paths が空 → FullScan
/// - "." 含む → FullScan
/// - --delete フラグ → FullScan（RightOnly を知る必要があるため）
/// - glob 文字（*, ?, [）含む → FullScan
/// - 空文字列を含む → FullScan
/// - 20パス超 → FullScan（stat + read の 2N SSH exec を抑制）
/// - 末尾 "/" のパスがある（全パスがディレクトリ形式）→ PartialScan
/// - ファイルとディレクトリ（末尾 "/"）が混在 → FullScan
/// - それ以外 → FastPath
///
/// Note: stat は I/O なのでこの関数内では行わない。ディレクトリ判定は末尾 "/" のヒントのみ使用。
/// stat でディレクトリと判明した場合は呼び出し元が PartialScan に切り替える。
pub fn resolve_scan_strategy(paths: &[String], has_delete_flag: bool) -> ScanStrategy {
    // 空パス → FullScan
    if paths.is_empty() {
        return ScanStrategy::FullScan;
    }

    // --delete フラグ → FullScan（RightOnly を知る必要がある）
    if has_delete_flag {
        return ScanStrategy::FullScan;
    }

    // "." や "./" を含む → FullScan
    if paths.iter().any(|p| is_root_marker(p)) {
        return ScanStrategy::FullScan;
    }

    // 空文字列を含む → FullScan
    if paths.iter().any(|p| p.is_empty()) {
        return ScanStrategy::FullScan;
    }

    // glob 文字を含む → FullScan
    if paths.iter().any(|p| has_glob_chars(p)) {
        return ScanStrategy::FullScan;
    }

    // パストラバーサルチェック（"." 以外の ".." を検出）
    if check_path_traversal(paths).is_err() {
        return ScanStrategy::FullScan;
    }

    // パス数上限チェック
    if paths.len() > FAST_PATH_MAX_PATHS {
        return ScanStrategy::FullScan;
    }

    // ディレクトリ（末尾 "/"）とファイルの分類
    let has_dirs = paths.iter().any(|p| p.ends_with('/'));
    let has_files = paths.iter().any(|p| !p.ends_with('/'));

    if has_dirs && has_files {
        // ファイルとディレクトリが混在 → FullScan
        return ScanStrategy::FullScan;
    }

    if has_dirs {
        // 全パスがディレクトリ形式 → PartialScan
        let dir_paths: Vec<String> = paths.to_vec();
        return ScanStrategy::PartialScan(dir_paths);
    }

    // 全パスがファイル形式 → FastPath
    ScanStrategy::FastPath(paths.to_vec())
}

/// パスが "." や "./" などのルートマーカーかどうかを判定する。
fn is_root_marker(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    trimmed == "." || trimmed.is_empty()
}

/// パスに glob 文字（*, ?, [）が含まれているかを判定する。
fn has_glob_chars(path: &str) -> bool {
    path.chars().any(|c| c == '*' || c == '?' || c == '[')
}

/// FastPath のファイルパスから親ディレクトリのリストを生成する（重複排除）。
///
/// ルート直下のファイル（スラッシュを含まない）は `"./"` を返す。
/// 呼び出し元で `"./"` の有無をチェックし、FullScan へのフォールバックを判断する。
pub fn fast_path_to_parent_dirs(paths: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in paths {
        let parent = if let Some(idx) = path.rfind('/') {
            format!("{}/", &path[..idx])
        } else {
            // ルート直下のファイル → "./" で PartialScan
            "./".to_string()
        };
        if seen.insert(parent.clone()) {
            dirs.push(parent);
        }
    }
    dirs
}

/// `fast_path_to_parent_dirs` の結果にルート直下マーカー（"./" や "."）が含まれるかを判定する。
///
/// ルート直下ファイルの PartialScan は FullScan 相当になるため、
/// 呼び出し元でこれを検知して FullScan にフォールバックする。
pub fn has_root_parent_dir(parent_dirs: &[String]) -> bool {
    parent_dirs
        .iter()
        .any(|d| d.trim_end_matches('/') == "." || d.trim_end_matches('/').is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── resolve_scan_strategy ──

    #[test]
    fn empty_paths_returns_full_scan() {
        assert_eq!(resolve_scan_strategy(&[], false), ScanStrategy::FullScan);
    }

    #[test]
    fn dot_returns_full_scan() {
        let paths = vec![".".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn dot_slash_returns_full_scan() {
        let paths = vec!["./".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn single_file_returns_fast_path() {
        let paths = vec!["file.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&paths, false),
            ScanStrategy::FastPath(vec!["file.txt".to_string()])
        );
    }

    #[test]
    fn single_dir_returns_partial_scan() {
        let paths = vec!["dir/".to_string()];
        assert_eq!(
            resolve_scan_strategy(&paths, false),
            ScanStrategy::PartialScan(vec!["dir/".to_string()])
        );
    }

    #[test]
    fn mixed_file_and_dir_returns_full_scan() {
        let paths = vec!["file.txt".to_string(), "dir/".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn glob_star_returns_full_scan() {
        let paths = vec!["*.php".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn glob_bracket_returns_full_scan() {
        let paths = vec!["file[1].txt".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn glob_question_returns_full_scan() {
        let paths = vec!["file?.txt".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn over_limit_returns_full_scan() {
        let paths: Vec<String> = (0..21).map(|i| format!("file_{}.txt", i)).collect();
        assert_eq!(paths.len(), 21);
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn at_limit_returns_fast_path() {
        let paths: Vec<String> = (0..20).map(|i| format!("file_{}.txt", i)).collect();
        assert_eq!(paths.len(), 20);
        match resolve_scan_strategy(&paths, false) {
            ScanStrategy::FastPath(p) => assert_eq!(p.len(), 20),
            other => panic!("expected FastPath, got {:?}", other),
        }
    }

    #[test]
    fn delete_flag_returns_full_scan() {
        let paths = vec!["file.txt".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, true), ScanStrategy::FullScan);
    }

    #[test]
    fn empty_string_returns_full_scan() {
        let paths = vec!["".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn multiple_files_returns_fast_path() {
        let paths = vec!["a/b/c.txt".to_string(), "x/y.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&paths, false),
            ScanStrategy::FastPath(vec!["a/b/c.txt".to_string(), "x/y.txt".to_string()])
        );
    }

    #[test]
    fn multiple_dirs_returns_partial_scan() {
        let paths = vec!["dir1/".to_string(), "dir2/".to_string()];
        assert_eq!(
            resolve_scan_strategy(&paths, false),
            ScanStrategy::PartialScan(vec!["dir1/".to_string(), "dir2/".to_string()])
        );
    }

    #[test]
    fn path_traversal_returns_full_scan() {
        let paths = vec!["../etc/passwd".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    #[test]
    fn nested_file_path_returns_fast_path() {
        let paths = vec!["src/service/fast_path.rs".to_string()];
        assert_eq!(
            resolve_scan_strategy(&paths, false),
            ScanStrategy::FastPath(vec!["src/service/fast_path.rs".to_string()])
        );
    }

    #[test]
    fn delete_flag_with_empty_paths_returns_full_scan() {
        assert_eq!(resolve_scan_strategy(&[], true), ScanStrategy::FullScan);
    }

    #[test]
    fn glob_in_middle_of_path_returns_full_scan() {
        let paths = vec!["src/*.rs".to_string()];
        assert_eq!(resolve_scan_strategy(&paths, false), ScanStrategy::FullScan);
    }

    // ── fast_path_to_parent_dirs ──

    #[test]
    fn parent_dirs_nested_file() {
        let paths = vec!["src/main.rs".to_string()];
        let dirs = fast_path_to_parent_dirs(&paths);
        assert_eq!(dirs, vec!["src/"]);
    }

    #[test]
    fn parent_dirs_root_file() {
        // ルート直下のファイル → "./"
        let paths = vec!["Cargo.toml".to_string()];
        let dirs = fast_path_to_parent_dirs(&paths);
        assert_eq!(dirs, vec!["./"]);
    }

    #[test]
    fn parent_dirs_deduplication() {
        // 同じディレクトリ配下のファイルは重複排除される
        let paths = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "tests/test.rs".to_string(),
        ];
        let dirs = fast_path_to_parent_dirs(&paths);
        assert_eq!(dirs, vec!["src/", "tests/"]);
    }

    #[test]
    fn parent_dirs_deeply_nested() {
        let paths = vec!["a/b/c/d.txt".to_string()];
        let dirs = fast_path_to_parent_dirs(&paths);
        assert_eq!(dirs, vec!["a/b/c/"]);
    }

    // ── has_root_parent_dir ──

    #[test]
    fn has_root_parent_dir_detects_dot_slash() {
        assert!(has_root_parent_dir(&["./".to_string()]));
    }

    #[test]
    fn has_root_parent_dir_detects_dot() {
        assert!(has_root_parent_dir(&[".".to_string()]));
    }

    #[test]
    fn has_root_parent_dir_no_root() {
        assert!(!has_root_parent_dir(&[
            "src/".to_string(),
            "tests/".to_string()
        ]));
    }

    #[test]
    fn has_root_parent_dir_mixed() {
        assert!(has_root_parent_dir(&["src/".to_string(), "./".to_string()]));
    }
}
