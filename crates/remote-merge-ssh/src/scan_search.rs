//! 読む権限はあるのに検索の権限のないディレクトリの、子の名前を集める。
//!
//! findutils 4.4.2 までの find は、そのディレクトリに入れないとディレクトリ全体を読めないと
//! 知らせ、子の名前を出さない。ローカルとエージェントの集め方は名前を一覧でき、子の情報だけを
//! 読めない。経路で結果が変わらないよう、その知らせを受けたディレクトリについてだけ、シェルの
//! glob で子の名前を集めて「情報を読めない項目」として答える（exclude に当たるかは規則が決める）。

use remote_merge_core::scan::raw::{join_relative, Answer, ReadCause, UnreadableDir};

use crate::scan_fields::printed_start;
use crate::scan_script::{assign, root_setup, ScriptQuestion, MARKER};

/// 名前を確かめる一つのディレクトリ：答えの番号と、問いのパスからの相対パス、find に渡した形のパス
struct Unsearched {
    answer: usize,
    rel: Vec<u8>,
    printed: Vec<u8>,
}

/// 答えの中の読めなかったディレクトリ
fn unsearched(questions: &[ScriptQuestion], answers: &[Answer]) -> Vec<Unsearched> {
    let mut dirs = Vec::new();
    for (index, (question, answer)) in questions.iter().zip(answers).enumerate() {
        let Some(tree) = &answer.subtree else {
            continue;
        };
        let start = printed_start(question);
        for dir in &tree.unreadable {
            let printed = join_relative(&start, &dir.path);
            dirs.push(Unsearched {
                answer: index,
                rel: dir.path.clone(),
                printed,
            });
        }
    }
    dirs
}

/// `dirs` の、読めて検索できないものの子の名前を出すスクリプト
pub(crate) fn names_script(root: &[u8], dirs: &[&[u8]]) -> String {
    let mut script = format!(
        "LC_ALL=C; export LC_ALL\nprintf '{MARKER}\\000'\n{}",
        root_setup(root)
    );
    for dir in dirs {
        script.push_str(&assign("d", dir));
        script.push_str(concat!(
            "if [ -r \"$d\" ] && [ ! -x \"$d\" ]; then printf '@N\\000'\n",
            " for n in \"$d\"/* \"$d\"/.[!.]* \"$d\"/..?*; do\n",
            "  case \"$n\" in \"$d/*\"|\"$d/.[!.]*\"|\"$d/..?*\") continue;; esac\n",
            "  printf '%s\\000' \"${n##*/}\"\n",
            " done\n",
            " printf '@Z\\000'\n",
            "else printf '@X\\000'; fi\n",
        ));
    }
    script.push_str("printf '@END\\000'\n");
    script
}

/// [`names_script`] の出力を、ディレクトリごとの子の名前（検索できるか読めないなら None）にする
pub(crate) fn parse_names(
    stdout: &[u8],
    count: usize,
) -> Result<Vec<Option<Vec<Vec<u8>>>>, String> {
    let marker = [MARKER.as_bytes(), b"\0"].concat();
    let begin = stdout
        .windows(marker.len())
        .position(|window| window == marker)
        .ok_or("The scan output has no start marker")?;
    let mut tokens = stdout[begin + marker.len()..].split(|byte| *byte == 0);
    let mut next = || tokens.next().ok_or("The scan output ended early");
    let mut found = Vec::with_capacity(count);
    for _ in 0..count {
        match next()? {
            b"@X" => found.push(None),
            b"@N" => {
                let mut names = Vec::new();
                loop {
                    match next()? {
                        b"@Z" => break,
                        name => names.push(name.to_vec()),
                    }
                }
                found.push(Some(names));
            }
            other => {
                return Err(format!(
                    "Unexpected scan output: {}",
                    String::from_utf8_lossy(other)
                ))
            }
        }
    }
    if next()? != b"@END" {
        return Err("The scan output ended early".into());
    }
    Ok(found)
}

/// 読めなかったディレクトリのうち、読めて検索できないものを、その子の名前ごとの「情報を読めない
/// 項目」に置き換える。`run` はスクリプトを動かして標準出力を返す
pub fn complete_unsearchable(
    root: &[u8],
    questions: &[ScriptQuestion],
    answers: &mut [Answer],
    mut run: impl FnMut(&str) -> Result<Vec<u8>, anyhow::Error>,
) -> Result<(), anyhow::Error> {
    let dirs = unsearched(questions, answers);
    if dirs.is_empty() {
        return Ok(());
    }
    let printed: Vec<&[u8]> = dirs.iter().map(|dir| dir.printed.as_slice()).collect();
    let stdout = run(&names_script(root, &printed))?;
    let found = parse_names(&stdout, dirs.len()).map_err(|message| anyhow::anyhow!(message))?;
    for (dir, names) in dirs.iter().zip(found) {
        let (Some(names), Some(tree)) = (names, answers[dir.answer].subtree.as_mut()) else {
            continue;
        };
        tree.unreadable
            .retain(|unreadable| unreadable.path != dir.rel);
        for name in names {
            let item = join_relative(&dir.rel, &name);
            tree.unreadable_entries
                .push(UnreadableDir::new(item, ReadCause::PermissionDenied));
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn names_below_a_directory_without_search_permission_are_listed_as_they_are() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        for name in ["a", ".h", "..x", "n\nl"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        std::fs::create_dir(temp.path().join("locked")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o644)).unwrap();
        let locked = temp.path().join("locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let script = names_script(
            temp.path().as_os_str().as_encoded_bytes(),
            &[b"./d", b"./locked"],
        );
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = parse_names(&output.stdout, 2).unwrap();
        if found[0].is_none() {
            eprintln!("note: running as root; permissions do not apply");
            return;
        }
        let mut names = found[0].clone().unwrap();
        names.sort();
        assert_eq!(names, [&b"..x"[..], b".h", b"a", b"n\nl"]);
        assert_eq!(found[1], None);
    }
}
