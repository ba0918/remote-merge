//! 権限の変更に渡す値（docs/ir/paths/operations.md の REQ-paths-015・REQ-paths-018）

/// 0o777 を超えるビットを持たない権限の値
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionMode(u32);

/// 0o777 を超える値（setuid・setgid・sticky を含む）を受け付けなかった
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("mode {0:#o} has bits beyond 0o777 (setuid, setgid and sticky are not set)")]
pub struct ModeTooWide(pub u32);

impl PermissionMode {
    pub fn new(mode: u32) -> Result<Self, ModeTooWide> {
        if mode & !0o777 == 0 {
            Ok(Self(mode))
        } else {
            Err(ModeTooWide(mode))
        }
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    /// 付ける権限。ディレクトリはすでに付いている setuid・setgid を残す
    pub fn resulting(self, current: u32, directory: bool) -> u32 {
        if directory {
            (current & 0o6000) | self.0
        } else {
            self.0
        }
    }
}
