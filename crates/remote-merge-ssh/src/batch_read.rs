//! 複数ファイルを1つのSSHコマンドで読み込むバッチ処理。
//!
//! チャネル枯渇防止: N個のファイルを1チャネルで読む。
//! コマンド生成・結果パースは純粋関数としてテスト可能。
//!
//! 出力の形式（ファイル i ごとに一つ、最後に終わりの印）:
//! - 読めたファイル: `F<i> <バイト数>\n` + 中身 + `\nS<i> <cat の終了コード>\n`
//! - 通常ファイルでないか読めないファイル: `E<i>\n`
//! - 終わりの印: `END<件数>\n`
//!
//! 中身はバイト数で切り出すので、中身に区切りに似た行があっても衝突しない。

use std::collections::HashMap;

use super::tree_parser::shell_escape;
use crate::login_shell::{wrapped_script_len, WRAP_OVERHEAD};

/// SSH バッチ読み込みのコマンド長上限（バイト）。
///
/// ARG_MAX 128KB (CentOS 5) の半分。安全マージンを確保する。
pub const SSH_BATCH_MAX_COMMAND_LEN: usize = 65536;

/// Agent バッチ読み込みのパス数上限。
///
/// Agent はシェルコマンドを使わないため ARG_MAX 制約なし。
/// プロトコルのシリアライズ負荷を考慮した値。
pub const AGENT_BATCH_MAX_PATHS: usize = 2000;

/// ファイルごとのコマンドをつなぐ区切り
const COMMAND_SEPARATOR: &str = "; ";

/// ファイル i を読むコマンド。
///
/// バイト数を中身より先に出すため `wc -c` で数えてから `cat` する。その間にファイルが
/// 変わると数が合わず、解析が呼び出し全体を失敗にする（部分的な結果を正しい結果にしない）。
fn file_command(index: usize, path: &str) -> String {
    let q = shell_escape(path);
    format!(
        "if [ -f {q} ] && [ -r {q} ] && n=$(wc -c < {q}); then printf 'F{index} %s\\n' \"$n\"; cat {q} 2>/dev/null; printf '\\nS{index} %s\\n' \"$?\"; else printf 'E{index}\\n'; fi"
    )
}

/// 終わりの印を出すコマンド
fn end_command(path_count: usize) -> String {
    format!("printf 'END{path_count}\\n'")
}

/// 複数ファイルを読むシェルコマンドを組み立てる。空のスライスなら `None` を返す。
pub fn build_batch_cat_command(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| file_command(i, path))
        .collect();
    parts.push(end_command(paths.len()));
    Some(parts.join(COMMAND_SEPARATOR))
}

/// 1パスが、ログインシェル向けに包んで送るコマンドに足す長さ（バイト単位）。区切りを含む。
fn estimate_command_len_for_path(path: &str, index: usize) -> usize {
    wrapped_script_len(&file_command(index, path)) + wrapped_script_len(COMMAND_SEPARATOR)
}

/// 終端マーカーと、ログインシェル向けの包みの前後が足す長さ。
fn estimate_end_marker_len(path_count: usize) -> usize {
    wrapped_script_len(&end_command(path_count)) + WRAP_OVERHEAD
}

/// パスをコマンド長上限に収まるチャンクに分割する。
///
/// 各チャンクの合計コマンド長が `max_command_len` バイト以下になるように分割する。
/// 1パスだけで上限を超える場合は、そのパスだけで1チャンクにする（パニックしない）。
pub fn chunk_paths(paths: &[String], max_command_len: usize) -> Vec<Vec<String>> {
    if paths.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut current_chunk: Vec<String> = Vec::new();
    // 現在のチャンクのコマンド長（終端マーカーを除く）
    let mut current_len: usize = 0;

    for path in paths.iter() {
        // `build_batch_cat_command()` はチャンク内 0-origin のインデックスを使うため、それと一致させる
        let chunk_local_index = current_chunk.len();
        let path_len = estimate_command_len_for_path(path, chunk_local_index);
        let end_marker_len = estimate_end_marker_len(current_chunk.len() + 1);

        if current_chunk.is_empty() {
            // チャンクが空の場合は必ず追加（1パスで上限超えでもパニックしない）
            current_chunk.push(path.clone());
            current_len = path_len;
        } else if current_len + path_len + end_marker_len > max_command_len {
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

/// バッチコマンドの出力を、読めたファイルのパス→バイト列に直す。
///
/// 読めなかったパスは結果に入れない。件数・バイト数・印が形式と合わない出力は Err にする。
pub fn parse_batch_output_bytes(
    output: &[u8],
    paths: &[String],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let mut results = HashMap::new();
    let mut rest = output;
    for (index, path) in paths.iter().enumerate() {
        let (line, after_line) = split_line(rest)?;
        if line == format!("E{index}").as_bytes() {
            rest = after_line;
            continue;
        }
        let size = parse_size_header(line, index)?;
        if after_line.len() < size {
            anyhow::bail!("Malformed batch read output: content of entry {index} is cut off");
        }
        let (content, after_content) = after_line.split_at(size);
        let status_line = after_content
            .strip_prefix(b"\n")
            .ok_or_else(|| malformed(index))?;
        let (status, after_status) = split_line(status_line)?;
        let status = status
            .strip_prefix(format!("S{index} ").as_bytes())
            .ok_or_else(|| malformed(index))?;
        if status == b"0" {
            results.insert(path.clone(), content.to_vec());
        }
        rest = after_status;
    }
    if rest != format!("END{}\n", paths.len()).as_bytes() {
        anyhow::bail!("Malformed batch read output: end marker mismatch");
    }
    Ok(results)
}

fn malformed(index: usize) -> anyhow::Error {
    anyhow::anyhow!("Malformed batch read output: unexpected marker for entry {index}")
}

/// `F<i> <バイト数>` の行からバイト数を取り出す。`wc -c` が前に空白を付ける環境もある
fn parse_size_header(line: &[u8], index: usize) -> anyhow::Result<usize> {
    line.strip_prefix(format!("F{index} ").as_bytes())
        .and_then(|size| std::str::from_utf8(size).ok())
        .and_then(|size| size.trim().parse().ok())
        .ok_or_else(|| malformed(index))
}

/// 最初の改行までの行と、その後ろを返す。改行がなければ Err
fn split_line(bytes: &[u8]) -> anyhow::Result<(&[u8], &[u8])> {
    let end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| anyhow::anyhow!("Malformed batch read output: missing line end"))?;
    Ok((&bytes[..end], &bytes[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login_shell::wrap_for_login_shell;

    /// 生成したコマンドを実際の sh で実行し、その stdout を返す
    fn run_batch(paths: &[String]) -> Vec<u8> {
        let command = build_batch_cat_command(paths).unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &command])
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output.stdout
    }

    fn path_in(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).to_str().unwrap().to_string()
    }

    // ── 生成したコマンドの実行と解析 ──

    #[cfg(unix)]
    #[test]
    fn unreadable_paths_are_left_out_and_an_empty_file_reads_as_empty() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("locked.txt"), "secret\n").unwrap();
        std::fs::set_permissions(
            dir.path().join("locked.txt"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        // root で実行すると権限を落としても読めてしまうため、前提の崩れを知らせる
        let locked_is_unreadable = std::fs::read(dir.path().join("locked.txt")).is_err();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("empty.txt"), "").unwrap();
        std::fs::write(dir.path().join("kept.txt"), "kept\n").unwrap();
        let paths: Vec<String> = ["missing.txt", "locked.txt", "sub", "empty.txt", "kept.txt"]
            .iter()
            .map(|name| path_in(&dir, name))
            .collect();

        let output = run_batch(&paths);
        let result = parse_batch_output_bytes(&output, &paths).unwrap();

        std::fs::set_permissions(
            dir.path().join("locked.txt"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(locked_is_unreadable, "test needs an unreadable file");
        let mut read: Vec<&str> = result.keys().map(String::as_str).collect();
        read.sort();
        assert_eq!(read, [paths[3].as_str(), paths[4].as_str()]);
        assert_eq!(result[&paths[3]], b"");
        assert_eq!(result[&paths[4]], b"kept\n");
    }

    #[test]
    fn file_contents_come_back_byte_for_byte() {
        let dir = tempfile::TempDir::new().unwrap();
        let contents: [(&str, &[u8]); 4] = [
            ("delimiter.txt", b"___BATCH_DELIM___0\nE1\nF1 3\nEND2\n"),
            ("no-newline.txt", b"last line"),
            ("binary.bin", &[0x00, 0xff, 0xfe, b'\n', 0x00, 0xc3]),
            ("trailing-newlines.txt", b"\n\n"),
        ];
        let mut paths = Vec::new();
        for (name, content) in contents {
            std::fs::write(dir.path().join(name), content).unwrap();
            paths.push(path_in(&dir, name));
        }

        let output = run_batch(&paths);
        let result = parse_batch_output_bytes(&output, &paths).unwrap();

        assert_eq!(result.len(), contents.len());
        for (path, (_, content)) in paths.iter().zip(contents) {
            assert_eq!(result[path], content, "{path}");
        }
    }

    #[test]
    fn paths_with_quotes_and_spaces_are_read() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("it's my file.txt"), "quoted\n").unwrap();
        let paths = vec![path_in(&dir, "it's my file.txt")];

        let output = run_batch(&paths);
        let result = parse_batch_output_bytes(&output, &paths).unwrap();

        assert_eq!(result[&paths[0]], b"quoted\n");
    }

    #[test]
    fn test_empty_paths_returns_none() {
        assert!(build_batch_cat_command(&[]).is_none());
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
        let command = build_batch_cat_command(&paths).unwrap();
        assert_eq!(wrap_for_login_shell(&command).len(), limit);
    }

    /// チャンク分割後の各チャンクのコマンドがコマンド長上限を超えないこと
    #[test]
    fn test_chunk_paths_command_len_within_limit() {
        // 多数のパスを用意し、小さい上限でチャンク分割を強制
        let paths: Vec<String> = (0..50)
            .map(|i| format!("app/controllers/file_{}.php", i))
            .collect();
        let max_len = 1500;
        let chunks = chunk_paths(&paths, max_len);

        assert!(chunks.len() > 1, "テストには複数チャンクが必要");

        // 各チャンクのコマンドが上限以内であること
        for chunk in &chunks {
            if let Some(cmd) = build_batch_cat_command(chunk) {
                // 送るのはログインシェル向けに包んだコマンド
                let cmd = wrap_for_login_shell(&cmd);
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
}
