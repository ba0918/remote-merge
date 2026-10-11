//! --with-permissions で書き込み先に写す権限を、走査したツリーの読み込み元の値から決める。

use remote_merge_core::operations::MAX_PERMISSION_MODE;

/// 走査したツリーの読み込み元の権限 `scanned`（種類のビットを含んでよい）から、写す 0o777 の範囲の
/// 権限を返す。0 もそのまま写す。値がないときと、setuid・setgid・sticky を含むときは写さずに Err
pub fn permissions_to_copy(scanned: Option<u32>) -> anyhow::Result<u32> {
    let Some(mode) = scanned.map(|mode| mode & 0o7777) else {
        anyhow::bail!("permissions not copied: the source permissions are unknown");
    };
    if mode > MAX_PERMISSION_MODE {
        anyhow::bail!(
            "permissions not copied: the source mode {mode:o} has setuid, setgid or sticky bits"
        );
    }
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_source_permissions_are_not_copied() {
        assert!(permissions_to_copy(None).is_err());
    }

    #[test]
    fn a_mode_of_0_is_copied_as_0() {
        assert_eq!(permissions_to_copy(Some(0)).unwrap(), 0);
    }

    #[test]
    fn the_file_type_bits_of_a_scanned_mode_are_dropped() {
        assert_eq!(permissions_to_copy(Some(0o100600)).unwrap(), 0o600);
    }

    #[test]
    fn special_bits_are_not_copied() {
        assert!(permissions_to_copy(Some(0o104755)).is_err());
    }
}
