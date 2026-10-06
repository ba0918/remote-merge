//! パスの確認のスクリプトの出力の読み取り。出力は信頼しない入力として扱い、知らない種類・
//! 一覧にない原因の文言・欠けた欄・余った欄はエラーにする。

use remote_merge_core::path_resolution::{ResolveError, ResolvedPath};

/// NUL で終わる欄を順に取り出す
struct Fields<'a> {
    rest: &'a [u8],
}

impl<'a> Fields<'a> {
    fn next_bytes(&mut self) -> anyhow::Result<Option<&'a [u8]>> {
        if self.rest.is_empty() {
            return Ok(None);
        }
        let end = self
            .rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| anyhow::anyhow!("path inspection output is not NUL-terminated"))?;
        let field = &self.rest[..end];
        self.rest = &self.rest[end + 1..];
        Ok(Some(field))
    }

    fn required(&mut self) -> anyhow::Result<&'a [u8]> {
        self.next_bytes()?
            .ok_or_else(|| anyhow::anyhow!("path inspection output is missing a field"))
    }

    /// 欄を文字列として読む。UTF-8 でなければローカルと同じく "non-UTF-8 path" で止める
    fn text(&mut self) -> anyhow::Result<String> {
        let field = self.required()?;
        Ok(std::str::from_utf8(field)
            .map_err(|_| ResolveError::NonUtf8Path)?
            .to_string())
    }

    fn path(&mut self) -> anyhow::Result<String> {
        let path = self.text()?;
        if !path.starts_with('/') {
            anyhow::bail!("path inspection returned a path that is not absolute: {path}");
        }
        Ok(path)
    }
}

/// スクリプトの出力を確認の結果に読む。辿れなかった原因は [`ResolveError`] のエラーにする
pub fn parse_inspect_path_output(output: &[u8]) -> anyhow::Result<ResolvedPath> {
    let mut fields = Fields { rest: output };
    let result = loop {
        let kind = fields.required()?;
        match kind {
            // 辿り始める root_dir。相対の root_dir は作業ディレクトリにつないだもので、ローカルと
            // エージェントは辿る前に UTF-8 でない root_dir で止まる
            b"root" => {
                fields.path()?;
            }
            // 読んだリンク文字列。ローカルは UTF-8 でないリンク文字列を読んだところで止まる
            b"link" => {
                fields.text()?;
            }
            b"exists" => {
                break ResolvedPath::Exists {
                    real_path: fields.path()?,
                }
            }
            b"symlink" => {
                break ResolvedPath::Symlink {
                    link_target: fields.text()?,
                    real_path: fields.path()?,
                }
            }
            b"missing" => {
                let real_parent = fields.path()?;
                let parent_exists = match fields.required()? {
                    b"1" => true,
                    b"0" => false,
                    other => anyhow::bail!(
                        "unknown parent marker in path inspection output: {}",
                        String::from_utf8_lossy(other)
                    ),
                };
                break ResolvedPath::Missing {
                    real_parent,
                    parent_exists,
                };
            }
            b"notdir" => {
                let real_path = fields.path()?;
                let regular_file = match fields.required()? {
                    b"regular" => true,
                    b"other" => false,
                    other => anyhow::bail!(
                        "unknown file kind in path inspection output: {}",
                        String::from_utf8_lossy(other)
                    ),
                };
                break ResolvedPath::AncestorNotDirectory {
                    real_path,
                    regular_file,
                };
            }
            b"error" => {
                let cause = fields.text()?;
                ensure_no_more(&mut fields)?;
                return Err(match ResolveError::from_cause(&cause) {
                    Some(error) => error.into(),
                    None => anyhow::anyhow!("unknown path inspection cause: {cause}"),
                });
            }
            other => anyhow::bail!(
                "unknown path inspection output: {}",
                String::from_utf8_lossy(other)
            ),
        }
    };
    ensure_no_more(&mut fields)?;
    Ok(result)
}

fn ensure_no_more(fields: &mut Fields<'_>) -> anyhow::Result<()> {
    match fields.next_bytes()? {
        None => Ok(()),
        Some(_) => anyhow::bail!("path inspection output has extra fields"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cause(output: &[u8]) -> Option<ResolveError> {
        parse_inspect_path_output(output)
            .unwrap_err()
            .downcast_ref::<ResolveError>()
            .copied()
    }

    #[test]
    fn a_listed_cause_reads_back_as_the_resolution_error() {
        assert_eq!(
            cause(b"link\0loop\0error\0too many levels of symbolic links\0"),
            Some(ResolveError::TooManyLinks)
        );
    }

    #[test]
    fn a_link_string_that_is_not_utf_8_stops_with_non_utf_8_path() {
        assert_eq!(
            cause(b"link\0name\xff\0error\0dangling symbolic link\0"),
            Some(ResolveError::NonUtf8Path)
        );
    }

    #[test]
    fn unlisted_causes_unknown_kinds_and_broken_fields_are_errors_without_a_cause() {
        for output in [
            b"error\0No such file or directory\0".as_slice(),
            b"file\0/srv/a\0",
            b"exists\0",
            b"exists\0/srv/a",
            b"exists\0/srv/a\0extra\0",
            b"missing\0/srv\0yes\0",
            b"exists\0relative\0",
            b"",
        ] {
            assert_eq!(cause(output), None, "{}", String::from_utf8_lossy(output));
        }
    }

    #[test]
    fn every_kind_reads_back() {
        assert_eq!(
            parse_inspect_path_output(b"missing\0/srv/app/missing\x000\0").unwrap(),
            ResolvedPath::Missing {
                real_parent: "/srv/app/missing".into(),
                parent_exists: false,
            }
        );
        assert_eq!(
            parse_inspect_path_output(b"link\0../b\0symlink\0../b\0/srv/b\0").unwrap(),
            ResolvedPath::Symlink {
                link_target: "../b".into(),
                real_path: "/srv/b".into(),
            }
        );
        assert_eq!(
            parse_inspect_path_output(b"notdir\0/srv/app/dir\0other\0").unwrap(),
            ResolvedPath::AncestorNotDirectory {
                real_path: "/srv/app/dir".into(),
                regular_file: false,
            }
        );
    }
}
