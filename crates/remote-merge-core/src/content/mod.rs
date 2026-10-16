use std::{borrow::Cow, ops::Range, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    EucJp,
    ShiftJis,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::EucJp => "euc-jp",
            Self::ShiftJis => "shift_jis",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Interpretation {
    #[default]
    Unspecified,
    Binary,
    Text(Encoding),
}

impl FromStr for Interpretation {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name.to_ascii_lowercase().as_str() {
            "utf-8" => Ok(Self::Text(Encoding::Utf8)),
            "euc-jp" => Ok(Self::Text(Encoding::EucJp)),
            "shift_jis" | "cp932" | "windows-31j" => Ok(Self::Text(Encoding::ShiftJis)),
            "binary" => Ok(Self::Binary),
            _ => Err(format!("unsupported content encoding: {name}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid {encoding_name} content", encoding_name = .encoding.name())]
pub struct DecodeError {
    pub encoding: Encoding,
}

#[derive(Debug)]
pub enum Content<'a> {
    Binary,
    Text(Text<'a>),
}

#[derive(Debug)]
pub struct Text<'a> {
    raw: &'a [u8],
    decoded: Cow<'a, str>,
    encoding: Encoding,
    bom_len: usize,
    raw_lines: Vec<Range<usize>>,
    text_lines: Vec<Range<usize>>,
}

impl Text<'_> {
    pub fn text(&self) -> &str {
        &self.decoded
    }
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    pub fn bom(&self) -> &[u8] {
        &self.raw[..self.bom_len]
    }
    pub fn raw(&self) -> &[u8] {
        self.raw
    }
    pub fn raw_line(&self, index: usize) -> &[u8] {
        &self.raw[self.raw_lines[index].clone()]
    }
    pub fn line(&self, index: usize) -> &str {
        &self.decoded[self.text_lines[index].clone()]
    }
    pub fn line_count(&self) -> usize {
        self.raw_lines.len()
    }
}

fn line_ranges(bytes: &[u8], offset: usize) -> Vec<Range<usize>> {
    let mut start = offset;
    let mut ranges = Vec::new();
    for (index, byte) in bytes.iter().enumerate().skip(offset) {
        // similar treats a lone CR as a line ending too; LF-only spans misalign its indices.
        if *byte == b'\n' || (*byte == b'\r' && bytes.get(index + 1) != Some(&b'\n')) {
            ranges.push(start..index + 1);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        ranges.push(start..bytes.len());
    }
    ranges
}

pub fn interpret(raw: &[u8], interpretation: Interpretation) -> Result<Content<'_>, DecodeError> {
    if interpretation == Interpretation::Binary || raw[..raw.len().min(8192)].contains(&0) {
        return Ok(Content::Binary);
    }
    let encoding = match interpretation {
        Interpretation::Unspecified => Encoding::Utf8,
        Interpretation::Text(encoding) => encoding,
        Interpretation::Binary => return Ok(Content::Binary),
    };
    let bom_len = if encoding == Encoding::Utf8 && raw.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    let body = &raw[bom_len..];
    // BOM sniffing and replacement would silently change the explicitly chosen interpretation.
    let decoded = match encoding {
        Encoding::Utf8 => std::str::from_utf8(body).ok().map(Cow::Borrowed),
        Encoding::EucJp => {
            encoding_rs::EUC_JP.decode_without_bom_handling_and_without_replacement(body)
        }
        Encoding::ShiftJis => {
            encoding_rs::SHIFT_JIS.decode_without_bom_handling_and_without_replacement(body)
        }
    };
    let Some(decoded) = decoded else {
        return if interpretation == Interpretation::Unspecified {
            Ok(Content::Binary)
        } else {
            Err(DecodeError { encoding })
        };
    };
    let raw_lines = line_ranges(raw, bom_len);
    let text_lines = line_ranges(decoded.as_bytes(), 0);
    Ok(Content::Text(Text {
        raw,
        decoded,
        encoding,
        bom_len,
        raw_lines,
        text_lines,
    }))
}
