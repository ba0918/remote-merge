//! 全体の走査の前に、include の値を走査する側のパスの確認で確かめる
//! （docs/ir/config/include.md の REQ-config-032・033）。
//!
//! パスの確認はローカル・SSH・エージェントで同じ結果を返すので、ここで値を飛ばすかを決めれば、
//! どの経路の走査にも確かめて残った値だけが渡る。警告はこの CLI のプロセスで標準エラーに出す。

use std::path::Path;

use crate::side::Side;

use super::core::CoreRuntime;
use super::target_io::TargetPath;

/// 確認の結果から決める、include の値の扱い
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncludeDisposition {
    Scan,
    Missing,
    OutsideRoot,
}

/// 全体の走査に渡す include
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IncludeToScan {
    /// 走査に渡す値。空なら include の制限がない
    Values(Vec<String>),
    /// 値がすべて飛ばされたので、走査せずに空の一覧にする
    Nothing,
}

/// 確認したパスの実パス（存在しないときは親の実パス）が、実パスに直した root_dir `root` の下にないか
pub(crate) fn resolved_path_outside_root(path: &TargetPath, root: &Path) -> bool {
    match path {
        TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => {
            !real_path.starts_with(root)
        }
        TargetPath::Missing { real_parent } => !real_parent.starts_with(root),
    }
}

/// include の値の確認の結果と、実パスに直した root_dir から扱いを決める
pub(crate) fn include_disposition(path: &TargetPath, real_root: &Path) -> IncludeDisposition {
    if resolved_path_outside_root(path, real_root) {
        IncludeDisposition::OutsideRoot
    } else if matches!(path, TargetPath::Missing { .. }) {
        IncludeDisposition::Missing
    } else {
        IncludeDisposition::Scan
    }
}

impl CoreRuntime {
    /// `side` の全体の走査に渡す include を、値ごとのパスの確認で決める。
    ///
    /// 確認は root_dir の 1 回と値ごとの 1 回だけ行う。root_dir が存在しなければ値を確かめずに
    /// そのまま返し、走査の層の root_dir のエラーに任せる。確認がエラーになった値はそのエラーで止める
    pub(crate) fn include_to_scan(&mut self, side: &Side) -> anyhow::Result<IncludeToScan> {
        let include = self.config.filter.include.clone();
        if include.is_empty() {
            return Ok(IncludeToScan::Values(include));
        }
        let real_root = match self.inspect_path(side, "")? {
            TargetPath::Missing { .. } => return Ok(IncludeToScan::Values(include)),
            TargetPath::File { real_path } | TargetPath::Symlink { real_path, .. } => real_path,
        };
        let mut kept = Vec::new();
        for value in include {
            let inspected = self.inspect_path(side, &value)?;
            match include_disposition(&inspected, &real_root) {
                IncludeDisposition::Scan => kept.push(value),
                IncludeDisposition::Missing => tracing::warn!(
                    target: "remote_merge::include",
                    "Include path does not exist, skipping: {value}"
                ),
                IncludeDisposition::OutsideRoot => tracing::warn!(
                    target: "remote_merge::include",
                    "Include path resolves outside root_dir, skipping: {value}"
                ),
            }
        }
        Ok(if kept.is_empty() {
            IncludeToScan::Nothing
        } else {
            IncludeToScan::Values(kept)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/srv/app")
    }

    #[test]
    fn values_inside_the_root_dir_are_scanned_and_missing_ones_skipped() {
        let cases = [
            (
                TargetPath::File {
                    real_path: "/srv/app/src".into(),
                },
                IncludeDisposition::Scan,
            ),
            (
                // リンク切れでもリンク先が root_dir の中なら走査に渡す
                TargetPath::Symlink {
                    link_target: "gone".into(),
                    real_path: "/srv/app/gone".into(),
                },
                IncludeDisposition::Scan,
            ),
            (
                TargetPath::Missing {
                    real_parent: "/srv/app".into(),
                },
                IncludeDisposition::Missing,
            ),
            (
                TargetPath::Symlink {
                    link_target: "/etc".into(),
                    real_path: "/etc".into(),
                },
                IncludeDisposition::OutsideRoot,
            ),
            (
                // 途中の要素が外を指す symlink で、末尾がない値
                TargetPath::Missing {
                    real_parent: "/srv/other".into(),
                },
                IncludeDisposition::OutsideRoot,
            ),
            (
                // 名前の途中で続くだけのパスは root_dir の下ではない
                TargetPath::File {
                    real_path: "/srv/apple".into(),
                },
                IncludeDisposition::OutsideRoot,
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(include_disposition(&path, &root()), expected, "{path:?}");
        }
    }
}
