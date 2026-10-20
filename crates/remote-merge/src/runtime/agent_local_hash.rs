//! 配置済みのバイナリと照合する、手元の Agent のバイナリの SHA-256（docs/ir/ssh/deploy.md の
//! REQ-ssh-016）

use std::io::Read;
use std::path::{Path, PathBuf};

/// 手元の Agent のバイナリの SHA-256。解決できなければ照合しない（None）。解決できたのに読めない
/// ときは、照合せずに動かさず誤りにする
pub(super) fn local_agent_hash<R: Read>(
    resolved: anyhow::Result<PathBuf>,
    open: impl FnOnce(&Path) -> std::io::Result<R>,
) -> anyhow::Result<Option<String>> {
    let Ok(path) = resolved else {
        return Ok(None);
    };
    let hash = open(&path)
        .and_then(remote_merge_core::sha256::hex_of_reader)
        .map_err(|error| {
            anyhow::anyhow!(
                "cannot read the local agent binary {}: {error}",
                path.display()
            )
        })?;
    Ok(Some(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::deploy;

    #[test]
    fn empty_and_multi_buffer_binaries_have_the_same_digest_as_their_contents() {
        for bytes in [Vec::new(), vec![0x5a; 128 * 1024 + 17]] {
            let hash = local_agent_hash(Ok(PathBuf::from("agent-bin")), |_| {
                Ok(std::io::Cursor::new(&bytes))
            })
            .unwrap();
            assert_eq!(hash, Some(deploy::sha256_of_bytes(&bytes)));
        }
    }

    #[test]
    fn a_read_error_after_some_binary_bytes_keeps_the_path_and_cause() {
        struct InterruptedBinary;
        impl std::io::Read for InterruptedBinary {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("binary read stopped"))
            }
        }
        let error = local_agent_hash(Ok(PathBuf::from("agent-bin")), |_| {
            Ok(std::io::Read::chain(&b"partial"[..], InterruptedBinary))
        })
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "cannot read the local agent binary agent-bin: binary read stopped"
        );
    }

    // @kotowari[REQ-ssh-016]
    #[test]
    fn an_unresolved_binary_is_not_compared_and_an_unreadable_one_is_an_error() {
        let unresolved =
            local_agent_hash::<&[u8]>(Err(anyhow::anyhow!("none")), |_| unreachable!());
        assert_eq!(unresolved.unwrap(), None);

        let read = local_agent_hash(Ok(PathBuf::from("/a")), |_| Ok(&b"abc"[..]));
        assert_eq!(read.unwrap(), Some(deploy::sha256_of_bytes(b"abc")));

        let unreadable = local_agent_hash::<&[u8]>(Ok(PathBuf::from("/agent-bin")), |_| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        });
        let message = unreadable.unwrap_err().to_string();
        assert!(message.contains("/agent-bin"), "{message}");
    }
}
