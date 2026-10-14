//! リモートの root_dir を利用者に見せる形と、root_dir がないときの誤り（docs/ir/scan/listing.md の
//! REQ-scan-015・017）

/// リモートの root_dir がないときの誤り（REQ-scan-015）
pub(crate) fn remote_root_missing(server_name: &str, root_dir: &std::path::Path) -> anyhow::Error {
    crate::error::AppError::RemoteRootNotFound {
        host: server_name.to_string(),
        path: root_dir.to_string_lossy().into_owned(),
    }
    .into()
}

/// 利用者に見せるリモートの root_dir。root_dir がないときの誤りと同じく "サーバ名:パス" にする
pub(crate) fn remote_shown_root(server_name: &str, root_dir: &std::path::Path) -> String {
    format!("{server_name}:{}", root_dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_merge_core::scan::outcome::{ScanError, ScanFailure};
    use remote_merge_engine::scan_result::into_listing;

    // @kotowari[REQ-scan-017]
    #[test]
    fn a_remote_root_dir_that_changed_during_the_scan_is_shown_with_its_server() {
        let root = std::path::Path::new("/srv/app");
        let failed = Err(ScanError::Failure(ScanFailure::RootChanged));
        let error = into_listing(&remote_shown_root("web", root), failed, || {
            remote_root_missing("web", root)
        })
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "root_dir changed during the scan: web:/srv/app"
        );
        let missing = remote_root_missing("web", root).to_string();
        assert!(missing.ends_with(": web:/srv/app"), "{missing}");
    }
}
