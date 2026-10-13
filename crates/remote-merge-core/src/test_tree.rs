//! 試験の一時ディレクトリと、試験が外した権限。外した権限は、試験が通っても panic しても、Drop で
//! 外した逆の順に元へ戻す。戻せなかったことと消せなかったことは黙って捨てない
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

/// 試験が権限を外した項目。一時ディレクトリをほかの道具が持つときは、その道具より後に作り、先に
/// 落ちるようにする
#[derive(Default)]
pub struct PermissionLocks {
    locked: RefCell<Vec<(PathBuf, fs::Permissions)>>,
}

impl PermissionLocks {
    pub fn new() -> Self {
        Self::default()
    }

    /// `path` の権限を `mode` にする。Drop で元の権限へ戻す
    pub fn lock(&self, path: impl AsRef<Path>, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let path = path.as_ref();
        self.keep(path);
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .unwrap_or_else(|error| panic!("cannot change {}: {error}", path.display()));
    }

    /// `path` の今の権限を覚える。試験が別の手段（子のプロセスなど）で権限を外しても、Drop でこの
    /// 権限へ戻す
    pub fn keep(&self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let original = fs::symlink_metadata(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
            .permissions();
        self.locked
            .borrow_mut()
            .push((path.to_path_buf(), original));
    }

    /// 外した逆の順に元の権限へ戻し、戻せなかった項目を返す
    fn restore(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (path, original) in self.locked.take().into_iter().rev() {
            match fs::set_permissions(&path, original) {
                Ok(()) => {}
                // 試験が自分で消したもの
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => problems.push(format!("cannot restore {}: {error}", path.display())),
            }
        }
        problems
    }
}

impl Drop for PermissionLocks {
    fn drop(&mut self) {
        report(self.restore());
    }
}

/// 試験の一時ディレクトリ。Drop で、外した権限を戻してから消す
pub struct TestTree {
    dir: Option<tempfile::TempDir>,
    locks: PermissionLocks,
}

impl TestTree {
    pub fn new() -> Self {
        Self {
            dir: Some(tempfile::tempdir().expect("cannot create a temporary directory")),
            locks: PermissionLocks::new(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.as_ref().expect("the tree is open").path()
    }

    /// `path` の権限を `mode` にする。Drop で、消す前に元の権限へ戻す
    pub fn lock(&self, path: impl AsRef<Path>, mode: u32) {
        self.locks.lock(path, mode);
    }
}

impl Default for TestTree {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestTree {
    fn drop(&mut self) {
        let mut problems = self.locks.restore();
        if let Some(dir) = self.dir.take() {
            let path = dir.path().to_path_buf();
            if let Err(error) = dir.close() {
                problems.push(format!("cannot remove {}: {error}", path.display()));
            }
        }
        report(problems);
    }
}

/// 戻せなかったことと消せなかったことを知らせる。試験がもう panic しているなら、二重の panic で
/// 中断させずに標準エラーに出す
fn report(problems: Vec<String>) {
    if problems.is_empty() {
        return;
    }
    let message = problems.join("; ");
    if std::thread::panicking() {
        eprintln!("{message}");
    } else {
        panic!("{message}");
    }
}

#[cfg(test)]
#[path = "test_tree_tests.rs"]
mod tests;
