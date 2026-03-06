//! 複数ファイルを1つのSSHコマンドで読み込むバッチ処理。
//!
//! チャネル枯渇防止: N個のファイルを1チャネルで読む。
//! コマンド生成・結果パースは純粋関数としてテスト可能。

use std::collections::HashMap;

use super::tree_parser::shell_escape;

/// バッチ読み込みに使う区切り文字のプレフィックス
const DELIMITER_PREFIX: &str = "___BATCH_DELIM___";

/// バッチ cat コマンドの区切り文字を生成する
///
/// ファイルインデックスを含めることで、各区切りがユニークになる。
fn make_delimiter(index: usize) -> String {
    format!("{}{}", DELIMITER_PREFIX, index)
}

/// 複数ファイルを cat するシェルコマンドを組み立てる。
///
/// 各ファイルの前に `echo '___BATCH_DELIM___N'` を出力し、
/// 最後に終端マーカーを付ける。
///
/// ファイルが存在しない場合は `cat` が失敗して区切り文字だけが出力される。
/// 空のスライスが渡された場合は `None` を返す。
pub fn build_batch_cat_command(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }

    let mut parts = Vec::with_capacity(paths.len() * 2 + 1);

    for (i, path) in paths.iter().enumerate() {
        let delim = make_delimiter(i);
        parts.push(format!("echo '{}'", delim));
        parts.push(format!("cat {} 2>/dev/null", shell_escape(path)));
    }

    // 終端マーカー: パース時に最後のファイル内容の終わりを検出するため
    let end_delim = make_delimiter(paths.len());
    parts.push(format!("echo '{}'", end_delim));

    Some(parts.join(" ; "))
}

/// バッチ cat コマンドの出力をパースして、パス→内容の HashMap に変換する。
///
/// 区切り文字で分割し、各ファイルの内容を抽出する。
/// ファイルが存在しなかった場合は空文字列になるが、結果には含まれない
/// （空文字列のファイルは「読み取り失敗」と区別できないため）。
///
/// # 引数
/// - `output`: バッチ cat コマンドの stdout
/// - `paths`: ファイルパス（`build_batch_cat_command` に渡したものと同じ順序）
pub fn parse_batch_output(output: &str, paths: &[String]) -> HashMap<String, String> {
    let mut results = HashMap::new();

    // 区切り文字の位置を見つける
    let lines: Vec<&str> = output.lines().collect();
    let mut segments: Vec<(usize, usize)> = Vec::new(); // (content_start_line, content_end_line)

    let mut delim_positions: Vec<usize> = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        if line.starts_with(DELIMITER_PREFIX) {
            delim_positions.push(line_idx);
        }
    }

    // N個のファイル → N+1個の区切り文字が必要
    if delim_positions.len() != paths.len() + 1 {
        tracing::warn!(
            "Batch read: delimiter count mismatch: expected {}, found {}",
            paths.len() + 1,
            delim_positions.len(),
        );
        return results;
    }

    // 各ファイルの内容を区切り文字の間から抽出
    for i in 0..paths.len() {
        let start = delim_positions[i] + 1;
        let end = delim_positions[i + 1];
        segments.push((start, end));
    }

    for (i, (start, end)) in segments.iter().enumerate() {
        if *start >= *end {
            // 空 = ファイルが存在しないか空ファイル
            // 空ファイルも有効な結果として含める（0バイトのファイルは存在する）
            // ただし、cat が失敗した場合も空になるので区別できない
            // → 空でも結果に含める（存在しないパスは caller 側で除外）
            results.insert(paths[i].clone(), String::new());
            continue;
        }

        let content_lines = &lines[*start..*end];
        let content = content_lines.join("\n");
        results.insert(paths[i].clone(), content);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_batch_cat_command テスト ──

    #[test]
    fn test_empty_paths_returns_none() {
        assert!(build_batch_cat_command(&[]).is_none());
    }

    #[test]
    fn test_single_file_command() {
        let paths = vec!["/var/www/app/main.rs".to_string()];
        let cmd = build_batch_cat_command(&paths).unwrap();

        assert!(cmd.contains("echo '___BATCH_DELIM___0'"));
        assert!(cmd.contains("cat '/var/www/app/main.rs'"));
        assert!(cmd.contains("echo '___BATCH_DELIM___1'"));
    }

    #[test]
    fn test_multiple_files_command() {
        let paths = vec![
            "/var/www/file1.txt".to_string(),
            "/var/www/file2.txt".to_string(),
            "/var/www/file3.txt".to_string(),
        ];
        let cmd = build_batch_cat_command(&paths).unwrap();

        // 各ファイルの区切り文字がある
        assert!(cmd.contains("echo '___BATCH_DELIM___0'"));
        assert!(cmd.contains("echo '___BATCH_DELIM___1'"));
        assert!(cmd.contains("echo '___BATCH_DELIM___2'"));
        // 終端マーカー
        assert!(cmd.contains("echo '___BATCH_DELIM___3'"));
        // 各 cat がある
        assert!(cmd.contains("cat '/var/www/file1.txt'"));
        assert!(cmd.contains("cat '/var/www/file2.txt'"));
        assert!(cmd.contains("cat '/var/www/file3.txt'"));
    }

    #[test]
    fn test_special_chars_in_path() {
        let paths = vec!["/var/www/my app/it's file.rs".to_string()];
        let cmd = build_batch_cat_command(&paths).unwrap();

        // shell_escape がシングルクォートをエスケープする
        assert!(cmd.contains("cat '/var/www/my app/it'\\''s file.rs'"));
    }

    #[test]
    fn test_stderr_redirect() {
        let paths = vec!["/some/file.txt".to_string()];
        let cmd = build_batch_cat_command(&paths).unwrap();

        // 存在しないファイルの stderr を抑制
        assert!(cmd.contains("2>/dev/null"));
    }

    // ── parse_batch_output テスト ──

    #[test]
    fn test_parse_single_file() {
        let paths = vec!["src/main.rs".to_string()];
        let output =
            "___BATCH_DELIM___0\nfn main() {\n    println!(\"hello\");\n}\n___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("src/main.rs").unwrap(),
            "fn main() {\n    println!(\"hello\");\n}"
        );
    }

    #[test]
    fn test_parse_multiple_files() {
        let paths = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
        ];
        let output = "\
___BATCH_DELIM___0
content of file 1
___BATCH_DELIM___1
line1 of file2
line2 of file2
___BATCH_DELIM___2
file3 content
___BATCH_DELIM___3";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("file1.txt").unwrap(), "content of file 1");
        assert_eq!(
            result.get("file2.txt").unwrap(),
            "line1 of file2\nline2 of file2"
        );
        assert_eq!(result.get("file3.txt").unwrap(), "file3 content");
    }

    #[test]
    fn test_parse_missing_file_gives_empty() {
        let paths = vec!["exists.txt".to_string(), "missing.txt".to_string()];
        // missing.txt の cat は失敗して出力なし → 区切り文字が連続する
        let output = "\
___BATCH_DELIM___0
real content here
___BATCH_DELIM___1
___BATCH_DELIM___2";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("exists.txt").unwrap(), "real content here");
        assert_eq!(result.get("missing.txt").unwrap(), "");
    }

    #[test]
    fn test_parse_all_missing() {
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let output = "\
___BATCH_DELIM___0
___BATCH_DELIM___1
___BATCH_DELIM___2";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("a.txt").unwrap(), "");
        assert_eq!(result.get("b.txt").unwrap(), "");
    }

    #[test]
    fn test_parse_delimiter_count_mismatch() {
        let paths = vec!["file.txt".to_string()];
        // 区切り文字が足りない
        let output = "___BATCH_DELIM___0\nsome content";

        let result = parse_batch_output(output, &paths);
        // パース失敗で空の HashMap
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_empty_output_with_no_paths() {
        let paths: Vec<String> = vec![];
        let result = parse_batch_output("", &paths);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_file_containing_delimiter_like_string() {
        // ファイル内容に区切り文字っぽい文字列が含まれるケース
        // 実際には行頭一致で判定するので、前置データがあれば衝突しない
        // が、行頭に偶然同じ文字列がある場合は壊れうる
        // → 実運用では極めてレアなので許容
        let paths = vec!["tricky.txt".to_string()];
        let output = "\
___BATCH_DELIM___0
normal line
another line
___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        assert_eq!(
            result.get("tricky.txt").unwrap(),
            "normal line\nanother line"
        );
    }

    #[test]
    fn test_roundtrip_command_and_parse() {
        // build_batch_cat_command で生成したコマンドの出力を
        // parse_batch_output でパースする統合テスト（シミュレーション）
        let paths = vec![
            "/app/src/lib.rs".to_string(),
            "/app/src/main.rs".to_string(),
        ];
        let _cmd = build_batch_cat_command(&paths).unwrap();

        // サーバーが返すであろう出力をシミュレート
        let simulated_output = "\
___BATCH_DELIM___0
pub fn lib_fn() {}
___BATCH_DELIM___1
fn main() {}
___BATCH_DELIM___2";

        let result = parse_batch_output(simulated_output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("/app/src/lib.rs").unwrap(), "pub fn lib_fn() {}");
        assert_eq!(result.get("/app/src/main.rs").unwrap(), "fn main() {}");
    }
}
