//! rollback が書き戻さずに飛ばす理由（docs/ir/backup/rollback-path.md）。分岐は型で行い、利用者に見せる
//! 文言はここの Display 一か所で作る

/// 書き戻さずに飛ばす理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 書き戻す先が、記録した実パスと別の場所に辿り着く
    ResolvesElsewhere,
    /// 置き換えた symlink が merge の後に変わった
    SymlinkChanged,
    /// symlink の書き戻しはしない
    SymlinkRestoreNotSupported,
    /// 書き戻す先の親ディレクトリがもうない
    ParentGone,
}

impl SkipReason {
    /// 書き戻しの要求そのものを拒む理由か（同じ記録のファイルをまとめて飛ばす）
    pub fn refuses_the_request(self) -> bool {
        matches!(self, Self::ResolvesElsewhere | Self::SymlinkChanged)
    }
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ResolvesElsewhere => "path now resolves to a different location",
            Self::SymlinkChanged => "symlink changed after merge",
            Self::SymlinkRestoreNotSupported => "symlink restore not supported",
            Self::ParentGone => "parent directory no longer exists",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 利用者に見せる文言は IR で決めたもの（REQ-backup-031 など）のまま
    #[test]
    fn each_reason_is_shown_with_its_fixed_wording() {
        assert_eq!(
            SkipReason::ResolvesElsewhere.to_string(),
            "path now resolves to a different location"
        );
        assert_eq!(
            SkipReason::SymlinkChanged.to_string(),
            "symlink changed after merge"
        );
        assert_eq!(
            SkipReason::SymlinkRestoreNotSupported.to_string(),
            "symlink restore not supported"
        );
        assert_eq!(
            SkipReason::ParentGone.to_string(),
            "parent directory no longer exists"
        );
    }

    #[test]
    fn only_a_changed_location_or_link_refuses_the_whole_request() {
        assert!(SkipReason::ResolvesElsewhere.refuses_the_request());
        assert!(SkipReason::SymlinkChanged.refuses_the_request());
        assert!(!SkipReason::SymlinkRestoreNotSupported.refuses_the_request());
        assert!(!SkipReason::ParentGone.refuses_the_request());
    }
}
