//! 配置済みのバイナリと照合する、手元の Agent のバイナリの SHA-256（docs/ir/ssh/deploy.md の
//! REQ-ssh-016）

use std::path::{Path, PathBuf};

use crate::agent::deploy;

/// 手元の Agent のバイナリの SHA-256。解決できなければ照合しない（None）。解決できたのに読めない
/// ときは、照合せずに動かさず誤りにする
pub(super) fn local_agent_hash(
    resolved: anyhow::Result<PathBuf>,
    read: impl FnOnce(&Path) -> std::io::Result<Vec<u8>>,
) -> anyhow::Result<Option<String>> {
    let Ok(path) = resolved else {
        return Ok(None);
    };
    let bytes = read(&path).map_err(|error| {
        anyhow::anyhow!(
            "cannot read the local agent binary {}: {error}",
            path.display()
        )
    })?;
    Ok(Some(deploy::sha256_of_bytes(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // @kotowari[REQ-ssh-016]
    #[test]
    fn an_unresolved_binary_is_not_compared_and_an_unreadable_one_is_an_error() {
        let unresolved = local_agent_hash(Err(anyhow::anyhow!("none")), |_| unreachable!());
        assert_eq!(unresolved.unwrap(), None);

        let read = local_agent_hash(Ok(PathBuf::from("/a")), |_| Ok(b"abc".to_vec()));
        assert_eq!(read.unwrap(), Some(deploy::sha256_of_bytes(b"abc")));

        let unreadable = local_agent_hash(Ok(PathBuf::from("/agent-bin")), |_| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        });
        let message = unreadable.unwrap_err().to_string();
        assert!(message.contains("/agent-bin"), "{message}");
    }
}
