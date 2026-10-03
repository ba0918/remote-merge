//! left・right・referenceの存在と内容の比較。

/// 三つの比較対象のファイル単位の関係。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileComparison {
    AllEqual,
    Differs,
    ExistsOnlyInRef,
    MissingInRef,
}

/// 三つの比較対象の行単位の関係。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineComparison {
    AllEqual,
    Differs,
}

/// ファイルの存在と内容の同一性から三者の関係を求める。
///
/// - `left_exists`: leftにファイルが存在するか。
/// - `right_exists`: rightにファイルが存在するか。
/// - `ref_exists`: referenceにファイルが存在するか。
/// - `left_eq_right`: leftとrightの内容が同一か。
/// - `left_eq_ref`: leftとreferenceの内容が同一か。
pub fn compute_file_comparison(
    left_exists: bool,
    right_exists: bool,
    ref_exists: bool,
    left_eq_right: bool,
    left_eq_ref: bool,
) -> FileComparison {
    let all_exist = left_exists && right_exists && ref_exists;

    // ref にだけ存在しない
    if left_exists && right_exists && !ref_exists {
        return FileComparison::MissingInRef;
    }

    // ref にだけ存在する（left/right 両方にない）
    if !left_exists && !right_exists && ref_exists {
        return FileComparison::ExistsOnlyInRef;
    }

    // 存在差がある（上記以外のパターン）が ref が絡む
    if !all_exist {
        // ref があって片方だけにもある → 3way で差分あり
        if ref_exists {
            return FileComparison::Differs;
        }
        // ref がなくて left/right の片方だけ → 2way の情報だけで十分、3way バッジ不要
        return FileComparison::AllEqual;
    }

    // 全3サーバに存在 → 内容比較
    if left_eq_right && left_eq_ref {
        return FileComparison::AllEqual;
    }

    // どれかが違う → 3way で差分あり
    FileComparison::Differs
}

/// 行内容が三者とも同じならAllEqual、それ以外はDiffersを返す。
/// `None`は該当行が存在しないことを表す。
pub fn compute_line_comparison(
    left: Option<&str>,
    right: Option<&str>,
    ref_line: Option<&str>,
) -> LineComparison {
    match (left, right, ref_line) {
        (Some(l), Some(r), Some(rf)) if l == r && l == rf => LineComparison::AllEqual,
        (None, None, None) => LineComparison::AllEqual,
        _ => LineComparison::Differs,
    }
}
