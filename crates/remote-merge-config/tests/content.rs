use remote_merge_config::load_config_from_paths;
use remote_merge_core::content::{Encoding, Interpretation};

#[test]
fn global_rules_survive_project_defaults_and_later_rules_win() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("global.toml");
    let project = dir.path().join("project.toml");
    std::fs::write(&global, "[local]\nroot_dir='.'\n[content.defaults]\nlocal='cp932'\n[[content.rules]]\nside='local'\npath='*.txt'\nencoding='euc-jp'\n").unwrap();
    std::fs::write(&project, "[content.defaults]\nlocal='UTF-8'\n[[content.rules]]\nside='local'\npath='special.txt'\nencoding='windows-31j'\n").unwrap();
    let config = load_config_from_paths(Some(&global), Some(&project)).unwrap();
    assert_eq!(
        config.content.resolve("local", "x.txt", None),
        Interpretation::Text(Encoding::EucJp)
    );
    assert_eq!(
        config.content.resolve("local", "special.txt", None),
        Interpretation::Text(Encoding::ShiftJis)
    );
    assert_eq!(
        config.content.resolve("local", "x.rs", None),
        Interpretation::Text(Encoding::Utf8)
    );
}
