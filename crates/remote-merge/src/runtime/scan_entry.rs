//! 走査の一つの入口。全体の走査も、指定したディレクトリの下の走査も、include の値の確認と
//! 起点の決め方（core の scan::starts）を通り、root_dir の確認は走査の最初の問いで側ごとに
//! 一度だけ行う（docs/decision/records/2026-10-12-foundation-2.md の C9）。

use remote_merge_core::path_resolution::ResolvedPath;
use remote_merge_core::relative_path::RelPath;
use remote_merge_core::scan::include::decide_value;

use super::core::CoreRuntime;
use crate::side::Side;

/// 側ごとに確かめた include の値と、確かめたときの root_dir と include の設定。設定が変われば
/// 確かめ直す
#[derive(Debug, Clone)]
pub(crate) struct CheckedInclude {
    basis: (Option<std::path::PathBuf>, Vec<String>),
    values: Option<Vec<String>>,
}

impl CoreRuntime {
    /// 走査する側で確かめて残った include の値。include の指定がなければ None。ない値と root_dir
    /// の外へ解決される値は警告して飛ばし、確認がエラーになった値はそのエラーで止める
    /// （docs/ir/config/include.md の REQ-config-032・033）
    pub(crate) fn checked_include(&mut self, side: &Side) -> anyhow::Result<Option<Vec<String>>> {
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
        // root_dir と include の値を、経路に一度にまとめて尋ねる
        let mut asked = vec![RelPath::parse("")?];
        for value in &values {
            asked.push(RelPath::parse(value)?);
        }
        let mut io = super::target_io::for_side(side, self);
        let mut answers = io
            .inspect_paths(self, &asked)
            .map_err(super::connection::label_read)?
            .into_iter();
        let real_root = match answers.next() {
            Some(Ok(ResolvedPath::Exists { real_path, .. }))
            | Some(Ok(ResolvedPath::Symlink { real_path, .. })) => real_path,
            // root_dir がないか読めないことは、走査の最初の問いで報告する
            Some(Ok(_)) => return Ok(Some(Vec::new())),
            Some(Err(error)) => return Err(super::connection::label_read(error)),
            None => anyhow::bail!("the include values were not all answered"),
        };
        let mut kept = Vec::new();
        for value in values {
            let resolved = match answers.next() {
                Some(answer) => answer.map_err(super::connection::label_read)?,
                None => anyhow::bail!("the include values were not all answered"),
            };
            let decision = decide_value(&resolved, &real_root)?;
            match decision.warning(&value) {
                Some(warning) => tracing::warn!(target: "remote_merge::warning", "{warning}"),
                None => kept.push(value),
            }
        }
        Ok(Some(kept))
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
        let first = core.checked_include(&Side::Local).unwrap();
        assert_eq!(first, Some(vec!["a".to_string()]));
        core.config.filter.include = vec!["b".into()];
        let second = core.checked_include(&Side::Local).unwrap();
        assert_eq!(second, Some(vec!["b".to_string()]));
    }
}
