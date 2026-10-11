use std::collections::VecDeque;

use super::{
    EntryKind, PathOps, ResolveError, ResolvedPath, NAME_LENGTH_LIMIT, PATH_LENGTH_LIMIT,
    SYMLINK_LIMIT,
};

/// まだ辿っていない一つの要素
struct Component {
    name: String,
    /// 元の文字列でこの要素の後に "/" があった
    slash_after: bool,
    /// 確認する相対パスの要素（root_dir の要素とリンク先の要素は false）
    from_rel: bool,
}

/// 文字列を要素に分ける。空の要素と "." は捨てる
fn components(path: &str, from_rel: bool) -> Vec<Component> {
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.len() - 1;
    segments
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty() && **name != ".")
        .map(|(index, name)| Component {
            name: (*name).to_string(),
            slash_after: index < last,
            from_rel,
        })
        .collect()
}

/// 辿っている途中の symlink のリンク先。`rest` は差し込んだときに後ろに残っていた要素の数で、
/// 取り出した要素の後ろに `rest` 個以上が残っている間、その要素はこのリンク先に属する
struct Expansion {
    rest: usize,
    /// 差し込んだ symlink が確認する相対パスの要素だった
    from_rel: bool,
}

fn joined(real: &str, name: &str) -> String {
    if real == "/" {
        format!("/{name}")
    } else {
        format!("{real}/{name}")
    }
}

fn parent(real: &str) -> String {
    match real.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => real[..index].to_string(),
    }
}

/// 後ろにまだ要素が残っているのにディレクトリでない要素 `real_path` に出会ったときの結論。
/// 辿っている途中のリンク先の要素がまだ残っていればリンク先を辿り切れないのでエラーにし、
/// そうでなければ、その要素を生んだ確認するパスの要素が相対パスのものなら途中の祖先が
/// ディレクトリでないという結果、root_dir のものならエラーにする
fn not_a_directory_before(
    expansions: &[Expansion],
    rest: usize,
    from_rel: bool,
    real_path: String,
    kind: EntryKind,
) -> Result<ResolvedPath, ResolveError> {
    if expansions.iter().any(|expansion| expansion.rest < rest) {
        return Err(ResolveError::NotADirectory);
    }
    let origin_from_rel = expansions
        .first()
        .map_or(from_rel, |expansion| expansion.from_rel);
    if !origin_from_rel {
        return Err(ResolveError::NotADirectory);
    }
    Ok(ResolvedPath::AncestorNotDirectory {
        real_path,
        regular_file: kind == EntryKind::RegularFile,
    })
}

/// `real` に要素 `name` をつないだ実パス。長すぎれば辿らずに止める
fn joined_within_limits(real: &str, name: &str) -> Result<String, ResolveError> {
    let next = joined(real, name);
    if name.len() > NAME_LENGTH_LIMIT || next.len() >= PATH_LENGTH_LIMIT {
        return Err(ResolveError::PathTooLong);
    }
    Ok(next)
}

/// 存在しない要素より後の一つの要素を辿る。そこには何もないので、".." を拒み、長さの上限だけを確かめる
fn step_in_missing_part(real: &str, name: &str) -> Result<String, ResolveError> {
    if name == ".." {
        return Err(ResolveError::ParentReferenceInMissingPart);
    }
    joined_within_limits(real, name)
}

/// 存在しないと分かった要素から先を、[`resolve_path`] が存在しない部分を辿るのと同じ手順で辿る。
///
/// `real_parent` はその要素の親の実パス（その要素を確かめた [`ResolvedPath::Missing`] の
/// `real_parent`）、`tail` はその要素から始まる相対パスで、要素を二つ以上持つ。
/// 同じ祖先の下の多くのパスを、祖先を一度確かめるだけで確認するために使う
pub fn resolve_below_missing(real_parent: &str, tail: &str) -> Result<ResolvedPath, ResolveError> {
    let mut real = real_parent.to_string();
    for component in components(tail, true) {
        real = step_in_missing_part(&real, &component.name)?;
    }
    Ok(ResolvedPath::Missing {
        real_parent: parent(&real),
        parent_exists: false,
    })
}

/// root_dir（絶対パス）と確認する相対パス `rel` をつないだパスを "/" から要素ごとに辿る
pub fn resolve_path(
    root: &str,
    rel: &str,
    ops: &impl PathOps,
) -> Result<ResolvedPath, ResolveError> {
    let mut queue: VecDeque<Component> = components(root, false).into();
    let rel_components = components(rel, true);
    if let (Some(last), false) = (queue.back_mut(), rel_components.is_empty()) {
        last.slash_after = true;
    }
    queue.extend(rel_components);

    let mut real = "/".to_string();
    let mut expansions: Vec<Expansion> = Vec::new();
    let mut terminal_link = None;
    let mut inserted_links = 0;
    let mut missing_parent_exists = None;
    while let Some(component) = queue.pop_front() {
        let rest = queue.len();
        expansions.retain(|expansion| expansion.rest <= rest);
        if missing_parent_exists.is_some() {
            real = step_in_missing_part(&real, &component.name)?;
            continue;
        }
        if component.name == ".." {
            real = parent(&real);
            continue;
        }
        let next = joined_within_limits(&real, &component.name)?;
        if !ops.can_search(&real)? {
            return Err(ResolveError::PermissionDenied);
        }
        match ops.entry_kind(&next)? {
            EntryKind::Missing => {
                // 後ろに要素が残っていた symlink（途中の祖先）のリンク先は辿り切れない
                if expansions
                    .last()
                    .is_some_and(|expansion| expansion.rest > 0)
                {
                    return Err(ResolveError::DanglingSymlink);
                }
                missing_parent_exists = Some(rest == 0);
                real = next;
            }
            EntryKind::Symlink => {
                if inserted_links == SYMLINK_LIMIT {
                    return Err(ResolveError::TooManyLinks);
                }
                inserted_links += 1;
                let target = String::from_utf8(ops.read_link(&next)?)
                    .map_err(|_| ResolveError::NonUtf8Path)?;
                if component.from_rel && rest == 0 {
                    terminal_link = Some(target.clone());
                }
                if target.starts_with('/') {
                    real = "/".to_string();
                }
                expansions.push(Expansion {
                    rest,
                    from_rel: component.from_rel,
                });
                let mut target_components = components(&target, false);
                // リンクの後の "/" はリンク先を辿った先にディレクトリを求める
                if let Some(last) = target_components.last_mut() {
                    last.slash_after |= component.slash_after;
                }
                for target_component in target_components.into_iter().rev() {
                    queue.push_front(target_component);
                }
            }
            EntryKind::Directory => real = next,
            kind @ (EntryKind::RegularFile | EntryKind::Other) => {
                if rest > 0 {
                    return not_a_directory_before(
                        &expansions,
                        rest,
                        component.from_rel,
                        next,
                        kind,
                    );
                }
                if component.slash_after {
                    return Err(ResolveError::NotADirectory);
                }
                real = next;
            }
        }
    }

    Ok(match (terminal_link, missing_parent_exists) {
        (Some(link_target), _) => ResolvedPath::Symlink {
            link_target,
            real_path: real,
        },
        (None, Some(parent_exists)) => ResolvedPath::Missing {
            real_parent: parent(&real),
            parent_exists,
        },
        (None, None) => ResolvedPath::Exists { real_path: real },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// 実パスごとの種類だけを持つファイルシステム。どのディレクトリも検索できる
    struct Entries(HashMap<&'static str, Result<EntryKind, ResolveError>>);

    impl PathOps for Entries {
        fn entry_kind(&self, path: &str) -> Result<EntryKind, ResolveError> {
            self.0.get(path).copied().unwrap_or(Ok(EntryKind::Missing))
        }
        fn can_search(&self, _: &str) -> Result<bool, ResolveError> {
            Ok(true)
        }
        fn read_link(&self, _: &str) -> Result<Vec<u8>, ResolveError> {
            Err(ResolveError::CannotRead)
        }
    }

    fn root_with(entries: &[(&'static str, Result<EntryKind, ResolveError>)]) -> Entries {
        let mut map: HashMap<_, _> = entries.iter().copied().collect();
        map.insert("/root", Ok(EntryKind::Directory));
        Entries(map)
    }

    /// 存在しない祖先の下は、祖先の結果と残りの要素だけで、全体を辿ったのと同じ結果になる
    #[test]
    fn below_a_missing_ancestor_gives_the_same_result_as_the_whole_walk() {
        let entries = root_with(&[]);
        let long = "d".repeat(4096 - "/root/new/".len());
        let too_long_name = "n".repeat(NAME_LENGTH_LIMIT + 1);
        for tail in [
            "new/a.txt".to_string(),
            "new/sub/b.txt".to_string(),
            format!("new/{long}"),
            format!("new/{too_long_name}/c.txt"),
            "new/../x".to_string(),
        ] {
            let Ok(ResolvedPath::Missing { real_parent, .. }) =
                resolve_path("/root", "new", &entries)
            else {
                panic!("new must be missing");
            };

            assert_eq!(
                resolve_below_missing(&real_parent, &tail),
                resolve_path("/root", &tail, &entries),
                "{tail}"
            );
        }
    }

    #[test]
    fn an_ancestor_that_is_neither_a_directory_nor_a_regular_file_is_reported_as_such() {
        let entries = root_with(&[("/root/fifo", Ok(EntryKind::Other))]);

        assert_eq!(
            resolve_path("/root", "fifo/a.txt", &entries),
            Ok(ResolvedPath::AncestorNotDirectory {
                real_path: "/root/fifo".into(),
                regular_file: false,
            })
        );
    }

    #[test]
    fn a_failure_to_examine_an_element_stops_with_cannot_read() {
        let entries = root_with(&[("/root/dir", Err(ResolveError::CannotRead))]);

        assert_eq!(
            resolve_path("/root", "dir/a.txt", &entries),
            Err(ResolveError::CannotRead)
        );
    }
}
