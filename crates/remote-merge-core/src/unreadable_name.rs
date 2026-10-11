//! UTF-8 として読めない名前を、置換文字に置き換えずに示す形にする（docs/ir/scan/listing.md の
//! REQ-scan-012）。
//!
//! 走査は、読めない名前のファイルを一覧に載せず、この形の名前で報告する。読めない部分のバイトを
//! `\xNN` と書くので、名前の違うファイルが同じ形にならない（読める部分にもとから `\x` を含む
//! 名前を除く）。

use std::fmt::Write as _;

/// `bytes` の UTF-8 として読める部分はそのまま、読めないバイトは `\xNN` と書いた文字列
pub fn escape_non_utf8(bytes: &[u8]) -> String {
    let mut escaped = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        escaped.push_str(chunk.valid());
        for byte in chunk.invalid() {
            let _ = write!(escaped, "\\x{byte:02x}");
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_parts_are_kept_and_unreadable_bytes_are_written_as_hex() {
        assert_eq!(
            escape_non_utf8(b"dir/bad\xff\xfe.txt"),
            "dir/bad\\xff\\xfe.txt"
        );
        assert_eq!(escape_non_utf8("日本.txt".as_bytes()), "日本.txt");
    }

    #[test]
    fn a_cut_multibyte_character_is_written_as_its_bytes() {
        assert_eq!(escape_non_utf8(b"a\xe6\x97"), "a\\xe6\\x97");
    }
}
