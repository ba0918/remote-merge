//! SSH の経路の中身とハッシュ（docs/decision/records/2026-10-12-foundation-3.md の D7）。
//! 複数のパスをまとめて尋ね、答えはパスごとに成功の値か失敗の種類を持つ。

use remote_merge_core::op_failure::OpFailure;

use crate::contents_output::{parse_hash, parse_read};
use crate::contents_script::{hash, read, split};
use crate::contents_stream::{raw, RawDigests};

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

/// 中身をパスごとに読む。`limit` を超えるファイルは読まずに上限を超える失敗にする。
/// `boundary` は中身の終わりを見分ける区切りで、読むファイルの中身から推し量れない値を呼び出し側が
/// 渡す（16 進の数字だけ）
pub fn read_all<E>(
    asked: &[(String, String)],
    limit: Option<u64>,
    boundary: &str,
    max_len: usize,
    mut run: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<PathAnswer<Vec<u8>>>, ContentsFailure<E>> {
    in_commands(asked, &read(&[], limit, boundary), max_len, |paths| {
        let stdout = run(&read(paths, limit, boundary)).map_err(ContentsFailure::Run)?;
        parse_read(&stdout, paths.len(), limit, boundary).map_err(ContentsFailure::Unreadable)
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

/// 通常ファイルの中身をまとめて尋ね、受けながら手元で SHA-256 を取る（コマンド行の長さで分ける）。
/// `run` はスクリプトを動かし、標準出力を受けた順に渡された `RawDigests` へ渡す
pub fn digest_all<E>(
    fulls: &[String],
    boundary: &str,
    max_len: usize,
    mut run: impl FnMut(&str, &mut RawDigests) -> Result<(), E>,
) -> Result<Vec<Result<String, OpFailure>>, ContentsFailure<E>> {
    let mut digests = Vec::with_capacity(fulls.len());
    for range in split(&raw(&[], boundary), fulls, max_len) {
        let part = &fulls[range];
        let mut answer = RawDigests::new(boundary);
        run(&raw(part, boundary), &mut answer).map_err(ContentsFailure::Run)?;
        digests.extend(
            answer
                .finish(part.len())
                .map_err(ContentsFailure::Unreadable)?,
        );
    }
    Ok(digests)
}

/// ハッシュを取る道具がなくて値のないパス（`Ok(None)`）を、`digest`（そのパスのリモートのパスを
/// まとめて受けて、パスごとのハッシュか失敗の種類を同じ順で返す）で埋める。順と重なったパスは保つ
pub fn fill_unhashed<E>(
    asked: &[(String, String)],
    hashes: Vec<PathAnswer<Option<String>>>,
    digest: impl FnOnce(&[String]) -> Result<Vec<Result<String, OpFailure>>, E>,
) -> Result<Vec<PathAnswer<String>>, E> {
    let unhashed: Vec<String> = asked
        .iter()
        .zip(&hashes)
        .filter(|(_, (_, hash))| matches!(hash, Ok(None)))
        .map(|((_, full), _)| full.clone())
        .collect();
    let mut digests = if unhashed.is_empty() {
        Vec::new()
    } else {
        digest(&unhashed)?
    }
    .into_iter();
    Ok(hashes
        .into_iter()
        .map(|(key, hash)| {
            let value = match hash {
                Ok(Some(value)) => Ok(value),
                Ok(None) => digests.next().unwrap_or_else(|| {
                    Err(OpFailure::Disconnected(
                        remote_merge_core::op_failure::Disconnection::NoAnswer(
                            "no digest for a path".into(),
                        ),
                    ))
                }),
                Err(failure) => Err(failure),
            };
            (key, value)
        })
        .collect())
}

#[cfg(all(test, unix))]
#[path = "contents_tests.rs"]
mod tests;
