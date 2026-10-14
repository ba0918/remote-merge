use std::collections::VecDeque;

use super::{
    EntryKind, ExistingKind, NAME_LENGTH_LIMIT, PATH_LENGTH_LIMIT, PathOps, ResolveError,
    ResolvedPath, SYMLINK_LIMIT,
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

/// 実パス `real_parent` の下の相対パス `tail` を、[`resolve_path`] が存在しない部分を辿るのと
/// 同じ手順（".." を拒み、長さの上限を確かめる）で辿り、つないだ実パスを返す。
///
/// `tail` の最初の要素が `real_parent` の下にないことは、呼び出し側が別に分かっているものとし、
/// ここでは確かめない。親が存在するかも求めない。同じ場所の下の多くのパスを、
/// その場所を一度確かめるだけで確認するために使う
pub fn walk_missing_part(real_parent: &str, tail: &str) -> Result<String, ResolveError> {
    let mut real = real_parent.to_string();
    for component in components(tail, true) {
        real = step_in_missing_part(&real, &component.name)?;
    }
    Ok(real)
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
    // 最後に辿った要素の種類（"/" はディレクトリ）
    let mut kind = ExistingKind::Directory;
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
            EntryKind::Directory => {
                kind = ExistingKind::Directory;
                real = next;
            }
            found @ (EntryKind::RegularFile | EntryKind::Other) => {
                if rest > 0 {
                    return not_a_directory_before(
                        &expansions,
                        rest,
                        component.from_rel,
                        next,
                        found,
                    );
                }
                if component.slash_after {
                    return Err(ResolveError::NotADirectory);
                }
                kind = if found == EntryKind::RegularFile {
                    ExistingKind::File
                } else {
                    ExistingKind::Other
                };
                real = next;
            }
        }
    }

    Ok(match (terminal_link, missing_parent_exists) {
        (Some(link_target), parent_exists) => ResolvedPath::Symlink {
            link_target,
            real_path: real,
            target_kind: parent_exists.is_none().then_some(kind),
        },
        (None, Some(parent_exists)) => ResolvedPath::Missing {
            real_parent: parent(&real),
            parent_exists,
        },
        (None, None) => ResolvedPath::Exists {
            real_path: real,
            kind,
        },
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::wildcard_enum_match_arm,
        reason = "テストの照合は対象の種類を一つ書き、ほかは絞り込みで示す"
    )]
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

    /// 存在しない祖先の下や、存在する親の下の存在しない名前は、親の実パスと残りの要素だけで、
    /// 全体を辿ったのと同じ実パスかエラーになる
    #[test]
    fn a_missing_part_walks_to_the_same_place_as_the_whole_walk() {
        let entries = root_with(&[]);
        let long = "d".repeat(4096 - "/root/new/".len());
        let too_long_name = "n".repeat(NAME_LENGTH_LIMIT + 1);
        for tail in [
            "new/a.txt".to_string(),
            "new/sub/b.txt".to_string(),
            format!("new/{long}"),
            format!("new/{too_long_name}/c.txt"),
            "new/../x".to_string(),
            "a.txt".to_string(),
        ] {
            let Ok(ResolvedPath::Missing { real_parent, .. }) =
                resolve_path("/root", "new", &entries)
            else {
                panic!("new must be missing");
            };

            let walked = walk_missing_part(&real_parent, &tail).map(|real| parent(&real));
            let whole = resolve_path("/root", &tail, &entries).map(|resolved| match resolved {
                ResolvedPath::Missing { real_parent, .. } => real_parent,
                other => panic!("{tail} must be missing: {other:?}"),
            });
            assert_eq!(walked, whole, "{tail}");
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
