//! エージェントの走査が降りたディレクトリの祖先の並び（docs/ir/scan/directory-links.md の
//! REQ-scan-011）。
//!
//! ディレクトリを指す symlink の配下を辿ると、祖先へ戻るリンクで走査が終わらなくなる。降りる
//! ディレクトリがその祖先のどれかと同じデバイス番号と inode 番号なら循環とする。ローカルの走査
//! （walkdir）と SSH の走査（find -L）と同じく祖先とだけ比べ、同じディレクトリを指す兄弟の
//! リンクはどちらも辿る。

use std::path::Path;

/// ディレクトリを見分ける値（Unix ではデバイス番号と inode 番号）
#[cfg(unix)]
type DirKey = (u64, u64);
#[cfg(not(unix))]
type DirKey = std::path::PathBuf;

#[cfg(unix)]
fn dir_key(path: &Path) -> std::io::Result<DirKey> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path)?;
    Ok((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn dir_key(path: &Path) -> std::io::Result<DirKey> {
    path.canonicalize()
}

/// 走査の起点からあるディレクトリまでの祖先（そのディレクトリ自体を含む）
#[derive(Debug, Clone, Default)]
pub struct DirAncestry(Vec<DirKey>);

/// 降りようとしたディレクトリが祖先と同じだった。走査を止め、不完全な一覧として報告する
#[derive(Debug)]
pub struct DirectoryCycle(pub String);

impl std::fmt::Display for DirectoryCycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DirectoryCycle {}

impl DirAncestry {
    /// この祖先の並びのディレクトリの子 `dir`（symlink ならリンク先）へ降りるときの祖先の並び。
    /// `dir` が祖先のどれかと同じなら循環
    pub fn descend(&self, dir: &Path) -> std::io::Result<Result<Self, DirectoryCycle>> {
        let key = dir_key(dir)?;
        if self.0.contains(&key) {
            return Ok(Err(DirectoryCycle(format!(
                "File system loop found: {} points to an ancestor directory",
                dir.display()
            ))));
        }
        let mut keys = self.0.clone();
        keys.push(key);
        Ok(Ok(Self(keys)))
    }
}
