//! 複数ファイルを1つのSSHコマンドで読み込むバッチ処理。
//!
//! チャネル枯渇防止: N個のファイルを1チャネルで読む。
//! コマンド生成・結果パースは純粋関数としてテスト可能。

use std::collections::HashMap;

use super::tree_parser::shell_escape;

/// バッチ読み込みに使う区切り文字のプレフィックス
const DELIMITER_PREFIX: &str = "___BATCH_DELIM___";

/// SSH バッチ読み込みのコマンド長上限（バイト）。
///
/// ARG_MAX 128KB (CentOS 5) の半分。安全マージンを確保する。
pub const SSH_BATCH_MAX_COMMAND_LEN: usize = 65536;

/// Agent バッチ読み込みのパス数上限。
///
/// Agent はシェルコマンドを使わないため ARG_MAX 制約なし。
/// プロトコルのシリアライズ負荷を考慮した値。
pub const AGENT_BATCH_MAX_PATHS: usize = 2000;

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
        // cat の後に `echo ''` で改行を保証する。
        // 末尾改行なしファイルの cat 出力直後に次の echo (区切り文字) が
        // 連結されて行頭一致で検出できなくなる問題を防ぐ。
        // パーサー側で追加された改行1つを除去して元の内容を復元する。
        parts.push(format!("cat {} 2>/dev/null ; echo ''", shell_escape(path)));
    }

    // 終端マーカー: パース時に最後のファイル内容の終わりを検出するため
    let end_delim = make_delimiter(paths.len());
    parts.push(format!("echo '{}'", end_delim));

    Some(parts.join(" ; "))
}

/// 1パスあたりのコマンド長を推定する（バイト単位）。
///
/// `build_batch_cat_command()` のロジックに基づいて、
/// 各パスが追加するコマンド長を計算する。
///
/// 各パスのコマンド部分:
///   `echo '___BATCH_DELIM___N' ; cat <escaped_path> 2>/dev/null ; echo '' ; `
fn estimate_command_len_for_path(path: &str, index: usize) -> usize {
    let delim = make_delimiter(index);
    // "echo '<delim>'" の長さ
    let echo_delim_len = "echo '".len() + delim.len() + "'".len();
    // "cat <escaped> 2>/dev/null ; echo ''" の長さ
    let escaped = shell_escape(path);
    let cat_len = "cat ".len() + escaped.len() + " 2>/dev/null ; echo ''".len();
    // セパレータ " ; " × 2（echo_delim ; cat_echo ; ）
    echo_delim_len + " ; ".len() + cat_len + " ; ".len()
}

/// 終端マーカーのコマンド長を推定する。
fn estimate_end_marker_len(path_count: usize) -> usize {
    let delim = make_delimiter(path_count);
    "echo '".len() + delim.len() + "'".len()
}

/// パスをコマンド長上限に収まるチャンクに分割する。
///
/// 各チャンクの合計コマンド長が `max_command_len` バイト以下になるように分割する。
/// 1パスだけで上限を超える場合は、そのパスだけで1チャンクにする（パニックしない）。
///
/// # 引数
/// - `paths`: 分割対象のパスリスト
/// - `max_command_len`: チャンクあたりの最大コマンド長（バイト）
pub fn chunk_paths(paths: &[String], max_command_len: usize) -> Vec<Vec<String>> {
    if paths.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut current_chunk: Vec<String> = Vec::new();
    // 現在のチャンクのコマンド長（終端マーカーを除く）
    let mut current_len: usize = 0;

    for path in paths.iter() {
        // チャンク内インデックスで推定する。
        // `build_batch_cat_command()` はチャンク内 0-origin のインデックスを使うため、
        // それと一致させる。
        let chunk_local_index = current_chunk.len();
        let path_len = estimate_command_len_for_path(path, chunk_local_index);
        let end_marker_len = estimate_end_marker_len(current_chunk.len() + 1);

        if current_chunk.is_empty() {
            // チャンクが空の場合は必ず追加（1パスで上限超えでもパニックしない）
            current_chunk.push(path.clone());
            current_len = path_len;
        } else if current_len + path_len + end_marker_len > max_command_len {
            // 上限を超えるので新しいチャンクを開始
            chunks.push(current_chunk);
            current_chunk = vec![path.clone()];
            // 新チャンクの先頭なのでインデックス 0 で再推定
            current_len = estimate_command_len_for_path(path, 0);
        } else {
            current_chunk.push(path.clone());
            current_len += path_len;
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
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

    // 区切り文字行のバイトオフセットを収集する。
    // `str::lines()` は末尾改行を消すため使わない。
    // 代わりに行頭のバイト位置を走査して区切り文字を検出し、
    // 区切り文字間の内容をバイトスライスで取り出す。
    let mut delim_ranges: Vec<(usize, usize)> = Vec::new(); // (line_start, line_end_incl_newline)

    let bytes = output.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        let line_start = pos;
        // 行末を探す
        let line_end = memchr_newline(bytes, pos);
        let line = &output[line_start..line_end];
        // 改行を含む行末位置（次の行の開始位置）
        let next_pos = if line_end < bytes.len() {
            line_end + 1
        } else {
            line_end
        };

        if line.starts_with(DELIMITER_PREFIX) {
            delim_ranges.push((line_start, next_pos));
        }
        pos = next_pos;
    }

    // N個のファイル → N+1個の区切り文字が必要
    if delim_ranges.len() != paths.len() + 1 {
        tracing::warn!(
            "Batch read: delimiter count mismatch: expected {}, found {}",
            paths.len() + 1,
            delim_ranges.len(),
        );
        return results;
    }

    // 各ファイルの内容を区切り文字の間からバイトスライスで取り出す。
    // `echo ''` で追加された末尾改行を1つ除去して元のファイル内容を復元する。
    for i in 0..paths.len() {
        let content_start = delim_ranges[i].1; // 区切り行の直後
        let content_end = delim_ranges[i + 1].0; // 次の区切り行の直前

        let content = &output[content_start..content_end];
        // echo '' が追加した末尾の \n を除去
        let content = content.strip_suffix('\n').unwrap_or(content);
        results.insert(paths[i].clone(), content.to_string());
    }

    results
}

/// バッチ cat コマンドのバイト列出力をパースして、パス→バイト列の HashMap に変換する。
///
/// `parse_batch_output` のバイト列版。バイナリファイルの内容をそのまま保持する。
/// 区切り文字（ASCII のみ）をバイト列走査で検出し、区切り間の内容を `Vec<u8>` で返す。
///
/// # 引数
/// - `output`: バッチ cat コマンドの stdout（生バイト列）
/// - `paths`: ファイルパス（`build_batch_cat_command` に渡したものと同じ順序）
pub fn parse_batch_output_bytes(output: &[u8], paths: &[String]) -> HashMap<String, Vec<u8>> {
    let mut results = HashMap::new();

    let delim_prefix_bytes = DELIMITER_PREFIX.as_bytes();

    // 区切り文字行のバイトオフセットを収集する
    let mut delim_ranges: Vec<(usize, usize)> = Vec::new(); // (line_start, line_end_incl_newline)

    let mut pos = 0;
    while pos < output.len() {
        let line_start = pos;
        let line_end = memchr_newline(output, pos);
        let line = &output[line_start..line_end];
        let next_pos = if line_end < output.len() {
            line_end + 1
        } else {
            line_end
        };

        if line.starts_with(delim_prefix_bytes) {
            delim_ranges.push((line_start, next_pos));
        }
        pos = next_pos;
    }

    // N個のファイル → N+1個の区切り文字が必要
    if delim_ranges.len() != paths.len() + 1 {
        tracing::warn!(
            "Batch read (bytes): delimiter count mismatch: expected {}, found {}",
            paths.len() + 1,
            delim_ranges.len(),
        );
        return results;
    }

    // 各ファイルの内容を区切り文字の間からバイトスライスで取り出す
    for i in 0..paths.len() {
        let content_start = delim_ranges[i].1;
        let content_end = delim_ranges[i + 1].0;

        let content = &output[content_start..content_end];
        // echo '' が追加した末尾の \n を除去
        let content = content.strip_suffix(b"\n").unwrap_or(content);
        results.insert(paths[i].clone(), content.to_vec());
    }

    results
}

/// バイト列内で `pos` から次の `\n` の位置を返す（`\n` 自体は含まない）。
fn memchr_newline(bytes: &[u8], pos: usize) -> usize {
    bytes[pos..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |offset| pos + offset)
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

    // テスト注: 実際のSSH出力では `echo DELIM` が末尾改行を付け、
    // `cat file` がファイル内容をそのまま出力する。
    // パーサーは区切り文字行の直後〜次の区切り文字行の直前をそのまま返す。
    // ファイルが末尾改行を持つ場合、content にも末尾改行が含まれる。

    #[test]
    fn test_parse_single_file() {
        let paths = vec!["src/main.rs".to_string()];
        // ファイル内容: "fn main() {\n    println!(\"hello\");\n}\n"
        // cat 出力 + echo '' の追加改行で末尾が \n\n になる
        let output =
            "___BATCH_DELIM___0\nfn main() {\n    println!(\"hello\");\n}\n\n___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 1);
        // strip_suffix で echo '' の改行が除去され、元の内容が復元される
        assert_eq!(
            result.get("src/main.rs").unwrap(),
            "fn main() {\n    println!(\"hello\");\n}\n"
        );
    }

    #[test]
    fn test_parse_multiple_files() {
        let paths = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
        ];
        // 各ファイルの cat 出力後に echo '' の改行が入る
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
        assert_eq!(result.get("file1.txt").unwrap(), "content of file 1\n");
        assert_eq!(
            result.get("file2.txt").unwrap(),
            "line1 of file2\nline2 of file2\n"
        );
        assert_eq!(result.get("file3.txt").unwrap(), "file3 content\n");
    }

    #[test]
    fn test_parse_missing_file_gives_empty() {
        let paths = vec!["exists.txt".to_string(), "missing.txt".to_string()];
        // exists.txt: cat 出力 + echo '' の改行
        // missing.txt: cat 失敗(出力なし) + echo '' の改行のみ
        let output = "\
___BATCH_DELIM___0
real content here

___BATCH_DELIM___1

___BATCH_DELIM___2";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("exists.txt").unwrap(), "real content here\n");
        // missing: echo '' の \n だけ → strip_suffix で空文字列
        assert_eq!(result.get("missing.txt").unwrap(), "");
    }

    #[test]
    fn test_parse_all_missing() {
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        // 各ファイル: cat 失敗 + echo '' の改行のみ
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
        let paths = vec!["tricky.txt".to_string()];
        // echo '' の改行が入る
        let output = "\
___BATCH_DELIM___0
normal line
another line

___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        assert_eq!(
            result.get("tricky.txt").unwrap(),
            "normal line\nanother line\n"
        );
    }

    #[test]
    fn test_roundtrip_command_and_parse() {
        let paths = vec![
            "/app/src/lib.rs".to_string(),
            "/app/src/main.rs".to_string(),
        ];
        let _cmd = build_batch_cat_command(&paths).unwrap();

        // 実際のSSH出力をシミュレート（各ファイルは末尾改行あり + echo '' の改行）
        let simulated_output = "\
___BATCH_DELIM___0
pub fn lib_fn() {}

___BATCH_DELIM___1
fn main() {}

___BATCH_DELIM___2";

        let result = parse_batch_output(simulated_output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get("/app/src/lib.rs").unwrap(),
            "pub fn lib_fn() {}\n"
        );
        assert_eq!(result.get("/app/src/main.rs").unwrap(), "fn main() {}\n");
    }

    /// 末尾改行なしのファイルでも正しくパースされること。
    /// `cat file ; echo ''` により、末尾改行なしファイルでも
    /// 必ず改行が付加される。strip_suffix でその改行を除去し、
    /// 元の内容を復元する。
    #[test]
    fn test_parse_file_without_trailing_newline() {
        let paths = vec!["no_newline.txt".to_string()];
        // ファイル内容: "hello"（末尾改行なし）
        // cat 出力: "hello" + echo '' 出力: "\n" → "hello\n"
        // strip_suffix('\n') → "hello"
        let output = "___BATCH_DELIM___0\nhello\n___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        assert_eq!(result.get("no_newline.txt").unwrap(), "hello");
    }

    /// 単一ファイル読み込み (read_file) との整合性テスト。
    /// read_file は exec_strict (cat) の stdout をそのまま返す。
    /// バッチ読み込みも同じ内容を返すべき。
    #[test]
    fn test_batch_matches_single_read_behavior() {
        let paths = vec!["test.rs".to_string()];
        // exec が返す stdout: "hello\nworld\n"
        // → read_file の結果: "hello\nworld\n"
        //
        // バッチの場合の出力（echo DELIM + cat + echo '' + echo DELIM）:
        // cat 出力 "hello\nworld\n" + echo '' "\n" → "hello\nworld\n\n"
        let output = "___BATCH_DELIM___0\nhello\nworld\n\n___BATCH_DELIM___1";

        let result = parse_batch_output(output, &paths);
        let single_read_result = "hello\nworld\n";
        assert_eq!(result.get("test.rs").unwrap(), single_read_result);
    }

    // ── chunk_paths テスト ──

    #[test]
    fn test_chunk_paths_empty() {
        let result = chunk_paths(&[], 65536);
        assert!(result.is_empty());
    }

    #[test]
    fn test_chunk_paths_single_path() {
        let paths = vec!["app/main.rs".to_string()];
        let result = chunk_paths(&paths, 65536);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], paths);
    }

    #[test]
    fn test_chunk_paths_fits_in_one_chunk() {
        let paths = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
        ];
        let result = chunk_paths(&paths, 65536);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
    }

    #[test]
    fn test_chunk_paths_exceeds_limit_splits() {
        // 非常に小さい上限でチャンク分割を強制する
        let paths = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
        ];
        // 1パスあたりのコマンド長を計算し、2パスで超える上限を設定
        let one_path_len = estimate_command_len_for_path("file1.txt", 0);
        let end_marker_len = estimate_end_marker_len(2);
        // 2パス + 終端マーカーがギリギリ超える上限
        let limit = one_path_len * 2 + end_marker_len - 1;
        let result = chunk_paths(&paths, limit);
        assert!(result.len() >= 2);
        // 全パスが含まれている
        let total: usize = result.iter().map(|c| c.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_chunk_paths_extremely_long_path() {
        // 1パスだけで上限を超える場合、そのパスだけで1チャンク
        let long_path = "a".repeat(100_000);
        let paths = vec![long_path.clone(), "short.txt".to_string()];
        let result = chunk_paths(&paths, 1000);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![long_path]);
        assert_eq!(result[1], vec!["short.txt".to_string()]);
    }

    #[test]
    fn test_chunk_paths_multibyte_paths() {
        // マルチバイトパス名でバイト長が正しく計算される
        let paths = vec![
            "日本語/ファイル.txt".to_string(),
            "中文/文件.txt".to_string(),
        ];
        let result = chunk_paths(&paths, 65536);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);

        // 小さい上限でマルチバイトのバイト長が正しく計算されて分割される
        let one_path_len = estimate_command_len_for_path(&paths[0], 0);
        let end_marker_len = estimate_end_marker_len(1);
        // 1パスだけ入る上限
        let limit = one_path_len + end_marker_len + 1;
        let result = chunk_paths(&paths, limit);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_chunk_paths_empty_string_path() {
        // 空文字列パスも正常に処理される
        let paths = vec!["".to_string(), "file.txt".to_string()];
        let result = chunk_paths(&paths, 65536);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
    }

    #[test]
    fn test_chunk_paths_exactly_at_limit() {
        // ちょうど上限のとき1チャンクに収まる
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let len0 = estimate_command_len_for_path("a.txt", 0);
        let len1 = estimate_command_len_for_path("b.txt", 1);
        let end_marker = estimate_end_marker_len(2);
        let limit = len0 + len1 + end_marker;
        let result = chunk_paths(&paths, limit);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
    }

    // ── parse_batch_output_bytes テスト ──

    #[test]
    fn test_parse_bytes_basic() {
        let paths = vec!["file.txt".to_string()];
        let output = b"___BATCH_DELIM___0\nhello world\n\n___BATCH_DELIM___1";
        let result = parse_batch_output_bytes(output, &paths);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("file.txt").unwrap(), b"hello world\n");
    }

    #[test]
    fn test_parse_bytes_binary_content() {
        let paths = vec!["binary.bin".to_string()];
        // バイナリコンテンツ（改行以外のバイト列）
        let mut output = Vec::new();
        output.extend_from_slice(b"___BATCH_DELIM___0\n");
        output.extend_from_slice(&[0x00, 0x01, 0xFF, 0xFE, 0x0A]); // 0x0A = \n
        output.extend_from_slice(b"\n___BATCH_DELIM___1");

        let result = parse_batch_output_bytes(&output, &paths);
        assert_eq!(result.len(), 1);
        // echo '' の \n が strip されて、元のバイナリ内容が復元される
        assert_eq!(
            result.get("binary.bin").unwrap(),
            &[0x00, 0x01, 0xFF, 0xFE, 0x0A]
        );
    }

    #[test]
    fn test_parse_bytes_multiple_files() {
        let paths = vec!["a.txt".to_string(), "b.txt".to_string()];
        let output =
            b"___BATCH_DELIM___0\ncontent_a\n\n___BATCH_DELIM___1\ncontent_b\n\n___BATCH_DELIM___2";
        let result = parse_batch_output_bytes(output, &paths);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("a.txt").unwrap(), b"content_a\n");
        assert_eq!(result.get("b.txt").unwrap(), b"content_b\n");
    }

    #[test]
    fn test_parse_bytes_delimiter_mismatch() {
        let paths = vec!["file.txt".to_string()];
        let output = b"___BATCH_DELIM___0\ncontent";
        let result = parse_batch_output_bytes(output, &paths);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_bytes_empty() {
        let paths: Vec<String> = vec![];
        let result = parse_batch_output_bytes(b"", &paths);
        assert!(result.is_empty());
    }

    // ── chunk_paths インデックス整合性テスト ──

    /// チャンク分割後の各チャンクで `build_batch_cat_command()` が
    /// 正しいコマンドを生成できることを検証する。
    /// `chunk_paths()` がチャンク内 0-origin インデックスで推定しているため、
    /// 生成されたコマンドがコマンド長上限を超えないことを確認する。
    #[test]
    fn test_chunk_paths_command_len_within_limit() {
        // 多数のパスを用意し、小さい上限でチャンク分割を強制
        let paths: Vec<String> = (0..50)
            .map(|i| format!("app/controllers/file_{}.php", i))
            .collect();
        let max_len = 500;
        let chunks = chunk_paths(&paths, max_len);

        assert!(chunks.len() > 1, "テストには複数チャンクが必要");

        // 各チャンクのコマンドが上限以内であること
        for chunk in &chunks {
            if let Some(cmd) = build_batch_cat_command(chunk) {
                // 1パスだけのチャンクは上限超えが許容される（パニック回避のため）
                if chunk.len() > 1 {
                    assert!(
                        cmd.len() <= max_len,
                        "chunk command len {} exceeds limit {} (paths: {})",
                        cmd.len(),
                        max_len,
                        chunk.len()
                    );
                }
            }
        }

        // 全パスが含まれている
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 50);
    }

    /// チャンク分割後に各チャンクの結果をマージすると
    /// 全パスの結果が得られることを検証する（テキスト版バッチ読み込みのシミュレーション）。
    #[test]
    fn test_chunk_paths_roundtrip_text_merge() {
        let paths: Vec<String> = (0..10).map(|i| format!("file_{}.txt", i)).collect();
        // 小さい上限で分割
        let chunks = chunk_paths(&paths, 300);

        let mut merged = HashMap::new();
        for chunk in &chunks {
            // 各チャンクの simulated 出力を生成
            let mut output = String::new();
            for (i, path) in chunk.iter().enumerate() {
                output.push_str(&format!("___BATCH_DELIM___{}\n", i));
                output.push_str(&format!("content of {}\n", path));
                // echo '' が追加する改行
                output.push('\n');
            }
            output.push_str(&format!("___BATCH_DELIM___{}", chunk.len()));

            let chunk_result = parse_batch_output(&output, chunk);
            merged.extend(chunk_result);
        }

        // 全パスの結果が含まれている
        assert_eq!(merged.len(), 10);
        for i in 0..10 {
            let key = format!("file_{}.txt", i);
            assert_eq!(merged.get(&key).unwrap(), &format!("content of {}\n", key));
        }
    }

    /// チャンク分割後に各チャンクの結果をマージすると
    /// 全パスの結果が得られることを検証する（バイト列版バッチ読み込みのシミュレーション）。
    #[test]
    fn test_chunk_paths_roundtrip_bytes_merge() {
        let paths: Vec<String> = (0..10).map(|i| format!("file_{}.bin", i)).collect();
        let chunks = chunk_paths(&paths, 300);

        let mut merged: HashMap<String, Vec<u8>> = HashMap::new();
        for chunk in &chunks {
            let mut output = Vec::new();
            for (i, path) in chunk.iter().enumerate() {
                output.extend_from_slice(format!("___BATCH_DELIM___{}\n", i).as_bytes());
                output.extend_from_slice(format!("bytes of {}\n", path).as_bytes());
                // echo '' が追加する改行
                output.push(b'\n');
            }
            output.extend_from_slice(format!("___BATCH_DELIM___{}", chunk.len()).as_bytes());

            let chunk_result = parse_batch_output_bytes(&output, chunk);
            merged.extend(chunk_result);
        }

        assert_eq!(merged.len(), 10);
        for i in 0..10 {
            let key = format!("file_{}.bin", i);
            assert_eq!(
                merged.get(&key).unwrap(),
                format!("bytes of {}\n", key).as_bytes()
            );
        }
    }
}
