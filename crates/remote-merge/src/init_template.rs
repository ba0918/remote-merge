//! init が書く設定のひな形（TOML）

use super::InitInput;

/// TOML 設定ファイルの内容を生成する。値は TOML の文字列として書くので、どの値でも読める
pub fn generate_toml(input: &InitInput) -> String {
    let mut toml = String::new();

    // servers セクション
    toml.push_str(&format!("[servers.{}]\n", table_key(&input.server_name)));
    toml.push_str(&format!("host     = {}\n", quoted(&input.host)));
    toml.push_str("port     = 22\n");
    toml.push_str(&format!("user     = {}\n", quoted(&input.user)));
    toml.push_str(&format!("auth     = {}\n", quoted(&input.auth)));
    if let Some(ref key) = input.key_path
        && !key.is_empty()
    {
        toml.push_str(&format!("key      = {}\n", quoted(key)));
    }
    toml.push_str(&format!("root_dir = {}\n", quoted(&input.remote_root_dir)));

    // local セクション
    toml.push_str(&format!(
        "\n[local]\nroot_dir = {}\n",
        quoted(&input.local_root_dir)
    ));

    // filter セクション
    if !input.exclude.is_empty() {
        toml.push_str("\n[filter]\nexclude = [");
        let patterns: Vec<String> = input.exclude.iter().map(|e| quoted(e)).collect();
        toml.push_str(&patterns.join(", "));
        toml.push_str("]\n");
    }

    toml
}

/// TOML の表の名前。英数字・"_"・"-" だけなら裸のまま、ほかは引用符で囲む
fn table_key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { name.to_string() } else { quoted(name) }
}

/// TOML の基本の文字列（必要な文字はエスケープする）
fn quoted(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}
