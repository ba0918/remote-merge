//! 走査のスクリプトの NUL で区切った出力を前から読む
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
