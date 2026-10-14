//! テスト用 SSH サーバが受け取ったコマンドから、`sh -c` に渡したスクリプトを取り出す。
//!
//! 製品はリモートのコマンドを単一引用符で囲んだ `sh -c` の引数にして送る。コマンドの文字列で
//! 振る舞いを決めるテスト用サーバは、ログインシェルと同じく引用を外したスクリプトを見る。

/// `sh -c '<引数>'` の形なら引数の中身を、そうでなければ受け取った文字列をそのまま返す。
pub fn script_of(command: &[u8]) -> String {
    let command = String::from_utf8_lossy(command);
    let Some(quoted) = command.strip_prefix("sh -c ") else {
        return command.into_owned();
    };
    let mut script = String::with_capacity(quoted.len());
    let mut in_quotes = false;
    let mut chars = quoted.chars();
    while let Some(ch) = chars.next() {
        match (in_quotes, ch) {
            (_, '\'') => in_quotes = !in_quotes,
            (false, '\\') => script.extend(chars.next()),
            _ => script.push(ch),
        }
    }
    script
}
