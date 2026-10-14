//! コマンドの入口のパスの引数の検査と、指定したパスがどの側にもないときの誤り。I/O なし

use remote_merge_core::relative_path::RelPath;

/// コマンドのパスの引数を、入力の形を検査して root_dir からの相対パスにする。最初に検査を
/// 通らなかった値のエラーで止める
pub fn parse_path_arguments(paths: &[String]) -> anyhow::Result<Vec<RelPath>> {
    paths
        .iter()
        .map(|path| RelPath::parse(path).map_err(anyhow::Error::from))
        .collect()
}

/// 指定したパスがすべて左右のどちらにもなく、書き込まずに止まるときの誤り（文言は diff・merge・sync
/// で同じにするので一か所に置く）
pub fn paths_not_found() -> anyhow::Error {
    anyhow::anyhow!("specified path(s) not found on either side")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_arguments_are_returned_checked_in_the_order_given() {
        let paths = parse_path_arguments(&["b/".to_string(), "a.txt".to_string()]).unwrap();
        let typed: Vec<&str> = paths.iter().map(|path| path.as_typed()).collect();
        assert_eq!(typed, ["b/", "a.txt"]);
    }

    #[test]
    fn path_arguments_naming_the_root_dir_are_accepted() {
        for path in ["/", "//", ".", "./", ""] {
            assert!(
                parse_path_arguments(&[path.to_string()]).is_ok(),
                "{path:?} was rejected"
            );
        }
    }

    #[test]
    fn path_arguments_inside_the_root_dir_are_accepted() {
        for path in ["a.txt", "dir/", "dir/a.txt", "a..b.txt", "./a.txt"] {
            assert!(
                parse_path_arguments(&[path.to_string()]).is_ok(),
                "{path:?} was rejected"
            );
        }
    }

    #[test]
    fn absolute_path_arguments_are_rejected_as_typed() {
        for path in ["/etc/passwd", "/etc/", "/."] {
            let error = parse_path_arguments(&["a.txt".to_string(), path.to_string()])
                .expect_err(path)
                .to_string();
            assert_eq!(error, format!("absolute subpath not allowed: {path}"));
        }
    }

    #[test]
    fn path_arguments_with_a_parent_component_are_rejected_as_typed() {
        for path in ["..", "../b.txt", "a/../b.txt", "a/..", "a/../"] {
            let error = parse_path_arguments(&[path.to_string()])
                .expect_err(path)
                .to_string();
            assert_eq!(error, format!("path traversal not allowed: {path}"));
        }
    }
}
