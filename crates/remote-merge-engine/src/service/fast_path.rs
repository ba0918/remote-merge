//! Fast path スキャン戦略の判定ロジック。
//!
//! CLI の diff/merge/sync コマンドで共通のスキャン戦略判定を行い、
//! 分岐条件のずれを防止する。全て純粋関数。

use remote_merge_core::relative_path::RelPath;

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
pub fn resolve_scan_strategy(paths: &[RelPath], has_delete_flag: bool) -> ScanStrategy {
    // 空パス・--delete（RightOnly を知る必要がある）・root_dir 全体・glob 文字・パス数の上限超え
    // → FullScan
    if paths.is_empty()
        || has_delete_flag
        || paths
            .iter()
            .any(|p| p.is_root() || has_glob_chars(p.as_typed()))
        || paths.len() > FAST_PATH_MAX_PATHS
    {
        return ScanStrategy::FullScan;
    }

    // ディレクトリ（末尾 "/"）とファイルの分類
    let has_dirs = paths.iter().any(RelPath::has_trailing_slash);
    let has_files = paths.iter().any(|p| !p.has_trailing_slash());
    let typed: Vec<String> = paths.iter().map(|p| p.as_typed().to_string()).collect();

    if has_dirs && has_files {
        // ファイルとディレクトリが混在 → FullScan
        return ScanStrategy::FullScan;
    }

    if has_dirs {
        // 全パスがディレクトリ形式 → PartialScan
        return ScanStrategy::PartialScan(typed);
    }

    // 全パスがファイル形式 → FastPath
    ScanStrategy::FastPath(typed)
}

/// diff の指定したパスの比べ方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffPaths {
    /// root_dir 全体を走査して比べるときの、走査に渡すパス（パスの指定がないときは空）。全体を
    /// 走査しないときは None
    pub whole: Option<Vec<RelPath>>,
    /// 一つずつ比べる、名前で指定したパス
    pub named: Vec<String>,
}

/// diff の指定したパスを、root_dir 全体の走査で比べるものと、名前で一つずつ比べるものに分ける
/// 純粋関数。
///
/// diff は名前で指定したパスを、数によらず、root_dir 全体を指すパスと一緒に指定されても一つずつ
/// 比べる（名前で指定したファイルは中身で比べ、フィルタを当てない）。パスがないとき、root_dir 全体を
/// 指すものや glob 文字を含むものがあるときは root_dir 全体も走査する。末尾の "/" は呼び出し元で
/// 外してある。root_dir 全体も走査するときは、名前で指定したパスを走査の結果と同じ形（"." の要素と
/// 続いた "/" を除いた形）にそろえ、同じファイルを二つの形で示さない。そろえると空になる名前
/// （root_dir そのもの）は全体の走査に任せる。
pub fn split_diff_paths(paths: &[RelPath]) -> DiffPaths {
    if paths.is_empty() {
        return DiffPaths {
            whole: Some(Vec::new()),
            named: Vec::new(),
        };
    }
    let (whole, named): (Vec<RelPath>, Vec<RelPath>) = paths
        .iter()
        .cloned()
        .partition(|p| p.is_root() || has_glob_chars(p.as_typed()));
    let named: Vec<String> = named.iter().map(|p| p.as_typed().to_string()).collect();
    if whole.is_empty() {
        return DiffPaths { whole: None, named };
    }
    let mut scanned_form: Vec<String> = Vec::new();
    for path in named.iter().map(|path| as_scanned(path)) {
        // "./." のように root_dir そのものを指す名前は、全体の走査がその配下を比べる
        if !path.is_empty() && !scanned_form.contains(&path) {
            scanned_form.push(path);
        }
    }
    DiffPaths {
        whole: Some(whole),
        named: scanned_form,
    }
}

/// 走査の結果と同じ形のパス。"." の要素と、続いた "/" を除く
fn as_scanned(path: &str) -> String {
    path.split('/')
        .filter(|element| !element.is_empty() && *element != ".")
        .collect::<Vec<_>>()
        .join("/")
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

    // ── split_diff_paths ──

    fn rels(paths: &[String]) -> Vec<RelPath> {
        paths.iter().map(|p| RelPath::parse(p).unwrap()).collect()
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn diff_compares_more_than_twenty_named_paths_one_by_one() {
        let paths: Vec<String> = (0..21).map(|i| format!("file_{i}.txt")).collect();
        assert_eq!(
            split_diff_paths(&rels(&paths)),
            DiffPaths {
                whole: None,
                named: paths.clone()
            }
        );
    }

    #[test]
    fn diff_scans_the_whole_root_dir_without_names() {
        for paths in [vec![], strings(&["."]), strings(&["a/*.txt"])] {
            assert_eq!(
                split_diff_paths(&rels(&paths)),
                DiffPaths {
                    whole: Some(rels(&paths)),
                    named: Vec::new()
                },
                "{paths:?}"
            );
        }
    }

    #[test]
    fn diff_compares_names_beside_the_root_dir_one_by_one() {
        assert_eq!(
            split_diff_paths(&rels(&strings(&[".", "app.log", "a/*.txt"]))),
            DiffPaths {
                whole: Some(rels(&strings(&[".", "a/*.txt"]))),
                named: strings(&["app.log"])
            }
        );
    }

    #[test]
    fn names_beside_the_root_dir_take_the_form_of_the_scanned_paths() {
        assert_eq!(
            split_diff_paths(&rels(&strings(&[".", "./a.txt", "d//./b.txt", "a.txt"]))),
            DiffPaths {
                whole: Some(rels(&strings(&["."]))),
                named: strings(&["a.txt", "d/b.txt"])
            }
        );
        assert_eq!(
            split_diff_paths(&rels(&strings(&[".", "./.", "././.", "./a.txt"]))),
            DiffPaths {
                whole: Some(rels(&strings(&["."]))),
                named: strings(&["a.txt"])
            }
        );
        assert_eq!(
            split_diff_paths(&rels(&strings(&["./a.txt"]))),
            DiffPaths {
                whole: None,
                named: strings(&["./a.txt"])
            }
        );
    }

    // ── resolve_scan_strategy ──

    #[test]
    fn empty_paths_returns_full_scan() {
        assert_eq!(resolve_scan_strategy(&[], false), ScanStrategy::FullScan);
    }

    #[test]
    fn dot_returns_full_scan() {
        let paths = vec![".".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn dot_slash_returns_full_scan() {
        let paths = vec!["./".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn single_file_returns_fast_path() {
        let paths = vec!["file.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FastPath(vec!["file.txt".to_string()])
        );
    }

    #[test]
    fn single_dir_returns_partial_scan() {
        let paths = vec!["dir/".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::PartialScan(vec!["dir/".to_string()])
        );
    }

    #[test]
    fn mixed_file_and_dir_returns_full_scan() {
        let paths = vec!["file.txt".to_string(), "dir/".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn glob_star_returns_full_scan() {
        let paths = vec!["*.php".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn glob_bracket_returns_full_scan() {
        let paths = vec!["file[1].txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn glob_question_returns_full_scan() {
        let paths = vec!["file?.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn over_limit_returns_full_scan() {
        let paths: Vec<String> = (0..21).map(|i| format!("file_{}.txt", i)).collect();
        assert_eq!(paths.len(), 21);
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn at_limit_returns_fast_path() {
        let paths: Vec<String> = (0..20).map(|i| format!("file_{}.txt", i)).collect();
        assert_eq!(paths.len(), 20);
        match resolve_scan_strategy(&rels(&paths), false) {
            ScanStrategy::FastPath(p) => assert_eq!(p.len(), 20),
            other => panic!("expected FastPath, got {:?}", other),
        }
    }

    #[test]
    fn delete_flag_returns_full_scan() {
        let paths = vec!["file.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), true),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn empty_string_returns_full_scan() {
        let paths = vec!["".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
    }

    #[test]
    fn multiple_files_returns_fast_path() {
        let paths = vec!["a/b/c.txt".to_string(), "x/y.txt".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FastPath(vec!["a/b/c.txt".to_string(), "x/y.txt".to_string()])
        );
    }

    #[test]
    fn multiple_dirs_returns_partial_scan() {
        let paths = vec!["dir1/".to_string(), "dir2/".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::PartialScan(vec!["dir1/".to_string(), "dir2/".to_string()])
        );
    }

    #[test]
    fn nested_file_path_returns_fast_path() {
        let paths = vec!["src/service/fast_path.rs".to_string()];
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
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
        assert_eq!(
            resolve_scan_strategy(&rels(&paths), false),
            ScanStrategy::FullScan
        );
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
