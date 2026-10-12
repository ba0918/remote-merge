//! エージェントが通信で受け取ったパスの、入力の形の検査。

use remote_merge_core::relative_path::{EntryPath, RelPathError};

/// 複数の項目のパスを、項目として検査を通ったものと拒んだもの（root_dir 全体を指す値を含む）に
/// 分ける。どちらも受け取った順のまま
pub fn split_entries(paths: &[String]) -> (Vec<EntryPath>, Vec<RelPathError>) {
    let mut entries = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match EntryPath::parse(path) {
            Ok(entry) => entries.push(entry),
            Err(refusal) => refused.push(refusal),
        }
    }
    (entries, refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_split_from_the_whole_root_dir_in_the_order_received() {
        let (entries, refused) = split_entries(&[
            "a".into(),
            "../b".into(),
            "./".into(),
            "c/".into(),
            "/d".into(),
        ]);
        let typed: Vec<&str> = entries.iter().map(EntryPath::as_typed).collect();
        assert_eq!(typed, ["a", "c/"]);
        assert_eq!(
            refused,
            [
                RelPathError::Traversal("../b".into()),
                RelPathError::RootDirItself("./".into()),
                RelPathError::Absolute("/d".into())
            ]
        );
    }
}
