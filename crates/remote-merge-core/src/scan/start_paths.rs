//! 走査の起点のパスを整える。exclude に当たるものと、ほかの起点の下にあるものを除く
use std::collections::HashSet;

use super::exclude::Exclude;
use super::outcome::Start;

/// 整えた起点
pub(super) struct StartPath {
    pub path: Vec<u8>,
    pub with_self: bool,
    pub descend: bool,
    pub group: usize,
    /// exclude に当たる指定したディレクトリ。下を読まず、ディレクトリなら器として載せるだけで、
    /// 届かない・辿れないことで走査を止めない（REQ-config-044）
    pub excluded: bool,
}

/// 起点のパスを整え、exclude に当たるもの（起点そのものだけを集める起点を除く）と、ほかの起点の下に
/// あるものを除く（同じパスなら自身も載せる起点を残す）。root_dir 全体を指す起点があれば全体の走査に
/// する。返す値の一つ目は全体の走査か
pub(super) fn start_paths(starts: &[Start], exclude: &Exclude) -> (bool, Vec<StartPath>) {
    let mut normal: Vec<StartPath> = starts
        .iter()
        .map(|start| StartPath {
            path: super::starts::normalize(&start.path).into_bytes(),
            with_self: start.with_self,
            descend: start.descend,
            group: start.group,
            excluded: false,
        })
        .collect();
    if normal.iter().any(|start| start.path.is_empty()) {
        return (true, Vec::new());
    }
    normal.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(b.descend.cmp(&a.descend))
            .then(b.with_self.cmp(&a.with_self))
    });
    let mut kept: Vec<StartPath> = Vec::new();
    // 残した起点のうち、下を集めるもののパス。下を集めない起点は、その下の起点を含まない
    let mut descending: HashSet<Vec<u8>> = HashSet::new();
    for start in normal {
        let covered = descending.contains(&start.path)
            || start
                .path
                .iter()
                .enumerate()
                .any(|(end, byte)| *byte == b'/' && descending.contains(&start.path[..end]));
        if covered {
            continue;
        }
        // 起点そのものだけを集める起点（名前で指定したパス）には exclude を当てない（REQ-cli-081）
        let itself = start.with_self && !start.descend;
        if itself || !exclude.excludes(&start.path) {
            if start.descend {
                descending.insert(start.path.clone());
            }
            kept.push(start);
        } else if !start.with_self {
            kept.push(StartPath {
                descend: false,
                excluded: true,
                ..start
            });
        }
    }
    (false, kept)
}
