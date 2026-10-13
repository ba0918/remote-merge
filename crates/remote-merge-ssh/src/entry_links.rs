//! 項目そのもの（最後の要素のリンクは辿らない）のリンク先を、親を一覧せずにまとめて尋ねる
//! （docs/ir/paths/operations.md の REQ-paths-013・020）。
//!
//! symlink かどうかはシェルの `[ -L ]` で決め、リンク文字列は readlink で読む。ないか入れないかは
//! `[ -e ]` が偽のときだけ `ls -d` の文言の終わりで見分ける（docs/remote-tools.md）。readlink か
//! ls が要るときにないなら、道具がないと答える。パスごとの処理ではシェルの関数を呼ばない。

use remote_merge_core::machine_ops::EntryLink;
use remote_merge_core::op_failure::{from_tool_message, OpFailure, Tool};

use crate::contents::{in_commands, ContentsFailure, PathAnswer};
use crate::contents_output::Cursor;
use crate::contents_script::argument;

/// 出力の始まりの目印
const LINK_MARKER: &str = "@RMLINK";
/// スクリプトの書き出し。製品の項目のリンク先の問いかを見分けるのに使える
pub const LINK_PREFIX: &str = "LC_ALL=C; export LC_ALL; printf '@RMLINK\\000'\n";

const LINK_BODY: &str = r#"rl=0; command -v readlink >/dev/null 2>&1 && rl=1
ls=0; command -v ls >/dev/null 2>&1 && ls=1
for p do
printf '@P\000'
if [ -L "$p" ]; then
if [ $rl = 0 ]; then printf 'R\000'
elif t=$(readlink "$p" 2>/dev/null && echo .); then t=${t%.}; t=${t%?}; printf 'L\000%s\000' "$t"
else e=$(readlink "$p" 2>&1 >/dev/null); printf 'E\000%s\000' "$e"; fi
elif [ -e "$p" ]; then printf 'N\000'
elif [ $ls = 0 ]; then printf 'S\000'
else e=$(ls -d "$p" 2>&1 >/dev/null); printf 'E\000%s\000' "$e"; fi
done
printf '@END\000'
"#;

/// `paths` の項目のリンク先を尋ねるスクリプト
pub fn script(paths: &[String]) -> String {
    let words: String = paths
        .iter()
        .map(|path| format!(" {}", argument(path)))
        .collect();
    format!("{LINK_PREFIX}set --{words}\n{LINK_BODY}")
}

/// 答えを読む。パスごとにリンクかリンクでないか、失敗の種類
fn parse(stdout: &[u8], count: usize) -> Result<Vec<Result<EntryLink, OpFailure>>, String> {
    let mut cursor = Cursor::after_marker(stdout, LINK_MARKER)?;
    let mut answers = Vec::with_capacity(count);
    for _ in 0..count {
        cursor.expect(b"@P")?;
        answers.push(match cursor.field()? {
            b"L" => Ok(EntryLink::Link(
                String::from_utf8_lossy(cursor.field()?).into_owned(),
            )),
            b"N" => Ok(EntryLink::NotALink),
            b"R" => Err(OpFailure::ToolMissing(Tool::Readlink)),
            b"S" => Err(OpFailure::ToolMissing(Tool::LsDirectory)),
            b"E" => Err(from_tool_message(cursor.field()?)),
            other => {
                return Err(format!(
                    "unknown answer {:?}",
                    String::from_utf8_lossy(other)
                ))
            }
        });
    }
    cursor.end()?;
    Ok(answers)
}

/// `asked`（結果のキーとリモートのパス）の項目のリンク先を、コマンド行の長さで分けて尋ねる
pub fn read_links<E>(
    asked: &[(String, String)],
    max_len: usize,
    mut run: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<PathAnswer<EntryLink>>, ContentsFailure<E>> {
    in_commands(asked, &script(&[]), max_len, |paths| {
        let stdout = run(&script(paths)).map_err(ContentsFailure::Run)?;
        parse(&stdout, paths.len()).map_err(ContentsFailure::Unreadable)
    })
}

#[cfg(all(test, unix))]
#[path = "entry_links_tests.rs"]
mod tests;
