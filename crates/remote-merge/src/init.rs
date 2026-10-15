//! `remote-merge init` サブコマンド。
//! 対話的に `.remote-merge.toml` を生成する。

use std::io::{self, BufRead, Write};
use std::path::Path;

#[path = "init_template.rs"]
mod template;
pub use template::generate_toml;

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

/// 対話的に入力を取得して .remote-merge.toml を生成する
pub fn run_init() -> anyhow::Result<()> {
    let output_path = Path::new(".remote-merge.toml");

    // 既存ファイルのチェック
    if output_path.exists() {
        eprint!(".remote-merge.toml already exists. Overwrite? [y/N]: ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!("\nGenerating remote-merge config file\n");

    let input = prompt_input(&mut io::stdin().lock(), &mut io::stderr())?;
    let toml_content = generate_toml(&input);

    std::fs::write(output_path, &toml_content)?;
    println!("\n.remote-merge.toml created successfully");

    Ok(())
}

/// 対話的に入力を取得する（テスト用に reader/writer を分離）
pub fn prompt_input<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> anyhow::Result<InitInput> {
    let mut input = InitInput::default();

    input.server_name = prompt_with_default(reader, writer, "Server name", &input.server_name)?;
    input.host = prompt_required(reader, writer, "Hostname")?;
    input.user = prompt_with_default(reader, writer, "Username", &input.user)?;
    loop {
        let answer =
            prompt_with_default(reader, writer, "Auth method [key/password]", &input.auth)?;
        // ツール自身が読める値（"key" か "password"）だけを受け付ける。大文字小文字は問わない
        // （"Password" をそのまま書くと、次のコマンドが設定を読めない）
        let normalized = answer.trim().to_ascii_lowercase();
        if normalized == "key" || normalized == "password" {
            input.auth = normalized;
            break;
        }
        writeln!(writer, "Please answer 'key' or 'password'.")?;
    }

    if input.auth == "key" {
        let key = prompt_with_default(reader, writer, "SSH key path", "~/.ssh/id_rsa")?;
        if key != "~/.ssh/id_rsa" {
            input.key_path = Some(key);
        }
    }

    input.remote_root_dir = prompt_required(reader, writer, "Remote root_dir")?;
    input.local_root_dir =
        prompt_with_default(reader, writer, "Local root_dir", &input.local_root_dir)?;

    let exclude_str = prompt_with_default(
        reader,
        writer,
        "Exclude patterns (comma-separated)",
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

    let line = read_answer(reader, label)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// 問いへの答えの一行。入力が終わっていたら、同じ問いを繰り返さずに誤りにする
fn read_answer<R: BufRead>(reader: &mut R, label: &str) -> anyhow::Result<String> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        anyhow::bail!("input ended before {label} was given; no config was written");
    }
    Ok(line)
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

        let line = read_answer(reader, label)?;
        let trimmed = line.trim();

        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        writeln!(writer, "  * Required")?;
    }
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
