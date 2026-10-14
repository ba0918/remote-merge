//! init のひな形の生成と問いの単体テスト
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
    assert!(
        parsed.is_ok(),
        "Generated TOML is invalid: {:?}",
        parsed.err()
    );
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
    let input_text = "staging\nstaging.example.com\nweb\npassword\n/opt/app\n./src\n*.log,*.tmp\n";
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

// 入力が途中で終わったら、同じ問いを繰り返さずに止める（書く設定がそろわないので書かない）
// @kotowari[REQ-cli-012]
#[test]
fn input_that_ends_before_the_required_values_stops_instead_of_asking_again() {
    for given in ["", "develop\n", "develop\nhost\n\n\n"] {
        let mut reader = std::io::Cursor::new(given.as_bytes().to_vec());
        let result = prompt_input(&mut reader, &mut Vec::new());
        assert!(result.is_err(), "{given:?}: {result:?}");
    }
}

// 生成したひな形は、どの値を入れても設定として読める（値は TOML の文字列として書く）
// @kotowari[REQ-cli-012]
#[test]
fn the_generated_template_reads_back_with_any_values() {
    let input = InitInput {
        server_name: "dev.box".to_string(),
        host: "a\"b".to_string(),
        user: "de\\ploy".to_string(),
        auth: "key".to_string(),
        key_path: Some("C:\\keys\\id".to_string()),
        remote_root_dir: "/srv/\"app\"".to_string(),
        local_root_dir: ".".to_string(),
        exclude: vec!["\\d".to_string(), "a\"b".to_string()],
    };

    let text = generate_toml(&input);

    let value: toml::Value =
        toml::from_str(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    let server = &value["servers"]["dev.box"];
    assert_eq!(server["host"].as_str(), Some("a\"b"));
    assert_eq!(server["root_dir"].as_str(), Some("/srv/\"app\""));
    assert_eq!(value["filter"]["exclude"][0].as_str(), Some("\\d"));
}
