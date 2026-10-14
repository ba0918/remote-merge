/// パスの確認が止まった原因。表示は "cannot resolve path: " に原因の文言を続けたもの
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    PathTooLong,
    TooManyLinks,
    NotADirectory,
    PermissionDenied,
    DanglingSymlink,
    ParentReferenceInMissingPart,
    NonUtf8Path,
    CannotRead,
}

const ALL: [ResolveError; 8] = [
    ResolveError::PathTooLong,
    ResolveError::TooManyLinks,
    ResolveError::NotADirectory,
    ResolveError::PermissionDenied,
    ResolveError::DanglingSymlink,
    ResolveError::ParentReferenceInMissingPart,
    ResolveError::NonUtf8Path,
    ResolveError::CannotRead,
];

impl ResolveError {
    /// "cannot resolve path: " に続く原因の文言
    pub fn cause(self) -> &'static str {
        match self {
            Self::PathTooLong => "path too long",
            Self::TooManyLinks => "too many levels of symbolic links",
            Self::NotADirectory => "not a directory",
            Self::PermissionDenied => "permission denied",
            Self::DanglingSymlink => "dangling symbolic link",
            Self::ParentReferenceInMissingPart => "parent reference in missing part",
            Self::NonUtf8Path => "non-UTF-8 path",
            Self::CannotRead => "cannot read",
        }
    }

    /// 原因の文言から読み戻す。一覧にない文言は `None`
    pub fn from_cause(cause: &str) -> Option<Self> {
        ALL.into_iter().find(|error| error.cause() == cause)
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot resolve path: {}", self.cause())
    }
}

impl std::error::Error for ResolveError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cause_reads_back_from_its_wording_and_unknown_wording_does_not() {
        for error in ALL {
            assert_eq!(ResolveError::from_cause(error.cause()), Some(error));
        }
        assert_eq!(ResolveError::from_cause("no such file"), None);
    }
}
