use std::path::PathBuf;

use salsa::{Accumulator, Database};

use crate::{ast::File, diagnostic::Diagnostic, parser};

#[salsa::input(debug)]
pub struct Source {
    #[returns(ref)]
    pub text: String,
    #[returns(ref)]
    pub file_name: PathBuf,
}

#[salsa::tracked]
pub fn parse_file<'db>(db: &'db dyn Database, source: Source) -> File<'db> {
    let parser = parser::FileParser::new();
    match parser.parse(db, source, source.text(db)) {
        Ok(file) => file,
        Err(e) => {
            Diagnostic::parser_error(e, source).accumulate(db);
            File::new(db, vec![])
        }
    }
}

/// TODO: enable reporting string errors through the parser.
/// Now it will panic if this function doesn't succeed
pub fn unescape_json_string(s: &str) -> Result<String, String> {
    // Strip surrounding quotes
    let raw = &s[1..s.len() - 1];

    let mut result = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    // Expect 4 hex digits
                    let code: String = chars.by_ref().take(4).collect();
                    if code.len() == 4 {
                        if let Ok(num) = u16::from_str_radix(&code, 16) {
                            if let Some(ch) = char::from_u32(num as u32) {
                                result.push(ch);
                            } else {
                                return Err(format!("Invalid unicode escape: {}", code));
                            }
                        } else {
                            return Err(format!("Bad hex in unicode escape: {}", code));
                        }
                    } else {
                        return Err("Incomplete unicode escape".into());
                    }
                }
                Some(other) => return Err(format!("Invalid escape: \\{}", other)),
                None => return Err("Incomplete escape".into()),
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

pub fn parse_char_literal(s: &str) -> Result<u8, String> {
    // Expect format: `'x'` or `'\xNN'` or `'\n'`
    if !s.starts_with('\'') || !s.ends_with('\'') {
        return Err("invalid char literal".into());
    }

    let inner = &s[1..s.len() - 1];
    let bytes = inner.as_bytes();

    // Case 1: normal one-character literal: `'a'`
    if bytes.len() == 1 {
        return Ok(bytes[0]);
    }

    // Case 2: escaped literal: starts with '\'
    if bytes.len() >= 2 && bytes[0] == b'\\' {
        match bytes[1] {
            b'a' => return Ok(0x07),
            b'b' => return Ok(0x08),
            b'f' => return Ok(0x0C),
            b'n' => return Ok(0x0A),
            b'r' => return Ok(0x0D),
            b't' => return Ok(0x09),
            b'v' => return Ok(0x0B),
            b'\\' => return Ok(b'\\'),
            b'\'' => return Ok(b'\''),
            b'"' => return Ok(b'"'),
            b'?' => return Ok(b'?'),
            b'x' => {
                // \xNN (1–2 hex digits)
                let hex = &inner[2..];
                if hex.is_empty() || hex.len() > 2 {
                    return Err("invalid hex escape".into());
                }
                return u8::from_str_radix(hex, 16).map_err(|_| "invalid hex digits".to_string());
            }
            _ => return Err("unknown escape".into()),
        }
    }

    Err("invalid char literal format".into())
}
