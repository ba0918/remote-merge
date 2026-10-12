//! エージェントが通信で受け取ったパスの、入力の形の検査。

use remote_merge_core::relative_path::{RelPath, RelPathError};

/// 複数のパスを、検査を通ったものと拒んだものに分ける。どちらも受け取った順のまま
pub fn split_checked(paths: &[String]) -> (Vec<RelPath>, Vec<RelPathError>) {
    let mut checked = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match RelPath::parse(path) {
            Ok(path) => checked.push(path),
            Err(refusal) => refused.push(refusal),
        }
    }
    (checked, refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_split_in_the_order_received() {
        let (checked, refused) =
            split_checked(&["a".into(), "../b".into(), "c/".into(), "/d".into()]);
        let typed: Vec<&str> = checked.iter().map(RelPath::as_typed).collect();
        assert_eq!(typed, ["a", "c/"]);
        assert_eq!(
            refused,
            [
                RelPathError::Traversal("../b".into()),
                RelPathError::Absolute("/d".into())
            ]
        );
    }
}
