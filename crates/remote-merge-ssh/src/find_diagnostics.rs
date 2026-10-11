//! 走査の find が失敗したとき、失敗したのが exclude で除くパスだけかを見分ける
//! （docs/ir/config/filters.md の REQ-config-021・REQ-config-022）。
//!
//! ローカルとエージェントの走査は exclude に当たったディレクトリの下へ降りないので、その下の
//! 循環する symlink や読めないディレクトリでは失敗しない。SSH の find は `find_prune` で刈り込める
//! パターンなら降りないが、刈り込めないパターンでは降りて失敗する。その失敗が、除くパスだけで
//! 起きたと言えるときに限り、走査を成功として扱う。
//!
//! 見分けるのは GNU findutils が C ロケールで出す次の二つの文言だけで、標準エラーの行が一つでも
//! これに当たらないか、どれかのパスが root_dir の外か除かれないなら、今までどおり失敗にする:
//!
//! - `find: 'P': Permission denied`
//! - `find: File system loop detected; 'P' is part of the same file system loop as 'Q'.`
//!
//! パスの引用の中の "\" の逃がしは、GNU find が C ロケールで出す形（`\'`・`\\`・制御文字の
//! `\a`・`\b`・`\f`・`\n`・`\r`・`\t`・`\v`、ASCII 以外のバイトや残りの制御文字の 3 桁の 8 進）を
//! 戻す。ほかの形の逃がしと、戻したバイトが UTF-8 として読めないパスは見分けない。

use crate::filter;

const PREFIX: &str = "find: ";
const PERMISSION_DENIED: &str = ": Permission denied";
const LOOP_HEAD: &str = "File system loop detected; ";
const LOOP_MIDDLE: &str = " is part of the same file system loop as ";

/// 走査の find の終了コード `exit_code` と標準エラー `stderr` から、走査が失敗したか。
/// exclude で除くパスの下で辿れなかっただけなら、ローカルとエージェントと同じく失敗にしない
pub fn scan_failed(exit_code: u32, stderr: &str, base_path: &str, exclude: &[String]) -> bool {
    exit_code != 0 && !(exit_code == 1 && only_excluded_paths_failed(stderr, base_path, exclude))
}

/// 標準エラー `stderr` の行がすべて、`base_path` の下で exclude に当たるパス（かその下のパス）を
/// 辿れなかったという find の診断で、そうした行が一つ以上あるか
pub fn only_excluded_paths_failed(stderr: &str, base_path: &str, exclude: &[String]) -> bool {
    let mut lines = stderr.lines().filter(|line| !line.is_empty()).peekable();
    lines.peek().is_some()
        && lines.all(|line| {
            diagnostic_path(line)
                .as_deref()
                .and_then(|path| root_relative(path, base_path))
                .is_some_and(|rel| is_excluded_or_below(rel, exclude))
        })
}

/// find の診断の行から、辿れなかったパスを取り出す。見分けられない行は None
fn diagnostic_path(line: &str) -> Option<String> {
    let body = line.strip_prefix(PREFIX)?;
    if let Some(rest) = body.strip_prefix(LOOP_HEAD) {
        let (path, rest) = quoted(rest)?;
        let (_, rest) = quoted(rest.strip_prefix(LOOP_MIDDLE)?)?;
        return (rest == ".").then_some(path);
    }
    let (path, rest) = quoted(body)?;
    (rest == PERMISSION_DENIED).then_some(path)
}

/// 先頭の `'...'` の中身と、その後ろを返す。逃がしを戻した中身が UTF-8 として読めなければ None
fn quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('\'')?;
    let bytes = body.as_bytes();
    let mut content = Vec::new();
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\'' => return Some((String::from_utf8(content).ok()?, &body[index + 1..])),
            b'\\' => {
                let (unescaped, used) = unescape(&bytes[index + 1..])?;
                content.push(unescaped);
                index += 1 + used;
            }
            _ => {
                content.push(byte);
                index += 1;
            }
        }
    }
    None
}

/// "\" の後ろの並び `rest` の先頭を 1 バイトに戻し、使った長さとともに返す
fn unescape(rest: &[u8]) -> Option<(u8, usize)> {
    let byte = match *rest.first()? {
        b'\'' => b'\'',
        b'\\' => b'\\',
        b'a' => 0x07,
        b'b' => 0x08,
        b'f' => 0x0c,
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'v' => 0x0b,
        _ => return octal_byte(rest),
    };
    Some((byte, 1))
}

/// 3 桁の 8 進で書いたバイト
fn octal_byte(rest: &[u8]) -> Option<(u8, usize)> {
    let digits = rest.get(..3)?;
    let value = digits.iter().try_fold(0u32, |value, digit| {
        (b'0'..=b'7')
            .contains(digit)
            .then(|| value * 8 + u32::from(digit - b'0'))
    })?;
    Some((u8::try_from(value).ok()?, 3))
}

/// `path` の `base_path` からの相対パス。`base_path` の下でなければ None
fn root_relative<'a>(path: &'a str, base_path: &str) -> Option<&'a str> {
    let base = base_path.trim_end_matches('/');
    let rel = path
        .strip_prefix(base)?
        .strip_prefix('/')?
        .trim_start_matches('/');
    (!rel.is_empty()).then_some(rel)
}

/// `rel` 自体かその祖先のどれかが exclude に当たるか
fn is_excluded_or_below(rel: &str, exclude: &[String]) -> bool {
    rel.match_indices('/')
        .map(|(end, _)| &rel[..end])
        .chain(std::iter::once(rel))
        .any(|path| filter::is_path_excluded(path, exclude))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exclude(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    const LOOP_UNDER_A_B: &str = "find: File system loop detected; '/r/a/b/back' is part of the same file system loop as '/r'.\n";
    const DENIED_UNDER_A_B: &str = "find: '/r/a/b/locked': Permission denied\n";

    #[test]
    fn errors_below_an_excluded_directory_are_accepted() {
        let stderr = format!("{LOOP_UNDER_A_B}{DENIED_UNDER_A_B}");
        for pattern in ["a/?", "**/b", "[b]"] {
            assert!(
                only_excluded_paths_failed(&stderr, "/r", &exclude(&[pattern])),
                "{pattern}"
            );
        }
    }

    #[test]
    fn an_error_outside_the_excluded_directories_is_not_accepted() {
        let stderr = format!("{DENIED_UNDER_A_B}find: '/r/c/locked': Permission denied\n");
        assert!(!only_excluded_paths_failed(
            &stderr,
            "/r",
            &exclude(&["a/?"])
        ));
    }

    #[test]
    fn an_unrecognised_line_is_not_accepted() {
        for extra in [
            "stat: cannot statx '/r/a/b/x': No such file or directory\n",
            "find: '/r/a/b/locked': Input/output error\n",
            "bfs: error: /r/a/b/locked: Permission denied.\n",
            "find: File system loop detected; '/r/a/b/back' is part of the same file system loop as '/r'\n",
        ] {
            let stderr = format!("{DENIED_UNDER_A_B}{extra}");
            assert!(
                !only_excluded_paths_failed(&stderr, "/r", &exclude(&["a/b"])),
                "{extra}"
            );
        }
    }

    #[test]
    fn empty_stderr_is_not_accepted() {
        assert!(!only_excluded_paths_failed("", "/r", &exclude(&["a/b"])));
    }

    #[test]
    fn paths_outside_the_root_are_not_accepted() {
        let stderr = "find: '/rx/a/b': Permission denied\n";
        assert!(!only_excluded_paths_failed(stderr, "/r", &exclude(&["b"])));
        assert!(!only_excluded_paths_failed(
            "find: '/r': Permission denied\n",
            "/r",
            &exclude(&["r"])
        ));
    }

    #[test]
    fn the_root_may_end_with_a_slash_or_be_the_filesystem_root() {
        assert!(only_excluded_paths_failed(
            DENIED_UNDER_A_B,
            "/r/",
            &exclude(&["b"])
        ));
        assert!(only_excluded_paths_failed(
            "find: '/a/b': Permission denied\n",
            "/",
            &exclude(&["a/b"])
        ));
    }

    #[test]
    fn escaped_quotes_and_backslashes_in_paths_are_restored() {
        let stderr = "find: '/r/it\\'s/a\\\\b/x': Permission denied\n";
        assert!(only_excluded_paths_failed(
            stderr,
            "/r",
            &exclude(&["it's/a\\\\b"])
        ));
    }

    #[test]
    fn c_escapes_and_octal_bytes_in_paths_are_decoded() {
        // GNU find は C ロケールで ASCII 以外のバイトを 3 桁の 8 進、制御文字を C の逃がしで出す
        let stderr = "find: '/r/a/b/\\346\\227\\245\\0011/t\\tn\\nx': Permission denied\n";
        assert!(only_excluded_paths_failed(stderr, "/r", &exclude(&["a/?"])));
        assert!(only_excluded_paths_failed(
            "find: '/r/\\346\\227\\245/x': Permission denied\n",
            "/r",
            &exclude(&["日"])
        ));
    }

    #[test]
    fn paths_that_are_not_utf8_or_unknown_escapes_are_not_accepted() {
        for path in ["/r/a/b/\\377", "/r/a/b/\\q", "/r/a/b/\\34"] {
            let stderr = format!("find: '{path}': Permission denied\n");
            assert!(
                !only_excluded_paths_failed(&stderr, "/r", &exclude(&["a/b"])),
                "{path}"
            );
        }
    }
}
