use super::super::merge_support::fixture;
use super::{diff, encodings};
use remote_merge_config::load_config_from_paths;
use remote_merge_core::content::{Encoding, Interpretation};
use tempfile::TempDir;

fn layers(global_content: &str, project_content: &str) -> remote_merge_config::AppConfig {
    let dir = TempDir::new().unwrap();
    let global = dir.path().join("global.toml");
    let project = dir.path().join("project.toml");
    std::fs::write(&global, format!("[local]\nroot_dir='.'\n[servers.legacy]\nhost='example.invalid'\nuser='unused'\nroot_dir='.'\n{global_content}")).unwrap();
    std::fs::write(&project, project_content).unwrap();
    load_config_from_paths(Some(&global), Some(&project)).unwrap()
}

// @kotowari[REQ-config-046, EX-config-073]
#[test]
fn ex_config_073_content_defaults_leave_connection_settings_unchanged() {
    let plain = layers("", "");
    let configured = layers("[content.defaults]\nlegacy='EUC-JP'\n", "");
    assert_eq!(
        configured.content.resolve("legacy", "a", None),
        Interpretation::Text(Encoding::EucJp)
    );
    assert_eq!(
        configured.servers["legacy"].host,
        plain.servers["legacy"].host
    );
    assert_eq!(
        configured.servers["legacy"].port,
        plain.servers["legacy"].port
    );
    for alias in ["shift_jis", "CP932", "windows-31j"] {
        let configured = layers(&format!("[content.defaults]\nlegacy='{alias}'\n"), "");
        assert_eq!(
            configured.content.resolve("legacy", "a", None),
            Interpretation::Text(Encoding::ShiftJis)
        );
    }
}

// @kotowari[REQ-config-046, REQ-config-039, EX-config-074]
#[test]
fn ex_config_074_unknown_codec_side_and_content_keys_are_configuration_errors() {
    for content in [
        "[content.defaults]\nlocal='utf-16'\n",
        "[content.defaults]\nunknown='utf-8'\n",
        "[content]\nunknown=true\n",
        "[[content.rules]]\nside='local'\npath='*'\nencoding='euc-jp'\nunknown=true\n",
    ] {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(&file, format!("[local]\nroot_dir='.'\n{content}")).unwrap();
        assert!(load_config_from_paths(Some(&file), None).is_err());
    }
}

// @kotowari[REQ-config-047, EX-config-075]
#[test]
fn ex_config_075_global_rules_outweigh_project_defaults() {
    let config = layers(
        "[content.defaults]\nlocal='utf-8'\n[[content.rules]]\nside='legacy'\npath='*.txt'\nencoding='cp932'\n",
        "[content.defaults]\nlegacy='euc-jp'\n",
    );
    assert_eq!(
        config.content.resolve("legacy", "a.txt", None),
        Interpretation::Text(Encoding::ShiftJis)
    );
    assert_eq!(
        config.content.resolve("legacy", "a.php", None),
        Interpretation::Text(Encoding::EucJp)
    );
    assert_eq!(
        config.content.resolve("local", "a.php", None),
        Interpretation::Text(Encoding::Utf8)
    );
}

// @kotowari[REQ-config-047, EX-config-076]
#[test]
fn ex_config_076_last_project_exception_overrides_inherited_rules() {
    let config = layers(
        "[[content.rules]]\nside='legacy'\npath='*.txt'\nencoding='cp932'\n",
        "[[content.rules]]\nside='legacy'\npath='new/a.txt'\nencoding='utf-8'\n",
    );
    assert_eq!(
        config.content.resolve("legacy", "new/a.txt", None),
        Interpretation::Text(Encoding::Utf8)
    );
    assert_eq!(
        config.content.resolve("legacy", "old/a.txt", None),
        Interpretation::Text(Encoding::ShiftJis)
    );
}

// @kotowari[REQ-config-048, EX-config-077]
#[test]
fn ex_config_077_mixed_remote_content_resolves_identically_for_every_scope() {
    let fixture = fixture();
    fixture.append_config("[content.defaults]\nlocal='utf-8'\n[[content.rules]]\nside='develop'\npath='old/**'\nencoding='euc-jp'\n[[content.rules]]\nside='develop'\npath='new/**'\nencoding='utf-8'\n");
    for side in ["local", "develop"] {
        for directory in ["old", "new"] {
            fixture.create_dir(side, directory);
        }
    }
    fixture.write("local", "old/a.php", "う\n");
    fixture.write("local", "new/a.php", "う\n");
    fixture.write_bytes("develop", "old/a.php", b"\xa4\xa2\n");
    fixture.write("develop", "new/a.php", "あ\n");
    for paths in [vec!["old/a.php", "new/a.php"], vec!["old", "new"], vec![]] {
        let (json, code) = diff(&fixture, &paths, Default::default(), false);
        assert_eq!(code, 1);
        for (path, encoding) in [("old/a.php", "euc-jp"), ("new/a.php", "utf-8")] {
            let file = super::super::diff_support::entry(&json, path);
            assert_eq!(file["encodings"]["right"], encoding);
            assert!(
                super::super::diff_support::lines_of(file)
                    .iter()
                    .any(|(_, text)| text == "あ")
            );
        }
    }
}

// @kotowari[REQ-config-049, EX-config-078]
#[test]
fn ex_config_078_reference_override_does_not_leak_to_either_side() {
    let fixture = fixture();
    fixture.append_config("[content.defaults]\nlocal='utf-8'\ndevelop='cp932'\nstaging='utf-8'\n");
    fixture.write("local", "a", "あ\n");
    fixture.write_bytes("develop", "a", b"\x82\xa2\n");
    fixture.write_bytes("staging", "a", b"\xa4\xa6\n");
    let mut overrides = encodings(Encoding::Utf8, Encoding::ShiftJis);
    overrides.left = None;
    overrides.right = None;
    overrides.reference = Some(Interpretation::Text(Encoding::EucJp));
    let (json, code) = diff(&fixture, &["a"], overrides, true);
    assert_eq!(code, 1);
    assert_eq!(
        json["files"][0]["encodings"],
        serde_json::json!({"left":"utf-8", "right":"shift_jis", "ref":"euc-jp"})
    );
}
