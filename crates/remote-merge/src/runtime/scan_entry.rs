//! 走査の一つの入口。全体の走査も、指定したディレクトリの下の走査も、include の値の確認と
//! 起点の決め方（core の scan::starts）を通り、root_dir の確認は走査の最初の問いで側ごとに
//! 一度だけ行う（docs/decision/records/2026-10-12-foundation-2.md の C9）。

use std::path::Path;

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::include::{decide_value, is_inside};
use remote_merge_core::scan::outcome::ScanRequest;
use remote_merge_core::scan::starts::scope_for;

use super::core::CoreRuntime;
use super::target_io::TargetPath;
use crate::side::Side;
use crate::tree::FileTree;

/// 側ごとに確かめた include の値と、確かめたときの root_dir と include の設定。設定が変われば
/// 確かめ直す
#[derive(Debug, Clone)]
pub(crate) struct CheckedInclude {
    basis: (Option<std::path::PathBuf>, Vec<String>),
    values: Option<Vec<String>>,
}

impl CoreRuntime {
    /// `side` を走査する。`dirs` は指定したディレクトリ（None なら root_dir 全体）で、件数の上限は
    /// ディレクトリごとに当てる（TBL-scan-001）。exclude と include は root_dir からのパスに当てる
    /// （REQ-scan-019）
    pub(crate) fn scan_side(
        &mut self,
        side: &Side,
        dirs: Option<&[String]>,
        max_entries: usize,
        fail_on_truncation: bool,
    ) -> anyhow::Result<FileTree> {
        if let Some(dirs) = dirs {
            for dir in dirs {
                RelPath::parse(dir)?;
            }
        }
        let include = self.checked_include(side)?;
        let request = ScanRequest {
            scope: scope_for(dirs, include.as_deref()),
            max_entries,
        };
        let mut io = super::target_io::for_side(side, self);
        io.scan_tree(self, &request, fail_on_truncation)
    }

    /// 走査する側で確かめて残った include の値。include の指定がなければ None。ない値と root_dir
    /// の外へ解決される値は警告して飛ばし、確認がエラーになった値はそのエラーで止める
    /// （docs/ir/config/include.md の REQ-config-032・033）
    fn checked_include(&mut self, side: &Side) -> anyhow::Result<Option<Vec<String>>> {
        let basis = (self.root_dir_of(side), self.config.filter.include.clone());
        if let Some(checked) = self.checked_include.get(side) {
            if checked.basis == basis {
                return Ok(checked.values.clone());
            }
        }
        let values = self.check_include(side)?;
        self.checked_include.insert(
            side.clone(),
            CheckedInclude {
                basis,
                values: values.clone(),
            },
        );
        Ok(values)
    }

    /// 設定に書いた `side` の root_dir（確かめた値を使い回してよいかを決めるのに使う）
    fn root_dir_of(&self, side: &Side) -> Option<std::path::PathBuf> {
        match side {
            Side::Local => Some(self.config.local.root_dir.clone()),
            Side::Remote(name) => self.config.servers.get(name).map(|s| s.root_dir.clone()),
        }
    }

    fn check_include(&mut self, side: &Side) -> anyhow::Result<Option<Vec<String>>> {
        let values = self.config.filter.include.clone();
        if values.is_empty() {
            return Ok(None);
        }
        let real_root = match self.inspect_path_for_rollback(side, "")? {
            ResolvedPath::Exists { real_path, .. } | ResolvedPath::Symlink { real_path, .. } => {
                real_path
            }
            // root_dir がないか読めないことは、走査の最初の問いで報告する
            _ => return Ok(Some(Vec::new())),
        };
        let mut kept = Vec::new();
        for value in values {
            let resolved = self.inspect_path_for_rollback(side, &value)?;
            let decision = decide_value(&resolved, &real_root)?;
            match decision.warning(&value) {
                Some(warning) => tracing::warn!("{warning}"),
                None => kept.push(value),
            }
        }
        Ok(Some(kept))
    }
}

/// 確認したパスが、symlink を辿ると root_dir の実パス `root` の外にあるか。ないパスは、親の実パス
/// で決める
pub(crate) fn resolved_path_outside_root(path: &TargetPath, root: &Path) -> bool {
    let real = match path {
        TargetPath::Missing { real_parent } => real_parent,
        TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => real_path,
    };
    !is_inside(&real.to_string_lossy(), &root.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_path_resolving_outside_the_root_dir_is_outside() {
        let root = Path::new("/srv/app");
        let file = |p: &str| TargetPath::File {
            real_path: PathBuf::from(p),
        };
        assert!(!resolved_path_outside_root(&file("/srv/app/a"), root));
        assert!(resolved_path_outside_root(&file("/srv/shared/a"), root));
        let missing = TargetPath::Missing {
            real_parent: PathBuf::from("/srv/shared"),
        };
        assert!(resolved_path_outside_root(&missing, root));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn changed_include_values_are_checked_again() {
        let temp = tempfile::tempdir().unwrap();
        for path in ["a/x.txt", "b/y.txt"] {
            let file = temp.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "x").unwrap();
        }
        let mut core = CoreRuntime::new_for_test();
        core.config.local.root_dir = temp.path().to_path_buf();
        core.config.filter.include = vec!["a".into()];
        let first = core.fetch_tree_recursive(&Side::Local, 100, true).unwrap();
        assert!(first.find_node("a/x.txt").is_some());
        core.config.filter.include = vec!["b".into()];
        let second = core.fetch_tree_recursive(&Side::Local, 100, true).unwrap();
        assert!(second.find_node("b/y.txt").is_some());
        assert!(second.find_node("a/x.txt").is_none());
    }
}
