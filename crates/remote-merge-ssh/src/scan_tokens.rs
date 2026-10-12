//! 走査のスクリプトの NUL で区切った出力を前から読む
use crate::scan_diagnostics::DirCheck;
use crate::scan_fields::Record;

/// NUL で区切った出力を前から読む
pub(crate) struct Tokens<'a> {
    pub(crate) rest: std::slice::Split<'a, u8, fn(&u8) -> bool>,
}

impl<'a> Tokens<'a> {
    pub(crate) fn next(&mut self) -> Result<&'a [u8], String> {
        self.rest
            .next()
            .ok_or_else(|| "The scan output ended early".to_string())
    }

    pub(crate) fn expect(&mut self, tag: &[u8]) -> Result<(), String> {
        let token = self.next()?;
        if token == tag {
            return Ok(());
        }
        Err(format!(
            "Unexpected scan output: {:?} instead of {:?}",
            String::from_utf8_lossy(token),
            String::from_utf8_lossy(tag)
        ))
    }

    /// ディレクトリを確かめた結果を一つ読む。終わりの目印なら None
    pub(crate) fn check(&mut self) -> Result<Option<DirCheck>, String> {
        match self.next()? {
            b"@CE" => return Ok(None),
            b"@D" => {}
            other => return Err(unexpected(other)),
        }
        let printed = self.next()?.to_vec();
        let (readable, searchable) = match self.next()? {
            b"11" => (true, true),
            b"10" => (true, false),
            b"01" => (false, true),
            b"00" => (false, false),
            other => return Err(unexpected(other)),
        };
        let mut names = Vec::new();
        loop {
            match self.next()? {
                b"@E" => break,
                name => names.push(
                    name.strip_prefix(b"N")
                        .ok_or_else(|| unexpected(name))?
                        .to_vec(),
                ),
            }
        }
        Ok(Some(DirCheck {
            printed,
            readable,
            searchable,
            names,
        }))
    }

    /// 項目の 8 つの欄。`first` は先に読んだ最初の欄
    pub(crate) fn record(&mut self, first: &'a [u8]) -> Result<Record<'a>, String> {
        Ok(Record {
            kind: first,
            dev: self.next()?,
            ino: self.next()?,
            time: self.next()?,
            mode: self.next()?,
            size: self.next()?,
            link: self.next()?,
            path: self.next()?,
        })
    }
}

pub(crate) fn unexpected(token: &[u8]) -> String {
    format!("Unexpected scan output: {}", String::from_utf8_lossy(token))
}
