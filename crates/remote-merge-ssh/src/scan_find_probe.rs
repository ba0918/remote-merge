//! 走査に要る find があるかの確かめ（docs/ir/ssh/compatibility.md の TBL-ssh-002・REQ-ssh-026）

use crate::scan_script::MARKER;

/// find が -printf と "-exec … {} +" を受け付けるかを確かめ、受け付けなければ "@NF" を出して
/// 終える（docs/ir/ssh/compatibility.md の TBL-ssh-002）。終了コードで決め、知らせの文言は読まない
pub(crate) const FIND_PROBE: &str = concat!(
    "find / -prune -printf '' -exec sh -c 'exit 0' sh {} + >/dev/null 2>&1",
    " || { printf '@NF\\000'; exit 0; }\n",
);

/// 走査に要る find がないと出したか
pub fn find_unusable(stdout: &[u8]) -> bool {
    let answer = [MARKER.as_bytes(), b"\0@NF\0"].concat();
    stdout
        .windows(answer.len())
        .any(|window| window == answer.as_slice())
}
