//! パスの要素の事実を尋ねる POSIX sh のスクリプト（docs/remote-tools.md）。
//!
//! 問いは "/" に近い順に並べたパスの並びで、スクリプトは並びの前から順に、symlink・ディレクトリ・
//! 通常ファイル・そのほか・ないかを、リンクを辿らないシェルのテストで確かめる。symlink・
//! 検索できないディレクトリ・ディレクトリでないもの・ないものに出会ったら、その並びの残りは
//! 尋ねない。ないか入れないかは、テストが偽のときだけ `ls -d` の文言の終わりで見分ける（ls が
//! 要るときにないなら、道具がないと答える）。
//! パスごとの処理ではシェルの関数を呼ばず、コマンド置換は readlink と ls -d のときだけ使う。

use std::ops::Range;

/// 出力の始まりの目印。ログインシェルが先に出したものと見分ける
pub const MARKER: &str = "@RMFACTS";
/// スクリプトの書き出し。製品のパスの確認のコマンドかを見分けるのに使える
pub const PREFIX: &str = "LC_ALL=C; export LC_ALL; printf '@RMFACTS\\000'\n";
/// 問いの区切り。問いのパスはどれも "/" で始まるので、パスと取り違えない
const QUESTION: &str = "@Q";

const BODY: &str = r#"ls=0; command -v ls >/dev/null 2>&1 && ls=1
rl=0; command -v readlink >/dev/null 2>&1 && rl=1
s=0
for p do
if [ "$p" = @Q ]; then s=0; printf '@Q\000'; continue; fi
[ $s = 1 ] && continue
if [ -L "$p" ]; then s=1
if [ $rl = 0 ]; then printf '%s\000R\000' "$p"
else t=$(readlink "$p" 2>/dev/null && echo .)
case $t in *.) t=${t%.}; t=${t%?}; printf '%s\000L\000%s\000' "$p" "$t";; *) printf '%s\000X\000' "$p";; esac
fi
elif [ -d "$p" ]; then
if [ -x "$p" ]; then printf '%s\000D\000' "$p"; else s=1; printf '%s\000N\000' "$p"; fi
elif [ -f "$p" ]; then s=1; printf '%s\000F\000' "$p"
elif [ -e "$p" ]; then s=1; printf '%s\000O\000' "$p"
else s=1; [ $ls = 0 ] && { printf '@NOLS\000'; exit 0; }
m=$(ls -d "$p" 2>&1)
case $m in *"No such file or directory") printf '%s\000M\000' "$p";; *) printf '%s\000X\000' "$p";; esac
fi
done
printf '@END\000'
"#;

/// シェルの一つの引数にする（一重引用符で囲む）
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// 一つの問いの引数（区切りと、並びのパス）
fn question_words(chain: &[String]) -> String {
    let mut words = QUESTION.to_string();
    for path in chain {
        words.push(' ');
        words.push_str(&quoted(path));
    }
    words
}

/// `chains` の問いに答えるスクリプト
pub fn build(chains: &[Vec<String>]) -> String {
    let words: Vec<String> = chains.iter().map(|chain| question_words(chain)).collect();
    format!("{PREFIX}set -- {}\n{BODY}", words.join(" "))
}

/// 問いを、ログインシェルに渡すコマンド行が `limit` バイトに収まるまとまりに分ける
pub fn split(chains: &[Vec<String>], limit: usize) -> Vec<Range<usize>> {
    let base = crate::scan_script::wrapped_len(&build(&[]));
    let words = chains
        .iter()
        .map(|chain| format!(" {}", question_words(chain)));
    crate::scan_script::split_by_length(base, words.collect::<Vec<_>>().into_iter(), limit)
}

/// 作業ディレクトリの実パスを尋ねるスクリプト。求められなければ目印だけを出す
pub fn working_directory() -> String {
    format!(
        "{PREFIX}if d=$(pwd -P 2>/dev/null && echo .); then d=${{d%.}}; d=${{d%?}}; \
         case $d in /*) printf '%s\\000' \"$d\";; esac; fi; printf '@END\\000'\n"
    )
}
