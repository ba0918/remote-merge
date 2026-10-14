//! 権限の値の規則（付けてよい値の上限と、ディレクトリの権限を変えた後の値）

/// 今の権限 `current` のものを `requested` に変えた後の権限。ディレクトリはすでに付いている
/// setuid・setgid を残し、0o777 の範囲のビットと sticky を `requested` にする（SSH の経路の
/// coreutils の chmod と同じ）。ファイルは `requested` そのものにする
pub fn resulting_mode(current: u32, requested: u32, directory: bool) -> u32 {
    if directory {
        (current & 0o6000) | (requested & 0o1777)
    } else {
        requested
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
            assert_eq!(
                resulting_mode(current, requested, directory),
                expected,
                "{current:o} {requested:o} {directory}"
            );
        }
    }
}
