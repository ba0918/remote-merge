//! SSH の経路の中身とハッシュ（docs/decision/records/2026-10-12-foundation-3.md の D7）。
//! 複数のパスをまとめて尋ね、答えはパスごとに成功の値か失敗の種類を持つ。

use remote_merge_core::op_failure::OpFailure;

use crate::contents_output::{parse_hash, parse_read};
use crate::contents_script::{hash, read, split};

/// 一つのパスの答え（結果のキーと、値か失敗の種類）
pub type PathAnswer<T> = (String, Result<T, OpFailure>);

/// まとめて尋ねられなかった理由
#[derive(Debug)]
pub enum ContentsFailure<E> {
    Run(E),
    /// 答えを問いへの答えとして読めなかった（接続断として扱う）
    Unreadable(String),
}

/// `asked`（結果のキーとリモートのパス）を、コマンド行の長さで分けて尋ねる
fn in_commands<T, E>(
    asked: &[(String, String)],
    base: &str,
    max_len: usize,
    mut ask: impl FnMut(&[String]) -> Result<Vec<Result<T, OpFailure>>, ContentsFailure<E>>,
) -> Result<Vec<PathAnswer<T>>, ContentsFailure<E>> {
    let paths: Vec<String> = asked.iter().map(|(_, path)| path.clone()).collect();
    let mut answers = Vec::with_capacity(asked.len());
    for range in split(base, &paths, max_len) {
        let results = ask(&paths[range.clone()])?;
        let keys = asked[range].iter().map(|(key, _)| key.clone());
        answers.extend(keys.zip(results));
    }
    Ok(answers)
}

/// 中身をパスごとに読む。`limit` を超えるファイルは読まずに上限を超える失敗にする
pub fn read_all<E>(
    asked: &[(String, String)],
    limit: Option<u64>,
    max_len: usize,
    mut run: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<PathAnswer<Vec<u8>>>, ContentsFailure<E>> {
    in_commands(asked, &read(&[], limit), max_len, |paths| {
        let stdout = run(&read(paths, limit)).map_err(ContentsFailure::Run)?;
        parse_read(&stdout, paths.len(), limit).map_err(ContentsFailure::Unreadable)
    })
}

/// パスごとの比べる値（symlink はリンク先、それ以外は中身の SHA-256）を取る。値が None の
/// パスは、ハッシュを取る道具がないので、呼び出し側が中身を読んで取る
pub fn hash_all<E>(
    asked: &[(String, String)],
    max_len: usize,
    mut run: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<PathAnswer<Option<String>>>, ContentsFailure<E>> {
    in_commands(asked, &hash(&[]), max_len, |paths| {
        let stdout = run(&hash(paths)).map_err(ContentsFailure::Run)?;
        parse_hash(&stdout, paths.len()).map_err(ContentsFailure::Unreadable)
    })
}

#[cfg(all(test, unix))]
#[path = "contents_tests.rs"]
mod tests;
