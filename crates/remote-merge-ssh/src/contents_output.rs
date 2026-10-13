//! 中身とハッシュのスクリプトの出力を読む。答えは尋ねたパスと一つずつ対応しなければならず、
//! 欠け・余り・形の壊れた出力は読めない答えにする

use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};

/// 出力を前から読む
struct Cursor<'a> {
    rest: &'a [u8],
}

impl<'a> Cursor<'a> {
    /// 目印を探し、その後ろから読む。ログインシェルが先に出したものは飛ばす
    fn after_marker(stdout: &'a [u8], marker: &str) -> Result<Self, String> {
        let mark = [marker.as_bytes(), b"\0"].concat();
        let start = stdout
            .windows(mark.len())
            .position(|window| window == mark.as_slice())
            .ok_or("the answer has no marker")?;
        Ok(Self {
            rest: &stdout[start + mark.len()..],
        })
    }

    /// NUL までの欄
    fn field(&mut self) -> Result<&'a [u8], String> {
        let end = self
            .rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or("the answer ended early")?;
        let field = &self.rest[..end];
        self.rest = &self.rest[end + 1..];
        Ok(field)
    }

    /// `end` の並びの前まで。`end` は読み飛ばす
    fn until(&mut self, end: &[u8]) -> Result<&'a [u8], String> {
        let at = self
            .rest
            .windows(end.len())
            .position(|window| window == end)
            .ok_or("the content has no end")?;
        let taken = &self.rest[..at];
        self.rest = &self.rest[at + end.len()..];
        Ok(taken)
    }

    fn number(&mut self) -> Result<u64, String> {
        let field = self.field()?;
        std::str::from_utf8(field)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .ok_or_else(|| format!("not a number: {:?}", String::from_utf8_lossy(field)))
    }

    fn expect(&mut self, tag: &[u8]) -> Result<(), String> {
        match self.field()? {
            field if field == tag => Ok(()),
            other => Err(format!("unexpected {:?}", String::from_utf8_lossy(other))),
        }
    }

    fn end(mut self) -> Result<(), String> {
        self.expect(b"@END")?;
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err("the answer has more than was asked".into())
        }
    }
}

/// 読み込みの答え。パスごとに中身か失敗の種類。中身は `boundary` の区切りまで
pub fn parse_read(
    stdout: &[u8],
    count: usize,
    limit: Option<u64>,
    boundary: &str,
) -> Result<Vec<Result<Vec<u8>, OpFailure>>, String> {
    let end = [
        b"\0",
        super::contents_script::content_end(boundary).as_bytes(),
        b"\0",
    ]
    .concat();
    let mut cursor = Cursor::after_marker(stdout, super::contents_script::READ_MARKER)?;
    let mut answers = Vec::with_capacity(count);
    for _ in 0..count {
        cursor.expect(b"@P")?;
        let answer = match cursor.field()? {
            b"E" => Err(from_tool_message(cursor.field()?)),
            b"T" => Err(OpFailure::ToolMissing(Tool::Stat)),
            b"B" => {
                let size = cursor.number()?;
                let limit = limit.ok_or("a size limit was answered without one")?;
                Err(OpFailure::TooLarge { size, limit })
            }
            b"C" => {
                let content = cursor.until(&end)?.to_vec();
                let status = cursor.field()?;
                let message = cursor.field()?;
                match status {
                    b"0" => Ok(content),
                    _ => Err(from_tool_message(message)),
                }
            }
            other => {
                return Err(format!(
                    "unknown answer {:?}",
                    String::from_utf8_lossy(other)
                ))
            }
        };
        answers.push(answer);
    }
    cursor.end()?;
    Ok(answers)
}

/// ハッシュの答え。パスごとに比べる値か失敗の種類。値が None のパスは、ハッシュを取る道具が
/// ないので中身を読んで取る
pub fn parse_hash(
    stdout: &[u8],
    count: usize,
) -> Result<Vec<Result<Option<String>, OpFailure>>, String> {
    let mut cursor = Cursor::after_marker(stdout, super::contents_script::HASH_MARKER)?;
    let mut answers = Vec::with_capacity(count);
    for _ in 0..count {
        cursor.expect(b"@P")?;
        let answer = match cursor.field()? {
            b"E" => Err(from_tool_message(cursor.field()?)),
            b"L" => Ok(Some(String::from_utf8_lossy(cursor.field()?).into_owned())),
            b"R" => Err(OpFailure::ToolMissing(Tool::Readlink)),
            b"H" => Ok(Some(digest(cursor.field()?)?)),
            b"N" => Ok(None),
            other => {
                return Err(format!(
                    "unknown answer {:?}",
                    String::from_utf8_lossy(other)
                ))
            }
        };
        answers.push(answer);
    }
    cursor.end()?;
    Ok(answers)
}

/// sha256sum か shasum の出力の初めの 64 桁の小文字の 16 進
fn digest(output: &[u8]) -> Result<String, String> {
    let hex = output.get(..64).ok_or("the hash is too short")?;
    if hex.iter().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(String::from_utf8_lossy(hex).to_ascii_lowercase())
    } else {
        Err(format!("not a hash: {:?}", String::from_utf8_lossy(output)))
    }
}
