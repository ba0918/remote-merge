//! サーバごとに読む環境変数の名前

/// `prefix` にサーバ名を続けた環境変数の名前。サーバ名に `-` や `.` があってもシェルで設定できる名前に
/// なるよう、サーバ名を大文字にし、`-` と `.` を `_` にする
pub(crate) fn server_env_name(prefix: &str, server_name: &str) -> String {
    format!(
        "{prefix}{}",
        server_name.replace(['-', '.'], "_").to_uppercase()
    )
}
