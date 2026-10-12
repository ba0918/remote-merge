//! 中身とハッシュをまとめて尋ねる POSIX sh のスクリプト（docs/remote-tools.md）。
//!
//! パスごとに "@P" で始まる答えを一つ出す。失敗は道具が標準エラーに出した文言をそのまま返し、
//! 種類は手元が文言の終わりで決める。大きさは `stat -L -c %s`、受け付けなければ `ls -ldnL` で
//! 尋ね、上限を超えるファイルの中身は送らない。ハッシュは sha256sum、なければ shasum -a 256 で
//! 取り、どちらもないか、あっても見つからないコマンド（終了コード 127）で終わるなら、通常ファイルに
//! ついてはハッシュを取れないと答える（手元が中身を読む）。
//! パスごとの処理ではシェルの関数を呼ばない。

/// 読み込みの出力の始まりの目印
pub const READ_MARKER: &str = "@RMREAD";
/// ハッシュの出力の始まりの目印
pub const HASH_MARKER: &str = "@RMHASH";
/// 読み込みのスクリプトの書き出し
pub const READ_PREFIX: &str = "LC_ALL=C; export LC_ALL; printf '@RMREAD\\000'\n";
/// ハッシュのスクリプトの書き出し
pub const HASH_PREFIX: &str = "LC_ALL=C; export LC_ALL; printf '@RMHASH\\000'\n";

const READ_BODY: &str = r#"if stat -L -c %s / >/dev/null 2>&1; then z=s
elif ls -ldnL / >/dev/null 2>&1 && command -v awk >/dev/null 2>&1; then z=l
else z=n; fi
for p do
printf '@P\000'
if [ -d "$p" ]; then printf 'E\000Is a directory\000'; continue; fi
[ $z = n ] && { printf 'T\000'; continue; }
if [ $z = s ]; then n=$(stat -L -c %s "$p" 2>&1) || { printf 'E\000%s\000' "$n"; continue; }
else n=$(ls -ldnL "$p" 2>&1) || { printf 'E\000%s\000' "$n"; continue; }
n=$(printf '%s\n' "$n" | awk 'NR==1{print $5}'); fi
if [ "$L" -ge 0 ] && [ "$n" -gt "$L" ]; then printf 'B\000%s\000' "$n"; continue; fi
if [ ! -r "$p" ]; then e=$(cat "$p" 2>&1 >/dev/null); printf 'E\000%s\000' "$e"; continue; fi
printf 'C\000%s\000' "$n"; cat "$p" 2>/dev/null; printf '\000%s\000' "$?"
done
printf '@END\000'
"#;

const HASH_BODY: &str = r#"if command -v sha256sum >/dev/null 2>&1; then h=sha256sum
elif command -v shasum >/dev/null 2>&1; then h='shasum -a 256'
else h=; fi
rl=0; command -v readlink >/dev/null 2>&1 && rl=1
for p do
printf '@P\000'
if [ -L "$p" ]; then
if [ $rl = 1 ] && t=$(readlink "$p" 2>/dev/null && echo .); then t=${t%.}; t=${t%?}; printf 'L\000%s\000' "$t"
else printf 'E\000readlink is not available on the remote\000'; fi
continue; fi
if [ -d "$p" ]; then printf 'E\000Is a directory\000'; continue; fi
if [ ! -r "$p" ]; then e=$(cat "$p" 2>&1 >/dev/null); printf 'E\000%s\000' "$e"; continue; fi
[ -z "$h" ] && { printf 'N\000'; continue; }
o=$($h < "$p" 2>&1); c=$?
if [ $c = 0 ]; then printf 'H\000%s\000' "$o"
elif [ $c = 127 ]; then printf 'N\000'
else printf 'E\000%s\000' "$o"; fi
done
printf '@END\000'
"#;

/// シェルの一つの引数にする（一重引用符で囲む）
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// "-" で始まる相対パスを道具のオプションに読ませないよう、"/" で始まらないパスは "./" を付ける
fn argument(path: &str) -> String {
    if path.starts_with('/') {
        quoted(path)
    } else {
        quoted(&format!("./{path}"))
    }
}

fn arguments(paths: &[String]) -> String {
    paths
        .iter()
        .map(|path| format!(" {}", argument(path)))
        .collect()
}

/// `paths` を読むスクリプト。`limit` を超えるファイルは読まない
pub fn read(paths: &[String], limit: Option<u64>) -> String {
    let limit = limit.map_or("-1".to_string(), |limit| limit.to_string());
    format!(
        "{READ_PREFIX}L={limit}\nset --{}\n{READ_BODY}",
        arguments(paths)
    )
}

/// `paths` のハッシュを取るスクリプト
pub fn hash(paths: &[String]) -> String {
    format!("{HASH_PREFIX}set --{}\n{HASH_BODY}", arguments(paths))
}

/// パスをコマンド行が `limit` バイトに収まるまとまりに分ける。`script` はパスのない台
pub fn split(script: &str, paths: &[String], limit: usize) -> Vec<std::ops::Range<usize>> {
    let base = crate::scan_script::wrapped_len(script);
    let words: Vec<String> = paths
        .iter()
        .map(|path| format!(" {}", argument(path)))
        .collect();
    crate::scan_script::split_by_length(base, words.into_iter(), limit)
}
