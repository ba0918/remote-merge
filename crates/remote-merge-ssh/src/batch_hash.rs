//! 複数ファイルの SHA-256 を1つの SSH コマンドで取るバッチ処理。
//!
//! ハッシュのコマンドは `sha256sum`、なければ `shasum -a 256` を使う（エージェントの配置の確かめと
//! 同じ順）。どちらも動かないサーバでは、ファイルを読まずに `UNSUPPORTED` だけを出す。
//!
//! 出力の形式（ファイル i ごとに一つ、最後に終わりの印）:
//! - ハッシュを取れたファイル: `H<i> <ハッシュのコマンドの出力>\n`
//! - 通常ファイルでないか読めないファイル: `E<i>\n`
//! - ハッシュのコマンドがない: `UNSUPPORTED\n`（ファイルごとの行は出ない）
//! - 終わりの印: `END<件数>\n`
//!
//! ハッシュは標準入力から読ませて取るので、出力にファイル名は現れない。どのハッシュがどの
//! ファイルのものかはインデックスで決まり、名前の空白や改行に左右されない。

use std::collections::HashMap;

use super::batch_read::chunk_paths_by;
use super::tree_parser::shell_escape;
use crate::login_shell::{wrapped_script_len, WRAP_OVERHEAD};

/// まとめてハッシュを取った結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchHashes {
    /// パス→小文字の16進の SHA-256。通常ファイルでないか読めなかったパスは入らない
    Hashes(HashMap<String, String>),
    /// サーバに sha256sum も shasum もない
    Unsupported,
}

/// 使えるハッシュのコマンドを h に入れる。どちらもなければ印を出す
const DETECT: &str = "if sha256sum </dev/null >/dev/null 2>&1; then h='sha256sum'; elif shasum -a 256 </dev/null >/dev/null 2>&1; then h='shasum -a 256'; else h=''; fi; if [ -z \"$h\" ]; then printf 'UNSUPPORTED\\n'; else ";

/// ファイルごとのコマンドの後に閉じる部分
const DETECT_END: &str = "; fi; ";

const UNSUPPORTED: &[u8] = b"UNSUPPORTED\n";

/// ファイルごとのコマンドをつなぐ区切り
const COMMAND_SEPARATOR: &str = "; ";

/// SHA-256 の16進の桁数
const HASH_HEX_LEN: usize = 64;

/// ファイル i のハッシュを取るコマンド。`$h` は空白で分けて `shasum -a 256` にもなる
fn file_command(index: usize, path: &str) -> String {
    let q = shell_escape(path);
    format!(
        "if [ -f {q} ] && [ -r {q} ] && d=$($h < {q} 2>/dev/null); then printf 'H{index} %s\\n' \"$d\"; else printf 'E{index}\\n'; fi"
    )
}

fn end_command(path_count: usize) -> String {
    format!("printf 'END{path_count}\\n'")
}

/// 複数ファイルのハッシュを取るシェルコマンドを組み立てる。空のスライスなら `None` を返す。
pub fn build_batch_hash_command(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let files: Vec<String> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| file_command(i, path))
        .collect();
    Some(format!(
        "{DETECT}{}{DETECT_END}{}",
        files.join(COMMAND_SEPARATOR),
        end_command(paths.len())
    ))
}

/// パスをコマンド長上限に収まるチャンクに分割する（1パスで上限を超えればそのパスだけで1チャンク）
pub fn chunk_paths(paths: &[String], max_command_len: usize) -> Vec<Vec<String>> {
    chunk_paths_by(
        paths,
        max_command_len,
        |path, index| {
            wrapped_script_len(&file_command(index, path)) + wrapped_script_len(COMMAND_SEPARATOR)
        },
        |count| {
            wrapped_script_len(DETECT)
                + wrapped_script_len(DETECT_END)
                + wrapped_script_len(&end_command(count))
                + WRAP_OVERHEAD
        },
    )
}

/// バッチコマンドの出力を、パス→ハッシュに直す。件数・印・ハッシュの形が合わない出力は Err にする。
pub fn parse_batch_hash_output(output: &[u8], paths: &[String]) -> anyhow::Result<BatchHashes> {
    let end = format!("END{}\n", paths.len());
    if output == [UNSUPPORTED, end.as_bytes()].concat() {
        return Ok(BatchHashes::Unsupported);
    }
    let text = std::str::from_utf8(output)
        .map_err(|_| anyhow::anyhow!("Malformed batch hash output: not UTF-8"))?;
    let mut lines = text.split_inclusive('\n');
    let mut hashes = HashMap::new();
    for (index, path) in paths.iter().enumerate() {
        let line = lines.next().ok_or_else(|| malformed(index))?;
        let line = line.strip_suffix('\n').ok_or_else(|| malformed(index))?;
        if line == format!("E{index}") {
            continue;
        }
        let hash = line
            .strip_prefix(&format!("H{index} "))
            .and_then(|value| value.split_whitespace().next())
            .filter(|hash| {
                hash.len() == HASH_HEX_LEN && hash.bytes().all(|b| b.is_ascii_hexdigit())
            })
            .ok_or_else(|| malformed(index))?;
        hashes.insert(path.clone(), hash.to_ascii_lowercase());
    }
    if lines.collect::<String>() != end {
        anyhow::bail!("Malformed batch hash output: end marker mismatch");
    }
    Ok(BatchHashes::Hashes(hashes))
}

fn malformed(index: usize) -> anyhow::Error {
    anyhow::anyhow!("Malformed batch hash output: unexpected line for entry {index}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login_shell::wrap_for_login_shell;
    use sha2::{Digest, Sha256};

    /// 生成したコマンドを実際の sh で実行し、その stdout を返す。`path_env` があれば PATH にする
    fn run_batch(paths: &[String], path_env: Option<&std::path::Path>) -> Vec<u8> {
        let command = build_batch_hash_command(paths).unwrap();
        let mut shell = std::process::Command::new("/bin/sh");
        shell
            .args(["-c", &command])
            .stdin(std::process::Stdio::null());
        if let Some(path_env) = path_env {
            shell.env("PATH", path_env);
        }
        let output = shell.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        output.stdout
    }

    fn sha256_hex(content: &[u8]) -> String {
        format!("{:x}", Sha256::digest(content))
    }

    #[cfg(unix)]
    #[test]
    fn regular_readable_files_are_hashed_whatever_their_names_and_the_rest_are_left_out() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let names = [
            "it's a file.txt",
            "line\nbreak.txt",
            "locked.txt",
            "sub",
            "missing",
        ];
        std::fs::write(dir.path().join(names[0]), "quoted\n").unwrap();
        std::fs::write(dir.path().join(names[1]), [0u8, 0xff, b'\n']).unwrap();
        std::fs::write(dir.path().join(names[2]), "secret\n").unwrap();
        std::fs::set_permissions(
            dir.path().join(names[2]),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        // root で実行すると権限を落としても読めてしまうため、前提の崩れを知らせる
        let locked_is_unreadable = std::fs::read(dir.path().join(names[2])).is_err();
        std::fs::create_dir(dir.path().join(names[3])).unwrap();
        let paths: Vec<String> = names
            .iter()
            .map(|name| dir.path().join(name).to_str().unwrap().to_string())
            .collect();

        let output = run_batch(&paths, None);
        let result = parse_batch_hash_output(&output, &paths).unwrap();

        std::fs::set_permissions(
            dir.path().join(names[2]),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(locked_is_unreadable, "test needs an unreadable file");
        let expected = HashMap::from([
            (paths[0].clone(), sha256_hex(b"quoted\n")),
            (paths[1].clone(), sha256_hex(&[0u8, 0xff, b'\n'])),
        ]);
        assert_eq!(result, BatchHashes::Hashes(expected));
    }

    #[test]
    fn a_server_without_hash_commands_reports_unsupported_without_reading_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        let paths = vec![dir.path().join("a.txt").to_str().unwrap().to_string()];
        // sh の組み込みのコマンドだけが動く PATH
        let empty = tempfile::TempDir::new().unwrap();

        let output = run_batch(&paths, Some(empty.path()));

        assert_eq!(
            parse_batch_hash_output(&output, &paths).unwrap(),
            BatchHashes::Unsupported
        );
    }

    #[test]
    fn output_that_does_not_match_the_paths_is_an_error() {
        let paths = vec!["a".to_string(), "b".to_string()];
        let hash = "a".repeat(64);
        let cases = [
            format!("H0 {hash}  -\nEND2\n"),
            format!("H0 {hash}  -\nE1\n"),
            format!("H0 {hash}  -\nE1\nEND2\nextra\n"),
            format!("H1 {hash}  -\nE1\nEND2\n"),
            "H0 xyz  -\nE1\nEND2\n".to_string(),
            "ready\n".to_string(),
        ];
        for output in cases {
            assert!(
                parse_batch_hash_output(output.as_bytes(), &paths).is_err(),
                "{output:?}"
            );
        }
        let ok = format!("H0 {}  -\nE1\nEND2\n", hash.to_uppercase());
        assert_eq!(
            parse_batch_hash_output(ok.as_bytes(), &paths).unwrap(),
            BatchHashes::Hashes(HashMap::from([("a".to_string(), hash)]))
        );
    }

    #[test]
    fn test_empty_paths_returns_none() {
        assert!(build_batch_hash_command(&[]).is_none());
    }

    #[test]
    fn every_chunk_command_fits_the_limit_and_every_path_is_kept() {
        let paths: Vec<String> = (0..50)
            .map(|i| format!("app/controllers/file_{i}.php"))
            .collect();
        let max_len = 2500;

        let chunks = chunk_paths(&paths, max_len);

        assert!(chunks.len() > 1, "test needs several chunks");
        for chunk in &chunks {
            let command = wrap_for_login_shell(&build_batch_hash_command(chunk).unwrap());
            assert!(command.len() <= max_len, "{}", command.len());
        }
        assert_eq!(chunks.concat(), paths);
    }
}
