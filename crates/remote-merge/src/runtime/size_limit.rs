//! ファイルの大きさの上限（docs/ir/cli/size-limit.md の REQ-cli-079）を、中身を読む前に
//! 大きさで当てる方針。
//!
//! 判定は `CoreRuntime` の四つの読み込みの入口だけが使い、経路（ローカル・SSH・エージェント）は
//! 大きさを答えるだけにする。経路の中に判定を置くと、経路ごとに判定が分かれる。

use std::collections::HashMap;

pub use crate::merge::executor::MAX_BINARY_FILE_SIZE;

/// 読み込みに当てる上限。--force を持つコマンドは --force から決め、status は上限なしにする
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeLimit {
    /// 上限を超えるファイルを読まない
    Enforced,
    /// 大きさを尋ねずに読む
    Lifted,
}

impl SizeLimit {
    pub fn from_force(force: bool) -> Self {
        if force {
            Self::Lifted
        } else {
            Self::Enforced
        }
    }
}

/// 経路が答えたパスごとの大きさ（symlink は辿った先のファイルの大きさ）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileSize {
    Known(u64),
    /// パスごとの失敗（ない、権限がない）。読み込みに進めて、その読み込みの失敗を返す
    Unknown,
    /// 大きさを尋ねられなかった。上限を外して読まず、この理由で読み込みの失敗にする
    Unavailable(String),
}

/// 上限を超えたので中身を読まなかったファイル
#[derive(Debug, thiserror::Error)]
#[error(
    "File too large ({size} bytes > {limit} bytes limit): {path}. Use --force to override.",
    limit = MAX_BINARY_FILE_SIZE
)]
pub struct FileTooLarge {
    pub path: String,
    pub size: u64,
}

/// `path` を読まない理由。読んでよければ None。大きさの答えがないパスは読む
pub(crate) fn refusal(path: &str, size: Option<&FileSize>) -> Option<anyhow::Error> {
    match size? {
        FileSize::Known(size) if *size > MAX_BINARY_FILE_SIZE as u64 => Some(
            FileTooLarge {
                path: path.to_string(),
                size: *size,
            }
            .into(),
        ),
        FileSize::Known(_) | FileSize::Unknown => None,
        FileSize::Unavailable(reason) => Some(anyhow::anyhow!("{reason}: {path}")),
    }
}

/// 一括読み込みで経路に読ませるパス。読まないパスは結果に入れず、呼び出し元が一件ずつ読み直して
/// 理由を受け取る
pub(crate) fn readable_paths(paths: &[String], sizes: &HashMap<String, FileSize>) -> Vec<String> {
    paths
        .iter()
        .filter(|path| refusal(path, sizes.get(path.as_str())).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sizes(entries: &[(&str, FileSize)]) -> HashMap<String, FileSize> {
        entries
            .iter()
            .map(|(path, size)| (path.to_string(), size.clone()))
            .collect()
    }

    #[test]
    fn force_lifts_the_limit() {
        assert_eq!(SizeLimit::from_force(true), SizeLimit::Lifted);
        assert_eq!(SizeLimit::from_force(false), SizeLimit::Enforced);
    }

    #[test]
    fn a_file_of_exactly_the_limit_is_read() {
        let limit = MAX_BINARY_FILE_SIZE as u64;
        assert!(refusal("f", Some(&FileSize::Known(limit))).is_none());
    }

    #[test]
    fn a_file_one_byte_over_the_limit_is_refused_as_too_large() {
        let limit = MAX_BINARY_FILE_SIZE as u64;
        let refusal = refusal("f", Some(&FileSize::Known(limit + 1))).unwrap();
        let too_large = refusal.downcast_ref::<FileTooLarge>().unwrap();
        assert_eq!(too_large.path, "f");
        assert_eq!(too_large.size, limit + 1);
    }

    #[test]
    fn a_path_whose_size_is_unknown_is_read() {
        assert!(refusal("f", Some(&FileSize::Unknown)).is_none());
        assert!(refusal("f", None).is_none());
    }

    #[test]
    fn a_path_whose_size_could_not_be_asked_is_refused_with_that_reason() {
        let refusal = refusal("f", Some(&FileSize::Unavailable("no route".into()))).unwrap();
        assert!(refusal.downcast_ref::<FileTooLarge>().is_none());
        assert!(refusal.to_string().contains("no route"), "{refusal}");
    }

    #[test]
    fn only_the_paths_within_the_limit_are_read_from_a_batch() {
        let limit = MAX_BINARY_FILE_SIZE as u64;
        let paths: Vec<String> = ["small", "big", "unknown", "unavailable"]
            .map(String::from)
            .to_vec();
        let sizes = sizes(&[
            ("small", FileSize::Known(1)),
            ("big", FileSize::Known(limit + 1)),
            ("unknown", FileSize::Unknown),
            ("unavailable", FileSize::Unavailable("no route".into())),
        ]);

        assert_eq!(readable_paths(&paths, &sizes), vec!["small", "unknown"]);
    }
}
