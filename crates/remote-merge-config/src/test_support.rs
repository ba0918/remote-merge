//! config の単体テストが共有する部品。

use std::io::Write;

use tempfile::NamedTempFile;

pub(crate) fn write_temp_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}
