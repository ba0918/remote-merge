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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_keeps_setuid_and_setgid_and_a_file_gets_the_mode_itself() {
        let cases = [
            (0o2775, 0o775, true, 0o2775),
            (0o4755, 0o700, true, 0o4700),
            (0o3775, 0o775, true, 0o2775),
            (0o1777, 0o755, true, 0o755),
            (0o2775, 0o775, false, 0o775),
            (0o4755, 0o644, false, 0o644),
        ];
        for (current, requested, directory, expected) in cases {
            let mode = PermissionMode::new(requested).expect("the requested mode is within 0o777");
            assert_eq!(
                mode.resulting(current, directory),
                expected,
                "{current:o} {requested:o} {directory}"
            );
        }
    }
}
