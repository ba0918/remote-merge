//! `remote-merge init` サブコマンド。
//! 対話的に `.remote-merge.toml` を生成する。

use std::io::{self, BufRead, Write};
use std::path::Path;

/// init コマンドの設定入力
#[derive(Debug, Clone)]
pub struct InitInput {
    pub server_name: String,
    pub host: String,
    pub user: String,
    pub auth: String,
    pub key_path: Option<String>,
    pub remote_root_dir: String,
    pub local_root_dir: String,
    pub exclude: Vec<String>,
}

impl Default for InitInput {
    fn default() -> Self {
        Self {
            server_name: "develop".to_string(),
            host: String::new(),
            user: "deploy".to_string(),
            auth: "key".to_string(),
            key_path: None,
            remote_root_dir: String::new(),
            local_root_dir: ".".to_string(),
            exclude: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "dist".to_string(),
            ],
        }
    }
}

/// TOML 設定ファイルの内容を生成する
pub fn generate_toml(input: &InitInput) -> String {
    let mut toml = String::new();

    // servers セクション
    toml.push_str(&format!("[servers.{}]\n", input.server_name));
    toml.push_str(&format!("host     = \"{}\"\n", input.host));
    toml.push_str("port     = 22\n");
    toml.push_str(&format!("user     = \"{}\"\n", input.user));
    toml.push_str(&format!("auth     = \"{}\"\n", input.auth));
    if let Some(ref key) = input.key_path {
        if !key.is_empty() {
            toml.push_str(&format!("key      = \"{}\"\n", key));
        }
    }
    toml.push_str(&format!("root_dir = \"{}\"\n", input.remote_root_dir));

    // local セクション
    toml.push_str(&format!(
        "\n[local]\nroot_dir = \"{}\"\n",
        input.local_root_dir
    ));

    // filter セクション
    if !input.exclude.is_empty() {
        toml.push_str("\n[filter]\nexclude = [");
        let patterns: Vec<String> = input.exclude.iter().map(|e| format!("\"{}\"", e)).collect();
        toml.push_str(&patterns.join(", "));
        toml.push_str("]\n");
    }

    toml
}

/// 対話的に入力を取得して .remote-merge.toml を生成する
pub fn run_init() -> anyhow::Result<()> {
    let output_path = Path::new(".remote-merge.toml");

    // 既存ファイルのチェック
    if output_path.exists() {
        eprint!(".remote-merge.toml は既に存在します。上書きしますか？ [y/N]: ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("キャンセルしました。");
            return Ok(());
        }
    }

    println!("\nremote-merge 設定ファイルを生成します\n");

    let input = prompt_input(&mut io::stdin().lock(), &mut io::stderr())?;
    let toml_content = generate_toml(&input);

    std::fs::write(output_path, &toml_content)?;
    println!("\n.remote-merge.toml を生成しました");

    Ok(())
}

/// 対話的に入力を取得する（テスト用に reader/writer を分離）
pub fn prompt_input<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> anyhow::Result<InitInput> {
    let mut input = InitInput::default();

    input.server_name = prompt_with_default(reader, writer, "サーバ名", &input.server_name)?;
    input.host = prompt_required(reader, writer, "ホスト名")?;
    input.user = prompt_with_default(reader, writer, "ユーザ名", &input.user)?;
    input.auth = prompt_with_default(reader, writer, "認証方式 [key/password]", &input.auth)?;

    if input.auth == "key" {
        let key = prompt_with_default(reader, writer, "SSH鍵パス", "~/.ssh/id_rsa")?;
        if key != "~/.ssh/id_rsa" {
            input.key_path = Some(key);
        }
    }

    input.remote_root_dir = prompt_required(reader, writer, "リモート root_dir")?;
    input.local_root_dir =
        prompt_with_default(reader, writer, "ローカル root_dir", &input.local_root_dir)?;

    let exclude_str = prompt_with_default(
        reader,
        writer,
        "除外パターン (カンマ区切り)",
        "node_modules,.git,dist",
    )?;
    input.exclude = exclude_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(input)
}

/// デフォルト値付きプロンプト
fn prompt_with_default<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
    default: &str,
) -> anyhow::Result<String> {
    write!(writer, "{} (default: {}): ", label, default)?;
    writer.flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// 必須入力プロンプト
fn prompt_required<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    label: &str,
) -> anyhow::Result<String> {
    loop {
        write!(writer, "{}: ", label)?;
        writer.flush()?;

        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim();

        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        writeln!(writer, "  ※ 入力してください")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_toml_minimal() {
        let input = InitInput {
            server_name: "develop".to_string(),
            host: "dev.example.com".to_string(),
            user: "deploy".to_string(),
            auth: "key".to_string(),
            key_path: None,
            remote_root_dir: "/var/www/app".to_string(),
            local_root_dir: ".".to_string(),
            exclude: vec!["node_modules".to_string(), ".git".to_string()],
        };

        let toml = generate_toml(&input);
        assert!(toml.contains("[servers.develop]"));
        assert!(toml.contains("host     = \"dev.example.com\""));
        assert!(toml.contains("user     = \"deploy\""));
        assert!(toml.contains("root_dir = \"/var/www/app\""));
        assert!(toml.contains("[local]"));
        assert!(toml.contains("root_dir = \".\""));
        assert!(toml.contains("[filter]"));
        assert!(toml.contains("\"node_modules\""));
    }

    #[test]
    fn test_generate_toml_with_key_path() {
        let input = InitInput {
            key_path: Some("~/.ssh/custom_key".to_string()),
            ..InitInput::default()
        };

        let toml = generate_toml(&input);
        assert!(toml.contains("key      = \"~/.ssh/custom_key\""));
    }

    #[test]
    fn test_generate_toml_password_auth() {
        let input = InitInput {
            auth: "password".to_string(),
            host: "legacy.example.com".to_string(),
            remote_root_dir: "/var/www".to_string(),
            ..InitInput::default()
        };

        let toml = generate_toml(&input);
        assert!(toml.contains("auth     = \"password\""));
        assert!(!toml.contains("password =")); // パスワード値は含めない（auth値としてのpasswordは除く）
    }

    #[test]
    fn test_generate_toml_is_valid_toml() {
        let input = InitInput {
            host: "dev.example.com".to_string(),
            remote_root_dir: "/var/www/app".to_string(),
            ..InitInput::default()
        };

        let toml_str = generate_toml(&input);
        // パースできることを確認
        let parsed: Result<toml::Value, _> = toml::from_str(&toml_str);
        assert!(parsed.is_ok(), "生成された TOML が不正: {:?}", parsed.err());
    }

    #[test]
    fn test_generate_toml_empty_exclude() {
        let input = InitInput {
            host: "dev.example.com".to_string(),
            remote_root_dir: "/var/www/app".to_string(),
            exclude: vec![],
            ..InitInput::default()
        };

        let toml = generate_toml(&input);
        assert!(!toml.contains("[filter]"));
    }

    #[test]
    fn test_prompt_input_with_defaults() {
        // シミュレート: すべてデフォルト値を使う（空行入力）
        // ただし host と remote_root_dir は必須なので値を入力
        let input_text = "\ndev.example.com\n\n\n\n/var/www/app\n\n\n";
        let mut reader = io::Cursor::new(input_text);
        let mut writer = Vec::new();

        let result = prompt_input(&mut reader, &mut writer).unwrap();

        assert_eq!(result.server_name, "develop");
        assert_eq!(result.host, "dev.example.com");
        assert_eq!(result.user, "deploy");
        assert_eq!(result.auth, "key");
        assert_eq!(result.remote_root_dir, "/var/www/app");
        assert_eq!(result.local_root_dir, ".");
    }

    #[test]
    fn test_prompt_input_custom_values() {
        let input_text =
            "staging\nstaging.example.com\nweb\npassword\n/opt/app\n./src\n*.log,*.tmp\n";
        let mut reader = io::Cursor::new(input_text);
        let mut writer = Vec::new();

        let result = prompt_input(&mut reader, &mut writer).unwrap();

        assert_eq!(result.server_name, "staging");
        assert_eq!(result.host, "staging.example.com");
        assert_eq!(result.user, "web");
        assert_eq!(result.auth, "password");
        assert_eq!(result.remote_root_dir, "/opt/app");
        assert_eq!(result.local_root_dir, "./src");
        assert_eq!(result.exclude, vec!["*.log", "*.tmp"]);
    }
}
