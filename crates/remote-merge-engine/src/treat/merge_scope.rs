//! merge と sync の走査の範囲（docs/ir/scan/limits.md の TBL-scan-001）と、名前で指定したパスのうち
//! 扱うもの（docs/ir/merge/basic.md の REQ-merge-021・022）

use remote_merge_core::relative_path::RelPath;

/// 指定したパスの数がこれ以上なら root_dir 全体を走査する
const MANY_PATHS: usize = 21;

/// 走査の範囲
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeScope {
    /// root_dir 全体
    Whole,
    /// それぞれのディレクトリの下だけ（件数の上限はディレクトリごと）
    Below(Vec<String>),
}

/// 走査の範囲と、扱う名前
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeTargets {
    pub scope: MergeScope,
    /// 名前で指定したパス（"." の要素と末尾の "/" を除き、重ねないもの。指定した順）。root_dir 全体を
    /// 指したときは空
    pub named: Vec<String>,
}

impl MergeTargets {
    /// `path` を扱うか（名前で指定したパスそのものか、その下）
    pub fn selects(&self, path: &str) -> bool {
        self.named.is_empty() || self.named.iter().any(|name| within(path, name))
    }
}

/// パスの引数から、走査の範囲と扱う名前を決める
pub fn merge_targets(paths: &[RelPath], delete: bool) -> MergeTargets {
    let whole = || MergeTargets {
        scope: MergeScope::Whole,
        named: Vec::new(),
    };
    if paths
        .iter()
        .any(|path| normalized(path.as_typed()).is_empty())
    {
        return whole();
    }
    let mut named: Vec<String> = Vec::new();
    for path in paths {
        let normal = normalized(path.as_typed());
        if !named.contains(&normal) {
            named.push(normal);
        }
    }
    let with_slash = paths
        .iter()
        .filter(|path| path.as_typed().ends_with('/'))
        .count();
    let glob = paths
        .iter()
        .any(|path| path.as_typed().contains(['*', '?', '[']));
    let mixed = with_slash != 0 && with_slash != paths.len();
    if delete || glob || mixed || paths.len() >= MANY_PATHS {
        return MergeTargets {
            scope: MergeScope::Whole,
            named,
        };
    }
    let dirs: Vec<String> = if with_slash == paths.len() {
        named.clone()
    } else {
        named.iter().map(|name| parent(name).to_string()).collect()
    };
    if dirs.iter().any(String::is_empty) {
        return MergeTargets {
            scope: MergeScope::Whole,
            named,
        };
    }
    let mut below: Vec<String> = Vec::new();
    for dir in dirs {
        if !below.contains(&dir) {
            below.push(dir);
        }
    }
    MergeTargets {
        scope: MergeScope::Below(below),
        named,
    }
}

/// `path` が `name` そのものか、その下にあるか（"/" の区切りの単位で）
fn within(path: &str, name: &str) -> bool {
    path == name
        || path
            .strip_prefix(name)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// 親ディレクトリ（root_dir の直下なら空）
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// "." の要素と空の要素を除いたパス
fn normalized(path: &str) -> String {
    path.split('/')
        .filter(|element| !element.is_empty() && *element != ".")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
#[path = "merge_scope_tests.rs"]
mod tests;
