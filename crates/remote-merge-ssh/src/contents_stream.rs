//! ハッシュを取る道具がないサーバで、通常ファイルの中身をまとめて尋ね、受けながら手元で SHA-256 を
//! 取る（docs/ir/cli/status.md の REQ-cli-095、docs/ir/ssh/reconnect.md の REQ-ssh-022）。
//!
//! 答えは読み込みと同じく、パスごとに "@P" の後に中身、手元が決めた区切り、cat の終わりの状態と
//! 知らせを出す。受けた順に区切りを探しながらハッシュに通し、中身をまとめてメモリに持たない。

use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};
use sha2::{Digest, Sha256};

use crate::contents_script::{argument, content_end, BOUNDARY_FROM_INPUT};

/// スクリプトの書き出し。製品の読み込みのコマンドかを見分けるのに使える
pub const RAW_PREFIX: &str = "LC_ALL=C; export LC_ALL; printf '@RMRAW\\000'\n";
const RAW_MARKER: &[u8] = b"@RMRAW\0";

const RAW_BODY: &str = r#"ct=0; command -v cat >/dev/null 2>&1 && ct=1
for p do
if [ $ct = 0 ]; then printf '@P\000\000%s\000K\000\000' "$B"; continue; fi
if [ -e "$p" ] && [ ! -f "$p" ]; then printf '@P\000\000%s\0001\000not a regular file\000' "$B"; continue; fi
printf '@P\000'; { e=$(cat "$p" 2>&1 >&3 3>&-); c=$?; } 3>&1
printf '\000%s\000%s\000%s\000' "$B" "$c" "$e"
done
printf '@END\000'
"#;

/// `paths` の中身をそのまま出すスクリプト。区切りは標準入力で受ける（[`boundary_input`]）
///
/// [`boundary_input`]: crate::contents_script::boundary_input
pub fn raw(paths: &[String]) -> String {
    let words: String = paths
        .iter()
        .map(|path| format!(" {}", argument(path)))
        .collect();
    format!("{RAW_PREFIX}{BOUNDARY_FROM_INPUT}set --{words}\n{RAW_BODY}")
}

enum State {
    /// 目印を探している
    Marker,
    /// "@P" か "@END" を待っている
    Header,
    /// 中身をハッシュに通している
    Content(Sha256),
    /// 中身の後の、終わりの状態と知らせ
    Trailer(Sha256),
    Done,
}

/// 受けた順に渡して、パスごとのハッシュか失敗の種類にする
pub struct RawDigests {
    end: Vec<u8>,
    state: State,
    /// まだ決められない受けた末尾（区切りや見出しが分かれて届いたときのため）
    held: Vec<u8>,
    results: Vec<Result<String, OpFailure>>,
    broken: Option<String>,
}

impl RawDigests {
    pub fn new(boundary: &str) -> Self {
        Self {
            end: [b"\0", content_end(boundary).as_bytes(), b"\0"].concat(),
            state: State::Marker,
            held: Vec::new(),
            results: Vec::new(),
            broken: None,
        }
    }

    pub fn feed(&mut self, piece: &[u8]) {
        if self.broken.is_some() {
            return;
        }
        self.held.extend_from_slice(piece);
        while self.step() {}
    }

    /// 持っている分で一つ先へ進めたら true
    fn step(&mut self) -> bool {
        let state = std::mem::replace(&mut self.state, State::Done);
        let (next, progressed) = match state {
            State::Marker => match find(&self.held, RAW_MARKER) {
                Some(at) => {
                    self.held.drain(..at + RAW_MARKER.len());
                    (State::Header, true)
                }
                None => {
                    let keep = self.held.len().saturating_sub(RAW_MARKER.len() - 1);
                    self.held.drain(..keep);
                    (State::Marker, false)
                }
            },
            State::Header => self.header(),
            State::Content(mut hasher) => match find(&self.held, &self.end) {
                Some(at) => {
                    hasher.update(&self.held[..at]);
                    self.held.drain(..at + self.end.len());
                    (State::Trailer(hasher), true)
                }
                None => {
                    let ready = self.held.len().saturating_sub(self.end.len() - 1);
                    hasher.update(&self.held[..ready]);
                    self.held.drain(..ready);
                    (State::Content(hasher), false)
                }
            },
            State::Trailer(hasher) => self.trailer(hasher),
            State::Done => (State::Done, false),
        };
        self.state = next;
        progressed
    }

    fn header(&mut self) -> (State, bool) {
        if self.held.starts_with(b"@P\0") {
            self.held.drain(..3);
            return (State::Content(Sha256::new()), true);
        }
        if self.held.starts_with(b"@END\0") {
            self.held.drain(..5);
            return (State::Done, true);
        }
        if self.held.len() >= 5 {
            self.broken = Some("unexpected answer".into());
        }
        (State::Header, false)
    }

    fn trailer(&mut self, hasher: Sha256) -> (State, bool) {
        let mut fields = self.held.splitn(3, |byte| *byte == 0);
        let (Some(status), Some(message), Some(_)) = (fields.next(), fields.next(), fields.next())
        else {
            return (State::Trailer(hasher), false);
        };
        let result = match status {
            b"0" => Ok(hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()),
            b"K" => Err(OpFailure::ToolMissing(Tool::Cat)),
            _ => Err(from_tool_message(message)),
        };
        let used = status.len() + message.len() + 2;
        self.results.push(result);
        self.held.drain(..used);
        (State::Header, true)
    }

    /// 答えが終わったところで、パスごとの結果にする。`count` 件と合わない・形が壊れていれば Err
    pub fn finish(self, count: usize) -> Result<Vec<Result<String, OpFailure>>, String> {
        if let Some(broken) = self.broken {
            return Err(broken);
        }
        if !matches!(self.state, State::Done) || !self.held.is_empty() {
            return Err("the answer ended early".into());
        }
        if self.results.len() != count {
            return Err(format!("{} answers for {count} paths", self.results.len()));
        }
        Ok(self.results)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
