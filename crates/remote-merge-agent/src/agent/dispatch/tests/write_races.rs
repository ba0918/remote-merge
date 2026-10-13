//! 書き込みの途中で書き込み先のパスが差し替えられたとき

use super::support::*;

fn part(path: &str, content: &[u8], more_to_follow: bool) -> AgentRequest {
    AgentRequest::WriteFile {
        path: path.into(),
        content: content.to_vec(),
        is_binary: true,
        more_to_follow,
    }
}

#[test]
fn later_parts_go_to_the_file_opened_for_the_first_part_even_if_the_path_is_swapped() {
    let (tmp, mut d) = setup();
    let root = tmp.path();
    fs::write(root.join("victim"), "secret").unwrap();

    let first = single(d.dispatch(part("big.bin", b"one", true)).unwrap());
    // 分の間に、書き込み先を別の名前へ移し、そのパスに別のファイルへのハードリンクを置く
    fs::rename(root.join("big.bin"), root.join("moved.bin")).unwrap();
    fs::hard_link(root.join("victim"), root.join("big.bin")).unwrap();
    let second = single(d.dispatch(part("big.bin", b"two", false)).unwrap());

    assert_eq!(first, AgentResponse::Changed { failure: None });
    assert_eq!(second, AgentResponse::Changed { failure: None });
    assert_eq!(fs::read(root.join("moved.bin")).unwrap(), b"onetwo");
    assert_eq!(fs::read_to_string(root.join("victim")).unwrap(), "secret");
}

// 消そうとしたものが消えていれば消せたもの、作ったディレクトリが別のものに替わっていれば残ったもの
// （ローカルとSSH の経路と同じ判断）
#[test]
fn a_created_entry_or_directory_gone_is_removed_and_a_replaced_directory_is_left() {
    use super::super::ownership::OwnerOps as _;
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("a.txt");
    let file = fs::File::create(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::write(tmp.path().join("was_dir"), "x").unwrap();
    let config = MetadataConfig::default();

    let mut owner = super::super::writes::Owner {
        config: &config,
        entry: &path,
        file: Some(&file),
    };

    assert!(owner.remove_entry().is_ok());
    assert!(owner.remove_dir(&tmp.path().join("gone")).is_ok());
    assert!(owner.remove_dir(&tmp.path().join("was_dir")).is_err());
    assert!(tmp.path().join("was_dir").exists());
}

#[test]
fn a_created_file_whose_path_was_swapped_is_not_removed_in_its_place() {
    use super::super::ownership::OwnerOps as _;
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("a.txt");
    let file = fs::File::create(&path).unwrap();
    fs::rename(&path, tmp.path().join("moved.txt")).unwrap();
    fs::write(&path, "someone else's").unwrap();
    let config = MetadataConfig::default();

    let mut owner = super::super::writes::Owner {
        config: &config,
        entry: &path,
        file: Some(&file),
    };

    assert!(owner.remove_entry().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "someone else's");
}
