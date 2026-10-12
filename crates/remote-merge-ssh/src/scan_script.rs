//! 走査の問いに答える POSIX sh のスクリプト（docs/remote-tools.md）。
//!
//! find にはリンク先を辿らせない（docs/decision/records/2026-10-12-foundation-2.md の C5）。
//! 問いのパスそのものは辿らずに読み、symlink ならリンク先がディレクトリか・ディレクトリでないか・
//! ないか・辿れないかをシェルの test と ls の終わりの文言で確かめ、ディレクトリなら起点だけを
//! 辿って下を集める。項目は NUL で区切って出す。名前とリンク先に TAB・改行が入りうるため。

use remote_merge_core::scan::exclude::Prunable;

/// find の -printf の書式。種類・デバイス番号・inode 番号・更新時刻・権限・大きさ・リンク文字列・パス
pub const RECORD_FORMAT: &str = r"%y\0%D\0%i\0%T@\0%m\0%s\0%l\0%p\0";

/// 出力の始まりの目印。ログインシェルが先に出したものと見分ける
pub const MARKER: &str = "@RMSCAN";

/// 問い一つ。`path` は root_dir からのパス（空なら root_dir）
pub struct ScriptQuestion<'a> {
    pub path: &'a [u8],
    pub descend: bool,
}

/// バイト列を `printf` の書式の中に置ける形にする。英数字と "._/-" 以外は 3 桁の 8 進にする
fn printf_literal(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"._/-".contains(&byte) {
            text.push(byte as char);
        } else {
            text.push_str(&format!("\\{byte:03o}"));
        }
    }
    text
}

/// root_dir に移る文。root_dir がないか入れなければ、その理由を出して終える。"/" で始まらない
/// root_dir は "./" を前に付け、"-" で始まる値を cd のオプションに読ませない。移った先の絶対
/// パスを R に置き、終わりに root_dir がまだあるかを確かめるのに使う（[`END`]）
pub(crate) fn root_setup(root: &[u8]) -> String {
    let root = if root.starts_with(b"/") {
        root.to_vec()
    } else {
        [b"./", root].concat()
    };
    let mut setup = assign("r", &root);
    setup.push_str(concat!(
        "if [ -d \"$r\" ]; then cd \"$r\" 2>/dev/null || { printf '@F\\000U\\000'; exit 0; }\n",
        "else e=$(ls -dL -- \"$r\" 2>&1 >/dev/null); printf '@F\\000%s\\000' \"$e\"; exit 0; fi\n",
        "R=$PWD\n",
    ));
    setup
}

/// 変数 `name` に `bytes` をそのまま入れる文。末尾の改行も残すため、後ろに "x" を足して外す
pub(crate) fn assign(name: &str, bytes: &[u8]) -> String {
    format!(
        "{name}=$(printf '{}x'); {name}=${{{name}%x}}\n",
        printf_literal(bytes)
    )
}

/// find の刈り込みの式（後ろに " -prune -o" を付けて使う）。刈り込むものがなければ空
fn prune_expression(prunable: &Prunable) -> (String, String) {
    let mut setup = String::new();
    let mut tests = Vec::new();
    for (index, name) in prunable.names.iter().enumerate() {
        setup.push_str(&assign(&format!("n{index}"), name.as_bytes()));
        tests.push(format!("-name \"$n{index}\""));
    }
    for (index, path) in prunable.paths.iter().enumerate() {
        setup.push_str(&assign(
            &format!("p{index}"),
            format!("./{path}").as_bytes(),
        ));
        tests.push(format!("-path \"$p{index}\""));
    }
    if tests.is_empty() {
        return (setup, String::new());
    }
    (setup, format!("\\( {} \\) -prune -o ", tests.join(" -o ")))
}

/// root_dir `root` を基点に `questions` に答えるスクリプト
pub fn build(root: &[u8], questions: &[ScriptQuestion], prunable: &Prunable) -> String {
    let (prune_setup, prune) = prune_expression(prunable);
    let mut script = format!(
        "LC_ALL=C; export LC_ALL\nprintf '{MARKER}\\000'\nprintf '{MARKER}\\n' >&2\nF='{RECORD_FORMAT}'\n"
    );
    script.push_str(&prune_setup);
    script.push_str(&crate::scan_checks::check_script(false));
    script.push_str(&root_setup(root));
    script.push_str(&format!(
        concat!(
            "q() {{\n",
            " printf '@Q\\000'\n",
            " if [ \"$1\" = . ]; then find . -prune -printf \"$F\"\n",
            " elif [ -e \"$1\" ] || [ -h \"$1\" ]; then find \"$1\" -prune -printf \"$F\"\n",
            "  if [ -h \"$1\" ]; then\n",
            "   if [ -d \"$1\" ]; then printf '@LD\\000'; elif [ -e \"$1\" ]; then printf '@LF\\000'\n",
            "   else e=$(ls -dL -- \"$1\" 2>&1 >/dev/null); printf '@LX\\000%s\\000' \"$e\"; fi\n",
            "  fi\n",
            " else e=$(ls -d -- \"$1\" 2>&1 >/dev/null); printf '@E\\000%s\\000@Z\\000' \"$e\"; return; fi\n",
            " if [ \"$2\" = 1 ] && [ -d \"$1\" ]; then printf '@T\\000'\n",
            "  find -H \"$1\" -prune -printf \"$F\"\n",
            "  printf '\\000@QS\\000' >&2; sh -c \"$c\" sh \"$1\"\n",
            "  find -H \"$1\" -mindepth 1 {prune}\\( -type d -printf \"$F\" -exec sh -c \"$c\" sh {{}} + \\) -o -printf \"$F\"\n",
            "  printf '@R\\000%s\\000' \"$?\"\n",
            " fi\n",
            " printf '@Z\\000'\n",
            "}}\n",
        ),
        prune = prune
    ));
    for question in questions {
        script.push_str(&question_lines(question));
    }
    script.push_str(END);
    script
}

/// スクリプトの終わり。root_dir がまだディレクトリかを絶対パスで確かめて "@RT" に 1 か 0 で
/// 出す。sh や find の知らせを読み分けずに、root_dir が走査の途中で消えたかをこれで決める
pub(crate) const END: &str = concat!(
    "if [ -d \"$R\" ]; then t=1; else t=0; fi; printf '@RT\\000%s\\000' $t\n",
    "printf '@END\\000'\n",
);

/// スクリプトの終わりに、root_dir がもうないと出したか
pub fn root_gone(stdout: &[u8]) -> bool {
    stdout.ends_with(b"@RT\x000\x00@END\x00")
}

/// 一つの問いに答える行
fn question_lines(question: &ScriptQuestion) -> String {
    let path = if question.path.is_empty() {
        b".".to_vec()
    } else {
        [b"./", question.path].concat()
    };
    format!(
        "{}q \"$a\" {}\n",
        assign("a", &path),
        u8::from(question.descend)
    )
}

/// 問いを、ログインシェルに渡すコマンド行が `limit` バイトに収まるまとまりに分ける
/// （CentOS 5 の ARG_MAX の半分。docs/remote-tools.md）。一つでも収まらない問いは一つで送る
pub fn split_for_command_line(
    root: &[u8],
    questions: &[ScriptQuestion],
    prunable: &Prunable,
    limit: usize,
) -> Vec<std::ops::Range<usize>> {
    let base = wrapped_len(&build(root, &[], prunable));
    split_by_length(base, questions.iter().map(question_lines), limit)
}

/// 問いのない台の長さ `base` に、問いごとの行 `lines` を足していき、`limit` バイトを超える前で
/// 分ける。一つでも収まらない行は一つで送る
pub(crate) fn split_by_length(
    base: usize,
    lines: impl ExactSizeIterator<Item = String>,
    limit: usize,
) -> Vec<std::ops::Range<usize>> {
    let count = lines.len();
    let mut ranges = Vec::new();
    let (mut begin, mut length) = (0, base);
    for (index, line) in lines.enumerate() {
        let added = crate::login_shell::wrapped_script_len(&line);
        if index > begin && length + added > limit {
            ranges.push(begin..index);
            (begin, length) = (index, base);
        }
        length += added;
    }
    if begin < count || count == 0 {
        ranges.push(begin..count);
    }
    ranges
}

pub(crate) fn wrapped_len(script: &str) -> usize {
    crate::login_shell::WRAP_OVERHEAD + crate::login_shell::wrapped_script_len(script)
}

#[cfg(test)]
#[path = "scan_script_tests.rs"]
mod tests;
